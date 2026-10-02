//! Acceptance criterion 17: `weekly_update` performs no heap allocation
//! after bootstrap. The counting allocator lives in `fm-render::ffi` (the
//! only place allowed to contain `unsafe`).

use fm_entities::{generate_database, PlayerId};
use fm_render::ffi::alloc_counter::{allocations, CountingAlloc};

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn weekly_update_does_not_allocate() {
    let mut db = generate_database(50_000, 77, 2026);
    let before = allocations();
    for week in 0..20 {
        for i in (0..50_000).step_by(3) {
            db.record_minutes(PlayerId(i), 90);
        }
        db.weekly_update(0xDEAD_BEEF, week);
    }
    let after = allocations();
    assert_eq!(
        after - before,
        0,
        "weekly_update allocated {} times",
        after - before
    );
}
