//! A whole season, natively (spec Fase 7B): how long it takes and what the
//! league looks like. Slow in a debug build, so it only runs when asked
//! for: `cargo test --release -p fm-world --test season -- --ignored --nocapture`.

use std::time::Instant;

use fm_world::{World, CLUBS, CLUB_NAMES, MATCHES, MATCHES_PER_ROUND, SEASON_DAYS};

#[test]
#[ignore = "a whole season: run in release"]
fn a_whole_season() {
    let mut world = World::new(2026, 0);
    let mut rounds = Vec::new();
    let start = Instant::now();
    while !world.finished() {
        let before = Instant::now();
        if !world.advance_day(|_, _| {}).is_empty() {
            rounds.push(before.elapsed());
        }
    }
    let total = start.elapsed();
    assert_eq!(world.day, SEASON_DAYS);
    assert!(world.fixtures.iter().all(|f| f.result.is_some()));
    assert_eq!(rounds.len(), MATCHES / MATCHES_PER_ROUND);
    rounds.sort();
    println!(
        "season (native, one thread): {total:.2?} for {MATCHES} matches; a round: median {:.0?}, slowest {:.0?}",
        rounds[rounds.len() / 2],
        rounds[rounds.len() - 1],
    );

    let results: Vec<_> = world.fixtures.iter().filter_map(|f| f.result).collect();
    let goals: u32 = results
        .iter()
        .map(|r| u32::from(r.home_goals) + u32::from(r.away_goals))
        .sum();
    let home_wins = results
        .iter()
        .filter(|r| r.home_goals > r.away_goals)
        .count();
    let draws = results
        .iter()
        .filter(|r| r.home_goals == r.away_goals)
        .count();
    #[allow(clippy::cast_precision_loss)] // 380 matches
    let per_match = f64::from(goals) / MATCHES as f64;
    println!(
        "goals per match {per_match:.2}; home wins {home_wins}, draws {draws}, away wins {}",
        MATCHES - home_wins - draws
    );
    let table = world.standings();
    for (place, row) in table.iter().enumerate() {
        let club = usize::from(row.club);
        println!(
            "{:>2}. {:<24} {:>3} pts  {:>2}-{:>2}-{:>2}  {:>3}:{:<3}  strength {}  {:?}",
            place + 1,
            CLUB_NAMES[club].0,
            row.points,
            row.won,
            row.drawn,
            row.lost,
            row.goals_for,
            row.goals_against,
            world.clubs[club].strength,
            world.clubs[club].formation,
        );
    }
    // Every club played the whole season.
    assert!(table.iter().all(|r| r.played == 38));
    assert_eq!(table.len(), CLUBS);
    let injured = world
        .players
        .dynamics()
        .iter()
        .filter(|d| d.injury_weeks > 0)
        .count();
    println!("injured at the end: {injured} of {}", world.players.len());
}
