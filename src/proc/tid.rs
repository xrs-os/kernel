use crate::{config, spinlock::MutexIrq};
use bitmap::Bitmap;

use core::mem::MaybeUninit;

static mut THREAD_ID_ALLOCATOR: MaybeUninit<ThreadIdAllocator> = MaybeUninit::uninit();

/// Initialize the thread id module
pub fn init() {
    unsafe { THREAD_ID_ALLOCATOR = MaybeUninit::new(ThreadIdAllocator::new()) }
}

/// Allocate thread id, return None means no thread id is free.
pub fn alloc() -> Option<ThreadId> {
    unsafe { THREAD_ID_ALLOCATOR.assume_init_ref().alloc() }
}

pub type RawThreadId = u32;

/// A thread id allocator.
/// The deallocated thread id can be reallocated.
pub struct ThreadIdAllocator(MutexIrq<Inner>);

struct Inner {
    last_id: RawThreadId,
    free: u32, // Number of unallocated ids.
    tidmap: Bitmap,
}

impl ThreadIdAllocator {
    fn new() -> Self {
        Self(MutexIrq::new(Inner {
            last_id: 0,
            free: config::MAX_THREAD_ID,
            tidmap: Bitmap::new(config::MAX_THREAD_ID),
        }))
    }

    /// Allocate thread id, return None means no thread id is free.
    pub fn alloc(&self) -> Option<ThreadId> {
        let mut inner = self.0.lock();
        if inner.free == 0 {
            return None;
        }
        let mut id = if inner.last_id > config::MAX_THREAD_ID {
            config::THREAD_RESERVED_ID
        } else {
            inner.last_id + 1
        };

        if inner.tidmap.test_and_set(id, true) {
            // `id` has been allocated.
            id = if let Some(newid) = inner.tidmap.find_next_zero(id, None) {
                newid
            } else {
                inner
                    .tidmap
                    .find_next_zero(config::THREAD_RESERVED_ID + 1, None)?
            };
            inner.tidmap.test_and_set(id, true);
        }
        inner.last_id = id;
        inner.free -= 1;
        Some(ThreadId(id))
    }

    /// Deallocate `thread_id`,
    /// returns false indicating that the `thread_id` has been allocated or has never been allocated.
    fn dealloc(&self, thread_id: RawThreadId) -> bool {
        let mut inner = self.0.lock();
        let old = inner.tidmap.test_and_set(thread_id, false);
        inner.free += 1;
        if inner.last_id == thread_id {
            inner.last_id -= 1;
        }
        old
    }
}

/// `ThreadId` represents a thread id.
pub struct ThreadId(RawThreadId);

impl ThreadId {
    /// Returns thread id.
    pub fn id(&self) -> &RawThreadId {
        &self.0
    }
}

impl Drop for ThreadId {
    fn drop(&mut self) {
        unsafe { THREAD_ID_ALLOCATOR.assume_init_ref().dealloc(*self.id()) };
    }
}
