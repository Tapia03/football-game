//! Acceptance criterion 17: `MatchEngine::tick_logic` performs no heap
//! allocation after construction (`MatchEngine::new` pre-allocates).

use fm_match::demo::demo_match;
use fm_match::MatchEngine;
use fm_test_utils::alloc_counter::{allocations, CountingAlloc};

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn tick_logic_does_not_allocate() {
    let (db, setup) = demo_match(5);
    let mut engine = MatchEngine::new(&setup, &db);
    let before = allocations();
    while !engine.is_finished() {
        engine.tick_logic();
    }
    let after = allocations();
    assert_eq!(
        after - before,
        0,
        "tick_logic allocated {} times",
        after - before
    );
    assert!(!engine.events().is_empty());
}
