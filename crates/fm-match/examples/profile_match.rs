//! `valgrind --tool=callgrind target/release/examples/profile_match` — one
//! full match in Abstract, for profiling `tick_logic` (SPEC Fase 5 budget).
use fm_match::{demo::demo_match, MatchEngine};

fn main() {
    let (db, setup) = demo_match(1);
    let mut e = MatchEngine::new(&setup, &db);
    while !e.is_finished() {
        e.tick_logic();
    }
    println!("{} events", e.events().len());
}
