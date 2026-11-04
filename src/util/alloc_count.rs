//! A counting global allocator for the test binary, so the frame profile can
//! report how much each interaction allocates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

struct CountingAllocator;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

// SAFETY: every call forwards to `System` unchanged; the counters are side effects only.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation(layout.size());
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation(new_size);
        LIVE_BYTES.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: as for `dealloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn count_allocation(size: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    ALLOCATED_BYTES.fetch_add(size as u64, Relaxed);
    LIVE_BYTES.fetch_add(size as u64, Relaxed);
}

/// Bytes allocated and not yet freed.
pub fn live_bytes() -> u64 {
    LIVE_BYTES.load(Relaxed)
}

/// Allocations and bytes requested since the process started.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllocationTally {
    pub count: u64,
    pub bytes: u64,
}

impl AllocationTally {
    pub fn now() -> Self {
        Self {
            count: ALLOCATIONS.load(Relaxed),
            bytes: ALLOCATED_BYTES.load(Relaxed),
        }
    }

    pub fn since(self, earlier: Self) -> Self {
        Self {
            count: self.count - earlier.count,
            bytes: self.bytes - earlier.bytes,
        }
    }
}
