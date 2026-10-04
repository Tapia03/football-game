//! `cargo run --release -p fm-match --example timing` — per-match cost of
//! `tick_logic` alone (LOD Abstract) vs Full sampling, over 30 demo matches.
use fm_match::{demo::demo_match, LodLevel, MatchEngine};
use std::time::{Duration, Instant};
fn main() {
    let mut abs = Vec::new();
    let mut full = Vec::new();
    for seed in 0..30u64 {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        let t = Instant::now();
        while !e.is_finished() {
            e.tick_logic();
        }
        abs.push(t.elapsed());
        let mut e = MatchEngine::new(&setup, &db);
        let mut n = 0usize;
        let t = Instant::now();
        e.run(LodLevel::Full, |s| n += s.players.len());
        full.push(t.elapsed());
        assert!(n > 0);
    }
    let stat = |v: &mut Vec<Duration>| {
        v.sort();
        (v[0], v[v.len() / 2], v[v.len() - 1])
    };
    println!(
        "Abstract (tick_logic only): min/median/max {:?}",
        stat(&mut abs)
    );
    println!(
        "Full (tick_logic + 6 samples/tick): min/median/max {:?}",
        stat(&mut full)
    );
}
