//! Shared test-binary instrumentation: a thread-scoped counting allocator.
//!
//! Extracted from the PCM edge experiment's test binary so that executable
//! evidence in this crate measures allocations through one implementation.
//! Each test binary includes this file via `#[path]` and installs
//! [`CountingAllocator`] as its own `#[global_allocator]`.
//!
//! A counting global allocator gives *measured* allocation evidence for
//! steady-state loops, replacing cooperative self-reporting counters (which
//! could only see allocations the code under test chose to report).
//!
//! Counting is armed per thread and per measurement window: allocations by
//! other threads (including test-harness coordination and test dispatch) are
//! excluded, so the count attributes heap work performed by the measured
//! closure itself. This keeps the measurement deterministic under parallel
//! test execution.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOC_WINDOW_ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOC_WINDOW_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOC_WINDOW_OBSERVER: Cell<Option<fn()>> = const { Cell::new(None) };
}

pub struct CountingAllocator;

// SAFETY: forwards every allocation call to the system allocator; the
// bookkeeping touches only const-initialized thread-locals without
// destructors, so it performs no allocation of its own on any path.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_if_armed();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_if_armed();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_if_armed();
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[inline]
fn record_if_armed() {
    ALLOC_WINDOW_ARMED.with(|armed| {
        if armed.get() {
            ALLOC_WINDOW_COUNT.with(|count| count.set(count.get() + 1));
            ALLOC_WINDOW_OBSERVER.with(|observer| {
                if let Some(observe) = observer.get() {
                    observe();
                }
            });
        }
    });
}

/// Runs `f` with allocation counting armed for the current thread only and
/// returns `(f's result, allocations performed)`.
pub fn run_counting_allocations<R>(f: impl FnOnce() -> R) -> (R, usize) {
    struct DisarmOnDrop;
    impl Drop for DisarmOnDrop {
        fn drop(&mut self) {
            ALLOC_WINDOW_ARMED.with(|armed| armed.set(false));
        }
    }
    ALLOC_WINDOW_COUNT.with(|count| count.set(0));
    ALLOC_WINDOW_ARMED.with(|armed| armed.set(true));
    let _disarm = DisarmOnDrop;
    let result = f();
    let count = ALLOC_WINDOW_COUNT.with(|count| count.get());
    (result, count)
}

/// Observes actual allocator calls within the existing thread-local counting
/// window. The observer must neither allocate nor panic (GlobalAlloc rules).
pub fn run_observing_allocations<R>(observer: fn(), f: impl FnOnce() -> R) -> (R, usize) {
    struct RestoreObserver(Option<fn()>);
    impl Drop for RestoreObserver {
        fn drop(&mut self) {
            ALLOC_WINDOW_OBSERVER.with(|observer| observer.set(self.0));
        }
    }
    let _restore = RestoreObserver(ALLOC_WINDOW_OBSERVER.with(|cell| cell.replace(Some(observer))));
    run_counting_allocations(f)
}
