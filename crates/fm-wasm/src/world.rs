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

/// The clubs of a save from their rows (`CLUB_ROW` bytes each) and their
/// line-ups (44 bytes each).
fn club_rows_of(club_rows: &[u8], lineups: &[u8]) -> Result<Vec<ClubRow>, String> {
    if club_rows.len() != CLUBS * CLUB_ROW || lineups.len() != CLUBS * 44 {
        return Err("the league has 20 clubs".into());
    }
    Ok(club_rows
        .chunks_exact(CLUB_ROW)
        .zip(lineups.chunks_exact(44))
        .zip(CLUB_NAMES)
        .map(|((row, lineup), (name, short_name))| {
            let mut eleven = [0_u8; 44];
            eleven.copy_from_slice(lineup);
            ClubRow {
                name: name.to_owned(),
                short_name: short_name.to_owned(),
                strength: row[0],
                formation: row[1],
                mentality: row[2],
                pressing: row[3],
                width: row[4],
                line_height: row[5],
                lineup: eleven,
            }
        })
        .collect())
}

/// The world, as one worker holds it. The world worker's host owns the
/// world; the hosts of the match workers (the pool) are copies kept in
/// step with `sync_day`, which only ever `play`.
#[wasm_bindgen]
pub struct WorldHost {
    world: World,
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
        let clubs = club_rows_of(club_rows, lineups)?;
        if fixture_rows.len() != MATCHES * FIXTURE_ROW || match_seeds.len() != MATCHES * 8 {
            return Err("the season has 380 matches".into());
        }
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

    /// Plays match `id` of today in LOD `Abstract` and returns its result
    /// row (`RESULT_ROW` words). The world is only read: nothing is kept
    /// and nothing changes, so any host holding the same day gives the same
    /// row — which is what lets the matches of a day be played elsewhere.
    ///
    /// # Errors
    /// When `id` is not a match of today.
    pub fn play(&self, id: u32) -> Result<Vec<u32>, String> {
        let at = id as usize;
        if !self.world.matches_today().contains(&at) {
            return Err(format!(
                "match {id} is not played on day {}",
                self.world.day
            ));
        }
        let mut row = vec![id];
        row.extend(result_words(&self.world.play(at).result));
        Ok(row)
    }

    /// Ends the day with `results` (`RESULT_ROW` words a match, in any
    /// order, wherever they were played): results and minutes applied in
    /// id order, the weekly update when a week closes, the day moved on.
    ///
    /// # Errors
    /// When `results` is not exactly the matches of today, each once, or a
    /// number does not fit. Nothing changes.
    pub fn finish_day(&mut self, results: &[u32]) -> Result<(), String> {
        let today = self.world.matches_today();
        if results.len() != today.len() * RESULT_ROW {
            return Err(format!(
                "day {} has {} matches; {} results came",
                self.world.day,
                today.len(),
                results.len() / RESULT_ROW
            ));
        }
        let mut played = Vec::with_capacity(today.len());
        for row in results.chunks_exact(RESULT_ROW) {
            let id = row[0] as usize;
            if !today.contains(&id) || played.iter().any(|&(seen, _)| seen == id) {
                return Err(format!(
                    "match {} is not a match of day {} (or came twice)",
                    row[0], self.world.day
                ));
            }
            let fixture = &self.world.fixtures[id];
            played.push((
                id,
                Played {
                    result: MatchResult {
                        home_goals: narrow(row[1], "home goals")?,
                        away_goals: narrow(row[2], "away goals")?,
                        home_shots: narrow(row[3], "home shots")?,
                        away_shots: narrow(row[4], "away shots")?,
                        home_on_target: narrow(row[5], "home shots on target")?,
                        away_on_target: narrow(row[6], "away shots on target")?,
                    },
                    // Who started: the same line-ups `play` used, since the
                    // world has not changed since the day began.
                    home_lineup: self.world.lineup_today(fixture.home),
                    away_lineup: self.world.lineup_today(fixture.away),
                },
            ));
        }
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

    /// Overwrites what changes from one day to the next — the day, every
    /// player's dynamic state, the clubs' tactics and line-ups — with
    /// another host's (`day`, `dynamics`, `club_rows`, `lineups`). A host
    /// kept in step like this plays any match of the day exactly as the
    /// host it follows would.
    ///
    /// # Errors
    /// When the pieces are not this world's (sizes, codes, a line-up with
    /// somebody from another club). The host may be left half updated: it
    /// must be loaded again.
    pub fn sync_day(
        &mut self,
        day: u16,
        dynamics: &[u8],
        club_rows: &[u8],
        lineups: &[u8],
    ) -> Result<(), String> {
        let clubs = club_rows_of(club_rows, lineups)?;
        self.world
            .sync_day(day, dynamics, &clubs)
            .map_err(|e| e.to_string())
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

    /// The rows of today's matches, played by `host` itself.
    fn play_day(host: &WorldHost) -> Vec<u32> {
        host.matches_today()
            .into_iter()
            .flat_map(|id| host.play(id).expect("a match of today"))
            .collect()
    }

    fn live(host: &mut WorldHost, days: u16) {
        for _ in 0..days {
            let results = play_day(host);
            host.finish_day(&results).expect("today's matches");
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

        // `play` only reads: twice, the same row; a match of another day, no.
        let first = host.play(3).expect("match 3 is today");
        assert_eq!((first.len(), first[0]), (RESULT_ROW, 3));
        assert_eq!(host.play(3).expect("again"), first);
        assert!(host.play(10).is_err(), "next week's match");
        assert_eq!(host.day(), 6);

        // The day ends with exactly today's results, whatever their order.
        let mut results = play_day(&host);
        assert!(
            host.finish_day(&results[RESULT_ROW..]).is_err(),
            "one missing"
        );
        let mut twice = results.clone();
        twice[RESULT_ROW] = 0; // match 0 where match 1 was
        assert!(host.finish_day(&twice).is_err(), "one twice");
        let mut alien = results.clone();
        alien[0] = 10;
        assert!(host.finish_day(&alien).is_err(), "next week's match");
        assert_eq!(host.day(), 6, "nothing changed");
        // Reversed: the order results arrive in does not matter.
        let reversed: Vec<u32> = results
            .chunks(RESULT_ROW)
            .rev()
            .flatten()
            .copied()
            .collect();
        host.finish_day(&reversed).expect("all ten");
        assert_eq!((host.day(), host.next_round()), (7, 1));
        results.truncate(10 * RESULT_ROW);
        assert_eq!(host.day_results(), results, "kept in id order");

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

    /// The pool: hosts loaded once and kept in step with `sync_day` play
    /// every match of every day exactly as the host that owns the world —
    /// so a season lived with its matches played elsewhere, in any split
    /// and any order, is the season lived alone.
    #[test]
    fn hosts_kept_in_step_play_the_same_matches() {
        let mut owner = WorldHost::new(11, 0);
        // Two players, loaded when the world was new.
        let mut players = [
            reloaded(&owner).expect("a world"),
            reloaded(&owner).expect("a world"),
        ];
        let mut alone = WorldHost::new(11, 0);
        let mut rounds = 0;
        for _ in 0..21 {
            let today = owner.matches_today();
            for player in &mut players {
                player
                    .sync_day(
                        owner.day(),
                        &owner.dynamics(),
                        &owner.club_rows(),
                        &owner.lineups(),
                    )
                    .expect("this world's day");
            }
            // Dealt out in turns, collected back to front.
            let mut results: Vec<Vec<u32>> = Vec::new();
            for (turn, &id) in today.iter().enumerate() {
                let row = players[turn % 2].play(id).expect("a match of today");
                assert_eq!(row, owner.play(id).expect("a match of today"), "match {id}");
                results.push(row);
            }
            results.reverse();
            owner
                .finish_day(&results.concat())
                .expect("today's matches");
            live(&mut alone, 1);
            rounds += usize::from(!today.is_empty());
        }
        assert_eq!(rounds, 3);
        assert_eq!(owner.world, alone.world);
        assert_eq!(owner.day(), 21);
        // The players never ended a day themselves: only what changes was
        // overwritten, and their calendar still has no result.
        assert!(players[0]
            .fixture_rows()
            .chunks(FIXTURE_ROW)
            .all(|r| r[4] == 0));

        // A day that is not this world's is refused.
        let (dynamics, clubs, lineups) = (owner.dynamics(), owner.club_rows(), owner.lineups());
        let player = &mut players[0];
        assert!(player
            .sync_day(21, &dynamics[14..], &clubs, &lineups)
            .is_err());
        assert!(player
            .sync_day(21, &dynamics, &clubs[6..], &lineups)
            .is_err());
        let mut wrong = clubs.clone();
        wrong[1] = 9; // a formation that does not exist
        assert!(player.sync_day(21, &dynamics, &wrong, &lineups).is_err());
        let mut stolen = lineups.clone();
        stolen[..4].copy_from_slice(&30_u32.to_le_bytes()); // club 1's player in club 0
        assert!(player.sync_day(21, &dynamics, &clubs, &stolen).is_err());
        assert!(player.sync_day(21, &dynamics, &clubs, &lineups).is_ok());
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
