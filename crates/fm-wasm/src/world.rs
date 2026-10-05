//! The world, living in the world worker (spec Fase 7B, 7B.4): the only
//! `World` of the page. The worker drives it one match at a time — `play`
//! only reads the world, so between two matches the worker can report
//! progress and hear a cancel — and ends the day with `finish_day`.
//!
//! What crosses to JavaScript is flat: buffers in the layouts of
//! `fm-persistence` and rows of plain numbers (the `*_ROW` constants). The
//! worker turns them into the requests of the database worker.

use fm_world::{
    ClubRow, Fixture, MatchResult, Played, World, WorldSave, CLUBS, CLUB_NAMES, MATCHES,
    SEASON_YEAR,
};
use wasm_bindgen::prelude::wasm_bindgen;

/// Bytes of a club row: strength, formation, mentality, pressing, width,
/// line height.
pub const CLUB_ROW: usize = 6;
/// Words of a player row: club (`u32::MAX`: none), position, birth year,
/// overall, potential, condition, morale, injury weeks.
pub const PLAYER_ROW: usize = 8;
/// Words of a fixture row: round, day, home, away, played (0/1), then the
/// result (home goals, away goals, home shots, away shots, home on target,
/// away on target; zeros while not played).
pub const FIXTURE_ROW: usize = 11;
/// Words of a result row: match id, then the six numbers of the result.
pub const RESULT_ROW: usize = 7;
/// Words of a standing row: club, played, won, drawn, lost, goals for,
/// goals against, points.
pub const STANDING_ROW: usize = 8;

fn result_words(r: &MatchResult) -> [u32; 6] {
    [
        u32::from(r.home_goals),
        u32::from(r.away_goals),
        u32::from(r.home_shots),
        u32::from(r.away_shots),
        u32::from(r.home_on_target),
        u32::from(r.away_on_target),
    ]
}

fn narrow<T: TryFrom<u32>>(value: u32, what: &str) -> Result<T, String> {
    T::try_from(value).map_err(|_| format!("{what} out of range: {value}"))
}

#[wasm_bindgen]
pub struct WorldHost {
    world: World,
    /// Today's matches already played and not yet applied.
    pending: Vec<(usize, Played)>,
    /// The results `finish_day` applied last (`RESULT_ROW` words each).
    last_results: Vec<u32>,
}

#[wasm_bindgen]
impl WorldHost {
    /// The world of `seed`, on day 0, with `user_club` for the user.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(seed: u64, user_club: u8) -> WorldHost {
        Self {
            world: World::new(seed, user_club),
            pending: Vec::new(),
            last_results: Vec::new(),
        }
    }

    /// The world a save holds, from the pieces the database returns: the
    /// rows as this module lays them out and the blobs as `fm-persistence`
    /// does.
    ///
    /// # Errors
    /// When the pieces are not a world of this league (see
    /// `World::from_save`), with the reason.
    ///
    /// # Panics
    /// Never: the chunks have the sizes checked just above.
    #[allow(clippy::too_many_arguments)] // the save, piece by piece, across the wasm boundary
    pub fn load(
        seed: u64,
        day: u16,
        user_club: u8,
        club_rows: &[u8],
        lineups: &[u8],
        sheets: &[u8],
        dynamics: &[u8],
        fixture_rows: &[u32],
        match_seeds: &[u8],
    ) -> Result<WorldHost, String> {
        if club_rows.len() != CLUBS * CLUB_ROW || lineups.len() != CLUBS * 44 {
            return Err("the league has 20 clubs".into());
        }
        if fixture_rows.len() != MATCHES * FIXTURE_ROW || match_seeds.len() != MATCHES * 8 {
            return Err("the season has 380 matches".into());
        }
        let clubs = club_rows
            .chunks_exact(CLUB_ROW)
            .zip(lineups.chunks_exact(44))
            .zip(CLUB_NAMES)
            .map(|((row, lineup), (name, short_name))| ClubRow {
                name: name.to_owned(),
                short_name: short_name.to_owned(),
                strength: row[0],
                formation: row[1],
                mentality: row[2],
                pressing: row[3],
                width: row[4],
                line_height: row[5],
                lineup: lineup.try_into().expect("chunks of 44 bytes"),
            })
            .collect();
        let mut fixtures = Vec::with_capacity(MATCHES);
        for (row, seed_bytes) in fixture_rows
            .chunks_exact(FIXTURE_ROW)
            .zip(match_seeds.chunks_exact(8))
        {
            let result = if row[4] == 0 {
                None
            } else {
                Some(MatchResult {
                    home_goals: narrow(row[5], "home goals")?,
                    away_goals: narrow(row[6], "away goals")?,
                    home_shots: narrow(row[7], "home shots")?,
                    away_shots: narrow(row[8], "away shots")?,
                    home_on_target: narrow(row[9], "home shots on target")?,
                    away_on_target: narrow(row[10], "away shots on target")?,
                })
            };
            fixtures.push(Fixture {
                round: narrow(row[0], "round")?,
                day: narrow(row[1], "day")?,
                home: narrow(row[2], "home club")?,
                away: narrow(row[3], "away club")?,
                seed: u64::from_le_bytes(seed_bytes.try_into().expect("chunks of 8 bytes")),
                result,
            });
        }
        let save = WorldSave {
            seed,
            day,
            season_year: SEASON_YEAR,
            user_club,
            clubs,
            // The display columns are derived from the blobs: not read back.
            players: Vec::new(),
            sheets: sheets.to_vec(),
            dynamics: dynamics.to_vec(),
            fixtures,
        };
        let world = World::from_save(&save).map_err(|e| e.to_string())?;
        Ok(Self {
            world,
            pending: Vec::new(),
            last_results: Vec::new(),
        })
    }

    /// The world seed in decimal (a `u64` does not fit a JavaScript number).
    #[must_use]
    pub fn seed(&self) -> String {
        self.world.seed.to_string()
    }

    /// Days of the season already lived: today is this day.
    #[must_use]
    pub fn day(&self) -> u16 {
        self.world.day
    }

    #[must_use]
    pub fn season_year(&self) -> u16 {
        SEASON_YEAR
    }

    #[must_use]
    pub fn user_club(&self) -> u8 {
        self.world.user_club
    }

    /// The season is over: no day left to live.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.world.finished()
    }

    /// Round (0-based) of the next match still to be played; 38 once the
    /// season is over.
    #[must_use]
    pub fn next_round(&self) -> u8 {
        self.world
            .fixtures
            .iter()
            .find(|f| f.result.is_none())
            .map_or(38, |f| f.round)
    }

    /// Ids of the matches of today, in id order (empty on a day off).
    #[must_use]
    pub fn matches_today(&self) -> Vec<u32> {
        self.world
            .matches_today()
            .into_iter()
            .filter_map(|id| u32::try_from(id).ok())
            .collect()
    }

    /// Plays match `id` of today in LOD `Abstract` and keeps its result
    /// until `finish_day`. The world is only read: nothing changes yet.
    /// Returns the result row (`RESULT_ROW` words).
    ///
    /// # Errors
    /// When `id` is not a match of today, or was already played today.
    pub fn play(&mut self, id: u32) -> Result<Vec<u32>, String> {
        let at = id as usize;
        if !self.world.matches_today().contains(&at) {
            return Err(format!(
                "match {id} is not played on day {}",
                self.world.day
            ));
        }
        if self.pending.iter().any(|&(played, _)| played == at) {
            return Err(format!("match {id} was already played today"));
        }
        let played = self.world.play(at);
        self.pending.push((at, played));
        let mut row = vec![id];
        row.extend(result_words(&played.result));
        Ok(row)
    }

    /// Ends the day with the matches played: results and minutes applied,
    /// the weekly update when a week closes, the day moved on. The results
    /// kept by `play` are consumed (see `day_results`).
    ///
    /// # Errors
    /// When not every match of today was played. Nothing changes.
    pub fn finish_day(&mut self) -> Result<(), String> {
        let today = self.world.matches_today();
        if self.pending.len() != today.len() {
            return Err(format!(
                "day {} has {} matches; {} were played",
                self.world.day,
                today.len(),
                self.pending.len()
            ));
        }
        let mut played = core::mem::take(&mut self.pending);
        played.sort_unstable_by_key(|&(id, _)| id);
        self.world.finish_day(&played);
        self.last_results = played
            .iter()
            .flat_map(|(id, p)| {
                let mut row = vec![u32::try_from(*id).unwrap_or(u32::MAX)];
                row.extend(result_words(&p.result));
                row
            })
            .collect();
        Ok(())
    }

    /// Forgets the matches played today and not applied: the day starts
    /// over (played again they give the same results).
    pub fn abandon_day(&mut self) {
        self.pending.clear();
    }

    /// The results the last `finish_day` applied, `RESULT_ROW` words each,
    /// in id order (empty after a day off).
    #[must_use]
    pub fn day_results(&self) -> Vec<u32> {
        self.last_results.clone()
    }

    /// `CLUB_ROW` bytes a club, in club order.
    #[must_use]
    pub fn club_rows(&self) -> Vec<u8> {
        self.world
            .to_save()
            .clubs
            .iter()
            .flat_map(|c| {
                [
                    c.strength,
                    c.formation,
                    c.mentality,
                    c.pressing,
                    c.width,
                    c.line_height,
                ]
            })
            .collect()
    }

    /// Club names, one a line, in club order.
    #[must_use]
    pub fn club_names(&self) -> String {
        CLUB_NAMES.map(|(name, _)| name).join("\n")
    }

    /// Club short names, one a line, in club order.
    #[must_use]
    pub fn club_short_names(&self) -> String {
        CLUB_NAMES.map(|(_, short)| short).join("\n")
    }

    /// The starting elevens (44 bytes a club), in club order.
    #[must_use]
    pub fn lineups(&self) -> Vec<u8> {
        self.world
            .to_save()
            .clubs
            .iter()
            .flat_map(|c| c.lineup)
            .collect()
    }

    /// `PLAYER_ROW` words a player, in player order.
    #[must_use]
    pub fn player_rows(&self) -> Vec<u32> {
        self.world
            .to_save()
            .players
            .iter()
            .flat_map(|p| {
                [
                    p.club.map_or(u32::MAX, u32::from),
                    u32::from(p.position),
                    u32::from(p.birth_year),
                    u32::from(p.overall),
                    u32::from(p.potential),
                    u32::from(p.condition),
                    u32::from(p.morale),
                    u32::from(p.injury_weeks),
                ]
            })
            .collect()
    }

    /// Player names, one a line, in player order.
    #[must_use]
    pub fn player_names(&self) -> String {
        self.world
            .to_save()
            .players
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every player's static sheet (60 bytes each), in player order.
    #[must_use]
    pub fn sheets(&self) -> Vec<u8> {
        fm_persistence::encode_sheets(&self.world.players)
    }

    /// Every player's dynamic state (14 bytes each), in player order.
    #[must_use]
    pub fn dynamics(&self) -> Vec<u8> {
        fm_persistence::encode_dynamics(&self.world.players)
    }

    /// `FIXTURE_ROW` words a match, in match order.
    #[must_use]
    pub fn fixture_rows(&self) -> Vec<u32> {
        self.world
            .fixtures
            .iter()
            .flat_map(|f| {
                let mut row = vec![
                    u32::from(f.round),
                    u32::from(f.day),
                    u32::from(f.home),
                    u32::from(f.away),
                    u32::from(f.result.is_some()),
                ];
                row.extend(f.result.as_ref().map_or([0; 6], result_words));
                row
            })
            .collect()
    }

    /// The match seeds (8 bytes each, little-endian), in match order.
    #[must_use]
    pub fn match_seeds(&self) -> Vec<u8> {
        self.world
            .fixtures
            .iter()
            .flat_map(|f| f.seed.to_le_bytes())
            .collect()
    }

    /// The league table as the world works it out, `STANDING_ROW` words a
    /// club, best first.
    #[must_use]
    pub fn standings(&self) -> Vec<u32> {
        self.world
            .standings()
            .iter()
            .flat_map(|s| {
                [
                    u32::from(s.club),
                    u32::from(s.played),
                    u32::from(s.won),
                    u32::from(s.drawn),
                    u32::from(s.lost),
                    u32::from(s.goals_for),
                    u32::from(s.goals_against),
                    u32::from(s.points),
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host rebuilt from the pieces of another, as the database would
    /// hand them back.
    fn reloaded(host: &WorldHost) -> Result<WorldHost, String> {
        WorldHost::load(
            host.seed().parse().expect("a u64"),
            host.day(),
            host.user_club(),
            &host.club_rows(),
            &host.lineups(),
            &host.sheets(),
            &host.dynamics(),
            &host.fixture_rows(),
            &host.match_seeds(),
        )
    }

    fn live(host: &mut WorldHost, days: u16) {
        for _ in 0..days {
            for id in host.matches_today() {
                host.play(id).expect("a match of today");
            }
            host.finish_day().expect("every match played");
        }
    }

    /// The host is the world: driven match by match it ends where
    /// `World::advance_day` ends, and its pieces rebuild it exactly.
    #[test]
    fn the_host_drives_the_world_match_by_match() {
        let mut host = WorldHost::new(7, 4);
        assert_eq!((host.day(), host.user_club(), host.next_round()), (0, 4, 0));
        assert_eq!(host.seed(), "7");
        assert!(host.matches_today().is_empty() && !host.finished());
        live(&mut host, 6);
        assert!(host.day_results().is_empty(), "six days off");
        assert_eq!(host.matches_today(), (0..10).collect::<Vec<u32>>());

        // A round: each match reports its result; the day ends with all ten.
        let first = host.play(3).expect("match 3 is today");
        assert_eq!((first.len(), first[0]), (RESULT_ROW, 3));
        assert!(host.play(3).is_err(), "not twice");
        assert!(host.play(10).is_err(), "next week's match");
        assert!(host.finish_day().is_err(), "nine matches missing");
        assert_eq!(host.day(), 6, "nothing changed");
        for id in [0, 1, 2, 4, 5, 6, 7, 8, 9] {
            host.play(id).expect("a match of today");
        }
        host.finish_day().expect("all ten played");
        assert_eq!((host.day(), host.next_round()), (7, 1));
        let results = host.day_results();
        assert_eq!(results.len(), 10 * RESULT_ROW);
        assert_eq!(
            results[3 * RESULT_ROW..4 * RESULT_ROW],
            first[..],
            "in id order"
        );

        // The same as the world living its days by itself.
        let mut world = World::new(7, 4);
        for _ in 0..7 {
            world.advance_day(|_, _| {});
        }
        assert_eq!(host.world, world);

        // Its pieces are the save; from them the same host comes back, and
        // goes on to the same place.
        let mut copy = reloaded(&host).expect("a world");
        assert_eq!(copy.world, host.world);
        live(&mut host, 7);
        live(&mut copy, 7);
        assert_eq!(copy.world, host.world);
        assert_eq!(copy.dynamics(), host.dynamics());
        assert_eq!(host.day(), 14);
    }

    /// Abandoning a day forgets what was played; played again, the matches
    /// give the same results.
    #[test]
    fn an_abandoned_day_starts_over() {
        let mut host = WorldHost::new(11, 0);
        live(&mut host, 6);
        let before = host.fixture_rows();
        let first: Vec<Vec<u32>> = [0, 1, 2].map(|id| host.play(id).expect("today")).to_vec();
        host.abandon_day();
        assert_eq!(host.fixture_rows(), before, "nothing was applied");
        assert_eq!(host.day(), 6);
        let again: Vec<Vec<u32>> = [0, 1, 2].map(|id| host.play(id).expect("today")).to_vec();
        assert_eq!(again, first);
    }

    /// The rows have the shape the worker counts on.
    #[test]
    fn rows_have_their_shape() {
        let mut host = WorldHost::new(7, 0);
        live(&mut host, 7);
        assert_eq!(host.club_rows().len(), 20 * CLUB_ROW);
        assert_eq!(host.lineups().len(), 20 * 44);
        assert_eq!(host.player_rows().len(), 500 * PLAYER_ROW);
        assert_eq!(host.sheets().len(), 500 * 60);
        assert_eq!(host.dynamics().len(), 500 * 14);
        assert_eq!(host.fixture_rows().len(), 380 * FIXTURE_ROW);
        assert_eq!(host.match_seeds().len(), 380 * 8);
        assert_eq!(host.club_names().lines().count(), 20);
        assert_eq!(host.club_short_names().lines().count(), 20);
        assert_eq!(host.player_names().lines().count(), 500);
        assert_eq!(host.season_year(), 2026);
        // Ten matches played, the rest to come.
        let played: u32 = host.fixture_rows().chunks(FIXTURE_ROW).map(|r| r[4]).sum();
        assert_eq!(played, 10);
        // The table: 20 clubs, one match each, best first.
        let table = host.standings();
        assert_eq!(table.len(), 20 * STANDING_ROW);
        assert!(table.chunks(STANDING_ROW).all(|r| r[1] == 1));
        let points: Vec<u32> = table.chunks(STANDING_ROW).map(|r| r[7]).collect();
        assert!(points.windows(2).all(|w| w[0] >= w[1]));
    }

    /// Pieces that are not a world are refused with the reason.
    #[test]
    fn pieces_that_are_not_a_world_are_refused() {
        let host = WorldHost::new(7, 0);
        let load = |seed: u64, sheets: &[u8], fixtures: &[u32], clubs: &[u8]| {
            WorldHost::load(
                seed,
                0,
                0,
                clubs,
                &host.lineups(),
                sheets,
                &host.dynamics(),
                fixtures,
                &host.match_seeds(),
            )
            .err()
        };
        let (sheets, fixtures, clubs) = (host.sheets(), host.fixture_rows(), host.club_rows());
        assert_eq!(load(7, &sheets, &fixtures, &clubs), None);
        assert_eq!(
            load(8, &sheets, &fixtures, &clubs).as_deref(),
            Some("the calendar is not the calendar of the world's seed")
        );
        assert_eq!(
            load(7, &sheets[60..], &fixtures, &clubs).as_deref(),
            Some("blob with 7000 bytes, expected 6986")
        );
        assert_eq!(
            load(7, &sheets, &fixtures[11..], &clubs).as_deref(),
            Some("the season has 380 matches")
        );
        assert_eq!(
            load(7, &sheets, &fixtures, &clubs[6..]).as_deref(),
            Some("the league has 20 clubs")
        );
        let mut bad = fixtures.clone();
        bad[5] = 300; // home goals that do not fit
        bad[4] = 1;
        assert_eq!(
            load(7, &sheets, &bad, &clubs).as_deref(),
            Some("home goals out of range: 300")
        );
    }
}
