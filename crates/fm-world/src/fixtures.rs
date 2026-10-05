//! The calendar of the league (spec Fase 7B): a double round-robin by the
//! circle method — 19 rounds, then the same 19 with the venues swapped.

use fm_core::rng::splitmix64;

/// Clubs in the league.
pub const CLUBS: usize = 20;
/// Rounds of the season: every club meets every other twice.
pub const ROUNDS: usize = 2 * (CLUBS - 1);
/// Matches of one round.
pub const MATCHES_PER_ROUND: usize = CLUBS / 2;
/// Matches of the season.
pub const MATCHES: usize = ROUNDS * MATCHES_PER_ROUND;
/// One round a week, on this day of the week (0-based).
pub const MATCH_DAY_OF_WEEK: u16 = 6;
/// Days of the season: 38 weeks.
pub const SEASON_DAYS: u16 = 7 * 38;

const _: () = assert!(ROUNDS == 38 && MATCHES == 380);

/// Mixed into the world seed for match seeds (never the raw world seed).
const MATCH_SEED_DOMAIN: u64 = 0x6D61_7463_685F_7364; // "match_sd"

/// What a match left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchResult {
    pub home_goals: u8,
    pub away_goals: u8,
    pub home_shots: u16,
    pub away_shots: u16,
    pub home_on_target: u16,
    pub away_on_target: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    /// 0-based round.
    pub round: u8,
    /// Day of the season it is played on.
    pub day: u16,
    /// Club ids.
    pub home: u8,
    pub away: u8,
    /// Seed of the match: stored in the save, replays the match.
    pub seed: u64,
    pub result: Option<MatchResult>,
}

/// Day of the season round `round` (0-based) is played on.
#[must_use]
pub const fn day_of_round(round: u8) -> u16 {
    7 * round as u16 + MATCH_DAY_OF_WEEK
}

/// The 380 fixtures of the season, in round order; the index of a fixture
/// is its match id. A pure function of the world seed (which only decides
/// the match seeds: the pairings are the same for every world).
///
/// # Panics
/// Never: club ids and rounds fit a `u8`.
#[must_use]
pub fn season_fixtures(world_seed: u64) -> Vec<Fixture> {
    let mut out = Vec::with_capacity(MATCHES);
    // Circle method: club 0 stays, the others rotate one seat a round.
    let mut seats: Vec<u8> = (0..CLUBS)
        .map(|c| u8::try_from(c).expect("20 clubs"))
        .collect();
    let mut first_leg = Vec::with_capacity(MATCHES / 2);
    for round in 0..CLUBS - 1 {
        for i in 0..MATCHES_PER_ROUND {
            let (a, b) = (seats[i], seats[CLUBS - 1 - i]);
            // A club moves one seat a round, so alternating venues by seat
            // alternates them by round for everybody; the fixed club (seat
            // 0) alternates by round on its own.
            let swap = if i == 0 { round % 2 == 1 } else { i % 2 == 1 };
            first_leg.push(if swap { (round, b, a) } else { (round, a, b) });
        }
        seats[1..].rotate_right(1);
    }
    let second_leg = first_leg
        .iter()
        .map(|&(round, home, away)| (round + CLUBS - 1, away, home));
    for (id, (round, home, away)) in first_leg.iter().copied().chain(second_leg).enumerate() {
        let round = u8::try_from(round).expect("38 rounds");
        out.push(Fixture {
            round,
            day: day_of_round(round),
            home,
            away,
            seed: splitmix64(world_seed ^ MATCH_SEED_DOMAIN ^ splitmix64(id as u64)),
            result: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 380 matches; every club plays once a round; every ordered pair
    /// (home, away) exactly once — so each pair of clubs meets twice, once
    /// at each ground.
    #[test]
    fn double_round_robin_is_complete() {
        let fixtures = season_fixtures(7);
        assert_eq!(fixtures.len(), 380);
        let mut pairs = HashSet::new();
        for (round, matches) in fixtures.chunks(MATCHES_PER_ROUND).enumerate() {
            let mut playing = HashSet::new();
            for f in matches {
                assert_eq!(usize::from(f.round), round);
                assert_eq!(f.day, day_of_round(f.round));
                assert_ne!(f.home, f.away);
                assert!(
                    playing.insert(f.home) && playing.insert(f.away),
                    "round {round}"
                );
                assert!(
                    pairs.insert((f.home, f.away)),
                    "{} x {} twice",
                    f.home,
                    f.away
                );
                assert!(f.result.is_none());
            }
            assert_eq!(playing.len(), CLUBS);
        }
        assert_eq!(pairs.len(), CLUBS * (CLUBS - 1));
        // The return leg is the first leg with the venues swapped, 19 rounds on.
        for (first, second) in fixtures[..190].iter().zip(&fixtures[190..]) {
            assert_eq!((first.home, first.away), (second.away, second.home));
            assert_eq!(first.round + 19, second.round);
        }
        // One round a week, the last one inside the season.
        assert_eq!(fixtures[0].day, 6);
        assert_eq!(fixtures[379].day, 265);
        assert!(fixtures[379].day < SEASON_DAYS);
    }

    /// Home and away are balanced: 19 of each, and never four in a row at one
    /// venue (three happens where the two legs meet).
    #[test]
    fn venues_are_balanced() {
        let fixtures = season_fixtures(7);
        for club in 0..20_u8 {
            let venues: Vec<bool> = fixtures
                .iter()
                .filter(|f| f.home == club || f.away == club)
                .map(|f| f.home == club)
                .collect();
            assert_eq!(venues.len(), 38);
            assert_eq!(
                venues.iter().filter(|&&home| home).count(),
                19,
                "club {club}"
            );
            let runs_of_four = venues
                .windows(4)
                .filter(|w| w.iter().all(|&v| v == w[0]))
                .count();
            assert_eq!(runs_of_four, 0, "club {club}: four in a row at one venue");
        }
    }

    #[test]
    fn match_seeds_depend_on_the_world_and_on_the_match() {
        let a = season_fixtures(7);
        let b = season_fixtures(8);
        assert_eq!(a, season_fixtures(7));
        assert_eq!(a.iter().map(|f| f.seed).collect::<HashSet<_>>().len(), 380);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((x.round, x.home, x.away), (y.round, y.home, y.away));
            assert_ne!(x.seed, y.seed);
        }
    }
}
