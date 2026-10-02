//! Global allocator that counts allocations per thread (acceptance
//! criterion 17: no heap allocation in hot paths after bootstrap).
//!
//! Install it in a test binary with
//! `#[global_allocator] static A: CountingAlloc = CountingAlloc;`
//! and compare [`allocations`] before and after the code under test.
//! Counting is per thread so the test harness's own allocations on other
//! threads cannot interfere.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// Forwards to `System`, counting `alloc` and `realloc` calls.
pub struct CountingAlloc;

thread_local! {
    // const-initialised Cell without Drop: accessing it never allocates.
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

/// Allocations performed so far on the current thread.
#[must_use]
pub fn allocations() -> usize {
    ALLOCS.with(Cell::get)
}

// SAFETY: every method forwards its arguments unchanged to `System`, which
// upholds the `GlobalAlloc` contract; the counter touches no allocator state.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        // SAFETY: caller guarantees `layout` is valid (GlobalAlloc contract).
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by `System` with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        // SAFETY: forwarded verbatim; same contract as `GlobalAlloc::realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
