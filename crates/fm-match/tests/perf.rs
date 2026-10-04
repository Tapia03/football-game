//! Performance gate (SPEC §0.17): a full match in LOD Abstract must cost
//! ≤ 50 ms (target 40 ms + 25% margin). Release only:
//! `cargo test --release -p fm-match --test perf -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use fm_match::demo::demo_match;
use fm_match::MatchEngine;

#[test]
#[ignore = "timing gate; run in release (CI job `bench`)"]
fn full_match_abstract_under_50ms() {
    let mut times = Vec::new();
    for seed in [1_u64, 2, 3, 4, 5, 6, 7] {
        let (db, setup) = demo_match(seed);
        let mut engine = MatchEngine::new(&setup, &db);
        let t = Instant::now();
        while !engine.is_finished() {
            engine.tick_logic();
        }
        times.push(t.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    println!("tick_logic full match (Abstract): median of 7 = {median:?}, all = {times:?}");
    assert!(
        median <= Duration::from_millis(50),
        "too slow: median {median:?}"
    );
}
