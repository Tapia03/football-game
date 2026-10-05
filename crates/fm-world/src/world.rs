//! The world (spec Fase 7B): one league, 20 clubs, 500 synthetic players,
//! a season of 380 matches. It is built from a seed and advanced one day at
//! a time; every match is played by the existing `MatchEngine` in LOD
//! `Abstract`. The same seed gives the same world, bit for bit, however the
//! matches of a day are scheduled.

use fm_core::rng::{splitmix64, Rng};
use fm_entities::{generate_database, ClubId, PlayerDatabase, PlayerId, PlayerStatic, Position};
use fm_match::{
    Formation, LodLevel, MatchEngine, MatchSetup, Side, Tactics, TeamSheet, TuningParams,
};

use crate::fixtures::{season_fixtures, Fixture, MatchResult, CLUBS, SEASON_DAYS};
use crate::squad::{overall, pick_lineup};

/// Players of a squad, and how many of each position.
pub const SQUAD_SIZE: usize = 25;
const QUOTA: [(Position, usize); 12] = [
    (Position::Goalkeeper, 3),
    (Position::CentreBack, 4),
    (Position::LeftBack, 2),
    (Position::RightBack, 2),
    (Position::DefensiveMidfielder, 2),
    (Position::CentralMidfielder, 3),
    (Position::LeftMidfielder, 1),
    (Position::RightMidfielder, 1),
    (Position::AttackingMidfielder, 1),
    (Position::LeftWinger, 2),
    (Position::RightWinger, 2),
    (Position::Striker, 2),
];
/// Players of the world.
pub const PLAYERS: usize = CLUBS * SQUAD_SIZE;

const _: () = assert!(PLAYERS == 500);

// The same sizes in the integer types the ids and the RNG take.
const CLUBS_U8: u8 = 20;
const SQUAD_SIZE_U32: u32 = 25;
const _: () = assert!(CLUBS_U8 as usize == CLUBS && SQUAD_SIZE_U32 as usize == SQUAD_SIZE);

/// The formation every club of the league starts with (spec Fase 7B,
/// decided 2026-10-05). Not drawn among the engine's five on purpose: with
/// the engine as it is the formation decides the table, not the squad —
/// drawn formations gave 4.6–5.3 goals a match and a 0.19–0.65 correlation
/// between squad strength and final position, 4-3-3 against 4-3-3 gave
/// 12.5 goals a match — while everybody on 4-4-2 gives ~3.0 goals and a
/// 0.71–0.94 correlation. The imbalance is the engine's, for Fase 8.
pub const LEAGUE_FORMATION: Formation = Formation::F442;

/// Year the first season is played in (ages are counted from it).
pub const SEASON_YEAR: u16 = 2026;
/// Minutes a starter is credited with (the engine has no substitutions).
const MATCH_MINUTES: u16 = 90;
/// How far the order of a draft round strays from the order of strength:
/// the weaker club picks before the stronger one often enough for the
/// league to have upsets, rarely enough for it to have favourites.
const DRAFT_NOISE: u32 = 7;

const BOOTSTRAP_DOMAIN: u64 = 0x776F_726C_645F_6273; // "world_bs"

#[derive(Debug, Clone, PartialEq)]
pub struct Club {
    pub id: ClubId,
    /// Mean `overall` of the squad when the world was made.
    pub strength: u8,
    pub formation: Formation,
    pub tactics: Tactics,
    /// The eleven that starts, slot by slot of `formation`.
    pub lineup: [PlayerId; 11],
    /// Everybody on the books.
    pub squad: Vec<PlayerId>,
}

/// A match as it was played: the result and who started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub result: MatchResult,
    pub home_lineup: [PlayerId; 11],
    pub away_lineup: [PlayerId; 11],
}

/// One line of the league table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub club: u8,
    pub played: u16,
    pub won: u16,
    pub drawn: u16,
    pub lost: u16,
    pub goals_for: u16,
    pub goals_against: u16,
    pub points: u16,
}

impl Standing {
    #[must_use]
    pub fn goal_difference(&self) -> i32 {
        i32::from(self.goals_for) - i32::from(self.goals_against)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub seed: u64,
    /// Days of the season already lived: today is day `day`.
    pub day: u16,
    /// Club the user manages.
    pub user_club: u8,
    pub players: PlayerDatabase,
    pub clubs: Vec<Club>,
    /// The season's matches; the index is the match id.
    pub fixtures: Vec<Fixture>,
}

/// Candidates of each position, in the order the generator made them.
fn candidates(seed: u64) -> Vec<Vec<PlayerStatic>> {
    // The generator is prefix-stable, so asking for more never changes the
    // players already seen: grow until every position has enough.
    let mut count = 4_000_u32;
    loop {
        let pool = generate_database(count, seed, SEASON_YEAR);
        let by_position: Vec<Vec<PlayerStatic>> = QUOTA
            .iter()
            .map(|&(position, quota)| {
                pool.statics()
                    .iter()
                    .filter(|s| s.position == position)
                    .take(CLUBS * quota)
                    .copied()
                    .collect()
            })
            .collect();
        let enough = by_position
            .iter()
            .zip(QUOTA)
            .all(|(found, (_, quota))| found.len() == CLUBS * quota);
        if enough {
            return by_position;
        }
        count *= 2;
    }
}

impl World {
    /// The world of `seed`, on day 0, with `user_club` (taken modulo the
    /// number of clubs) for the user.
    ///
    /// # Panics
    /// Never: 20 clubs and 500 players fit their ids.
    #[must_use]
    pub fn new(seed: u64, user_club: u8) -> Self {
        let mut rng = Rng::seed_from_u64(splitmix64(seed ^ BOOTSTRAP_DOMAIN));
        // Order of strength: `ranked[0]` is the club that tends to pick first.
        let mut ranked: Vec<usize> = (0..CLUBS).collect();
        for i in (1..CLUBS).rev() {
            let j = rng.below(u32::try_from(i + 1).expect("20 clubs")) as usize;
            ranked.swap(i, j);
        }
        // A draft per position: every round the clubs pick the best player
        // left, in an order that is the order of strength plus noise.
        let mut squads: Vec<Vec<PlayerStatic>> =
            (0..CLUBS).map(|_| Vec::with_capacity(SQUAD_SIZE)).collect();
        for (mut pool, (_, quota)) in candidates(seed).into_iter().zip(QUOTA) {
            // Best first; the generator's order breaks ties.
            pool.sort_by_key(|s| u8::MAX - overall(s));
            let mut pool = pool.into_iter();
            for _ in 0..quota {
                let mut order: Vec<(u32, usize)> = ranked
                    .iter()
                    .enumerate()
                    .map(|(rank, &club)| {
                        (
                            u32::try_from(rank).expect("20 clubs") + rng.below(DRAFT_NOISE),
                            club,
                        )
                    })
                    .collect();
                order.sort_unstable();
                for (_, club) in order {
                    squads[club].push(pool.next().expect("a pool has clubs × quota players"));
                }
            }
        }
        // Club by club, so that a squad is a run of ids.
        let mut players = PlayerDatabase::with_capacity(PLAYERS);
        let mut clubs = Vec::with_capacity(CLUBS);
        for (c, squad) in squads.into_iter().enumerate() {
            let id = ClubId(u32::try_from(c).expect("20 clubs"));
            let total: u32 = squad.iter().map(|s| u32::from(overall(s))).sum();
            let ids: Vec<PlayerId> = squad
                .into_iter()
                .map(|mut s| {
                    s.club = Some(id);
                    players.create(s)
                })
                .collect();
            let formation = LEAGUE_FORMATION;
            clubs.push(Club {
                id,
                strength: u8::try_from(total / SQUAD_SIZE_U32).expect("a mean of ratings"),
                formation,
                tactics: Tactics::default(),
                lineup: pick_lineup(&players, &ids, formation, |_| true),
                squad: ids,
            });
        }
        Self {
            seed,
            day: 0,
            user_club: user_club % CLUBS_U8,
            players,
            clubs,
            fixtures: season_fixtures(seed),
        }
    }

    /// The season is over: no day left to live.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.day >= SEASON_DAYS
    }

    /// Ids of the matches played today, in id order.
    #[must_use]
    pub fn matches_today(&self) -> Vec<usize> {
        if self.finished() {
            return Vec::new();
        }
        self.fixtures
            .iter()
            .enumerate()
            .filter(|(_, f)| f.day == self.day && f.result.is_none())
            .map(|(id, _)| id)
            .collect()
    }

    /// Whether the player can be picked (not injured).
    #[must_use]
    pub fn available(&self, id: PlayerId) -> bool {
        self.players.dynamic_of(id).injury_weeks == 0
    }

    /// The eleven `club` fields today: its line-up, with whoever is injured
    /// replaced by the best available player for the slot.
    #[must_use]
    pub fn lineup_today(&self, club: u8) -> [PlayerId; 11] {
        let club = &self.clubs[usize::from(club)];
        if club.lineup.iter().all(|&id| self.available(id)) {
            return club.lineup;
        }
        // The fit starters keep their slots; the others are picked anew.
        let mut out = club.lineup;
        let fresh = pick_lineup(&self.players, &club.squad, club.formation, |id| {
            self.available(id) && !club.lineup.iter().any(|&l| l == id && self.available(l))
        });
        for (slot, id) in out.iter_mut().enumerate() {
            if !self.available(*id) {
                *id = fresh[slot];
            }
        }
        out
    }

    /// Plays match `id` in LOD `Abstract`. Pure: it reads the world as it
    /// is at the start of the day and changes nothing, so the matches of a
    /// day can be played in any order (or elsewhere) with the same result.
    ///
    /// # Panics
    /// When `id` is not a match of the season.
    #[must_use]
    pub fn play(&self, id: usize) -> Played {
        let fixture = &self.fixtures[id];
        let sheet = |club: u8| {
            let c = &self.clubs[usize::from(club)];
            TeamSheet {
                formation: c.formation,
                tactics: c.tactics,
                players: self.lineup_today(club),
            }
        };
        let setup = MatchSetup {
            match_seed: fixture.seed,
            home: sheet(fixture.home),
            away: sheet(fixture.away),
            tuning: TuningParams::default(),
        };
        let mut engine = MatchEngine::new(&setup, &self.players);
        engine.run(LodLevel::Abstract, |_| {});
        let state = engine.state();
        let (home, away) = (state.team(Side::Home), state.team(Side::Away));
        Played {
            result: MatchResult {
                home_goals: home.score,
                away_goals: away.score,
                home_shots: home.shots,
                away_shots: away.shots,
                home_on_target: home.shots_on_target,
                away_on_target: away.shots_on_target,
            },
            home_lineup: setup.home.players,
            away_lineup: setup.away.players,
        }
    }

    /// Ends the day: records the matches played (in id order, whatever the
    /// order of `played`), credits the minutes, and moves to the next day —
    /// running the weekly update of every player when a week closes.
    ///
    /// # Panics
    /// When `played` is not exactly today's matches.
    pub fn finish_day(&mut self, played: &[(usize, Played)]) {
        let mut played: Vec<(usize, Played)> = played.to_vec();
        played.sort_unstable_by_key(|&(id, _)| id);
        let ids: Vec<usize> = played.iter().map(|&(id, _)| id).collect();
        assert_eq!(ids, self.matches_today(), "exactly today's matches");
        for (id, p) in played {
            self.fixtures[id].result = Some(p.result);
            for player in p.home_lineup.into_iter().chain(p.away_lineup) {
                self.players.record_minutes(player, MATCH_MINUTES);
            }
        }
        self.day += 1;
        if self.day % 7 == 0 {
            self.players
                .weekly_update(self.seed, u32::from(self.day / 7));
        }
    }

    /// Lives one day: plays today's matches one after the other, telling
    /// `on_match(done, total)` after each, and ends the day. Returns the
    /// ids of the matches played. Does nothing once the season is over.
    pub fn advance_day(&mut self, mut on_match: impl FnMut(usize, usize)) -> Vec<usize> {
        if self.finished() {
            return Vec::new();
        }
        let today = self.matches_today();
        let mut played = Vec::with_capacity(today.len());
        for (done, &id) in today.iter().enumerate() {
            played.push((id, self.play(id)));
            on_match(done + 1, today.len());
        }
        self.finish_day(&played);
        today
    }

    /// The league table: points, then goal difference, then goals scored,
    /// then club id.
    #[must_use]
    pub fn standings(&self) -> Vec<Standing> {
        let mut table: Vec<Standing> = (0..CLUBS_U8)
            .map(|club| Standing {
                club,
                played: 0,
                won: 0,
                drawn: 0,
                lost: 0,
                goals_for: 0,
                goals_against: 0,
                points: 0,
            })
            .collect();
        for f in &self.fixtures {
            let Some(r) = f.result else { continue };
            let sides = [
                (f.home, r.home_goals, r.away_goals),
                (f.away, r.away_goals, r.home_goals),
            ];
            for (club, scored, conceded) in sides {
                let row = &mut table[usize::from(club)];
                row.played += 1;
                row.goals_for += u16::from(scored);
                row.goals_against += u16::from(conceded);
                match scored.cmp(&conceded) {
                    core::cmp::Ordering::Greater => {
                        row.won += 1;
                        row.points += 3;
                    }
                    core::cmp::Ordering::Equal => {
                        row.drawn += 1;
                        row.points += 1;
                    }
                    core::cmp::Ordering::Less => row.lost += 1,
                }
            }
        }
        table.sort_by_key(|s| {
            (
                core::cmp::Reverse(s.points),
                core::cmp::Reverse(s.goal_difference()),
                core::cmp::Reverse(s.goals_for),
                s.club,
            )
        });
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{MATCHES, MATCHES_PER_ROUND};
    use std::collections::HashSet;

    /// Same seed, same world; another seed, another world.
    #[test]
    fn the_world_is_a_function_of_its_seed() {
        let a = World::new(7, 3);
        assert_eq!(a, World::new(7, 3));
        let b = World::new(8, 3);
        assert_ne!(a.players, b.players);
        assert_eq!(World::new(7, 23).user_club, 3, "the user's club wraps");
        assert_eq!((a.day, a.fixtures.len()), (0, MATCHES));
        assert!(!a.finished());
    }

    /// 500 players, 25 a club by the quota of positions; every player in
    /// one club; line-ups made of the club's own players; names for all.
    #[test]
    fn squads_follow_the_quotas() {
        let w = World::new(7, 0);
        assert_eq!(w.players.len(), PLAYERS);
        assert_eq!(w.clubs.len(), CLUBS);
        let mut seen = HashSet::new();
        for (c, club) in w.clubs.iter().enumerate() {
            assert_eq!(club.id, ClubId(u32::try_from(c).unwrap()));
            assert_eq!(club.squad.len(), SQUAD_SIZE);
            for (position, quota) in QUOTA {
                let n = club
                    .squad
                    .iter()
                    .filter(|&&id| w.players.static_of(id).position == position)
                    .count();
                assert_eq!(n, quota, "club {c}, {position:?}");
            }
            for &id in &club.squad {
                assert_eq!(w.players.static_of(id).club, Some(club.id));
                assert!(seen.insert(id), "{id:?} in two clubs");
                assert!(w.players.static_of(id).attributes.is_valid());
            }
            assert_eq!(club.lineup.iter().collect::<HashSet<_>>().len(), 11);
            assert!(club.lineup.iter().all(|id| club.squad.contains(id)));
            assert_eq!(
                w.players.static_of(club.lineup[0]).position,
                Position::Goalkeeper,
                "slot 0 is the keeper"
            );
            assert_eq!(w.lineup_today(u8::try_from(c).unwrap()), club.lineup);
        }
        assert_eq!(seen.len(), PLAYERS);
    }

    /// The draft makes a league with favourites: the strongest squads are
    /// clearly better than the weakest. Every club plays the league's one
    /// formation, with default tactics.
    #[test]
    fn clubs_differ_in_strength_not_in_formation() {
        let w = World::new(7, 0);
        let mut strengths: Vec<u8> = w.clubs.iter().map(|c| c.strength).collect();
        strengths.sort_unstable();
        let (weakest, strongest) = (strengths[0], strengths[CLUBS - 1]);
        assert!(strongest >= weakest + 3, "{strengths:?}");
        assert_eq!(LEAGUE_FORMATION, Formation::F442);
        assert!(w
            .clubs
            .iter()
            .all(|c| c.formation == LEAGUE_FORMATION && c.tactics == Tactics::default()));
    }

    /// An injured starter is replaced by a fit player of the squad, and the
    /// fit starters keep their slots.
    #[test]
    fn injured_starters_are_replaced() {
        let mut w = World::new(7, 0);
        let club = w.clubs[4].clone();
        let (keeper, striker) = (club.lineup[0], club.lineup[10]);
        w.players.dynamic_mut(keeper).injury_weeks = 2;
        w.players.dynamic_mut(striker).injury_weeks = 1;
        let today = w.lineup_today(4);
        assert_ne!(today[0], keeper);
        assert_ne!(today[10], striker);
        assert_eq!(w.players.static_of(today[0]).position, Position::Goalkeeper);
        assert_eq!(today[1..10], club.lineup[1..10]);
        assert_eq!(today.iter().collect::<HashSet<_>>().len(), 11);
        assert!(today
            .iter()
            .all(|&id| w.available(id) && club.squad.contains(&id)));
    }

    /// Two weeks of the season, twice: the same world, bit for bit. Empty
    /// days play nothing; a round plays its ten matches; the table adds up.
    #[test]
    fn living_days_is_deterministic_and_the_table_adds_up() {
        let live = |seed: u64| {
            let mut w = World::new(seed, 0);
            let mut calls = Vec::new();
            let mut rounds = Vec::new();
            for _ in 0..14 {
                let day = w.day;
                let played = w.advance_day(|done, total| calls.push((day, done, total)));
                if !played.is_empty() {
                    rounds.push((day, played));
                }
            }
            (w, calls, rounds)
        };
        let (a, calls, rounds) = live(7);
        let (b, ..) = live(7);
        assert_eq!(a, b);
        assert_ne!(a.fixtures, live(8).0.fixtures);
        assert_eq!(a.day, 14);
        // Rounds on days 6 and 13, ten matches each, reported one by one.
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0], (6, (0..10).collect::<Vec<_>>()));
        assert_eq!(rounds[1], (13, (10..20).collect::<Vec<_>>()));
        let expected: Vec<(u16, usize, usize)> = [6, 13]
            .into_iter()
            .flat_map(|day| (1..=MATCHES_PER_ROUND).map(move |done| (day, done, 10)))
            .collect();
        assert_eq!(calls, expected);
        assert_eq!(a.fixtures.iter().filter(|f| f.result.is_some()).count(), 20);
        assert_eq!(a.matches_today(), Vec::<usize>::new());

        // The table is the results, added up.
        let table = a.standings();
        assert_eq!(table.len(), CLUBS);
        let (mut goals_for, mut goals_against, mut points) = (0, 0, 0);
        for row in &table {
            assert_eq!(row.played, 2);
            assert_eq!(row.won + row.drawn + row.lost, row.played);
            assert_eq!(row.points, 3 * row.won + row.drawn);
            goals_for += row.goals_for;
            goals_against += row.goals_against;
            points += row.points;
        }
        assert_eq!(goals_for, goals_against);
        let goals: u16 = a
            .fixtures
            .iter()
            .filter_map(|f| f.result)
            .map(|r| u16::from(r.home_goals) + u16::from(r.away_goals))
            .sum();
        assert_eq!(goals_for, goals);
        let draws = a
            .fixtures
            .iter()
            .filter_map(|f| f.result)
            .filter(|r| r.home_goals == r.away_goals)
            .count();
        assert_eq!(usize::from(points), 3 * 20 - draws);
        for pair in table.windows(2) {
            let key = |s: &Standing| (s.points, s.goal_difference(), s.goals_for);
            assert!(key(&pair[0]) >= key(&pair[1]));
        }
        // Minutes were credited and the weeks closed: somebody is tired.
        assert!(a.players.dynamics().iter().any(|d| d.fatigue > 0));
        assert_ne!(a.players.dynamics(), World::new(7, 0).players.dynamics());
    }

    /// The matches of a day do not depend on each other: played in any
    /// order they give the same results, and the day ends the same.
    #[test]
    fn the_order_of_a_days_matches_does_not_matter() {
        let mut in_order = World::new(11, 0);
        for _ in 0..6 {
            in_order.advance_day(|_, _| {});
        }
        let mut reversed = in_order.clone();
        let today = in_order.matches_today();
        assert_eq!(today.len(), 10);
        let mut played: Vec<(usize, Played)> = today
            .iter()
            .rev()
            .map(|&id| (id, reversed.play(id)))
            .collect();
        // Playing twice changes nothing: `play` only reads.
        assert_eq!(played[0].1, reversed.play(played[0].0));
        played.swap(2, 7);
        reversed.finish_day(&played);
        in_order.advance_day(|_, _| {});
        assert_eq!(in_order, reversed);
    }
}
