//! Acceptance criterion 17: `weekly_update` performs no heap allocation
//! after bootstrap. Counted per thread so the test harness's own allocations
//! on other threads cannot interfere.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use fm_entities::{generate_database, PlayerId};

struct CountingAlloc;

thread_local! {
    // const-initialised Cell without Drop: accessing it never allocates.
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

#[allow(unsafe_code)]
// SAFETY: forwards every call to `System` unchanged; only adds a counter.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn weekly_update_does_not_allocate() {
    let mut db = generate_database(50_000, 77, 2026);
    let before = ALLOCS.with(Cell::get);
    for week in 0..20 {
        for i in (0..50_000).step_by(3) {
            db.record_minutes(PlayerId(i), 90);
        }
        db.weekly_update(0xDEAD_BEEF, week);
    }
    let after = ALLOCS.with(Cell::get);
    assert_eq!(
        after - before,
        0,
        "weekly_update allocated {} times",
        after - before
    );
}
