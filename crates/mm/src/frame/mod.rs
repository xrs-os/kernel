use alloc::vec::Vec;

use super::Frame;
use super::PhysicalAddress;

pub mod allocator;

pub trait Allocator {
    fn init(&mut self, _start: PhysicalAddress, _end: PhysicalAddress) {}

    fn alloc(&mut self) -> Option<Frame>;

    fn alloc_consecutive(&mut self, n: usize) -> Vec<Frame>;

    fn dealloc(&mut self, frame: &Frame) -> bool;
}

// Align up `pa` by `frame_size`
const fn farme_round_up(pa: PhysicalAddress, frame_size: usize) -> PhysicalAddress {
    PhysicalAddress((pa.0 + frame_size - 1) & !(frame_size - 1))
}

pub struct LockedAllocator<MutexType, A> {
    inner: lock_api::Mutex<MutexType, A>,
}

impl<MutexType, A> LockedAllocator<MutexType, A>
where
    MutexType: lock_api::RawMutex,
    A: Allocator,
{
    pub const fn new(allocator: A) -> Self {
        Self {
            inner: lock_api::Mutex::new(allocator),
        }
    }

    pub fn init(&self, start: PhysicalAddress, end: PhysicalAddress) {
        self.inner.lock().init(start, end);
    }

    pub fn alloc(&self) -> Option<Frame> {
        self.inner.lock().alloc()
    }

    pub fn alloc_consecutive(&self, n: usize) -> Vec<Frame> {
        self.inner.lock().alloc_consecutive(n)
    }

    pub fn dealloc(&self, frame: &Frame) -> bool {
        self.inner.lock().dealloc(frame)
    }
}
