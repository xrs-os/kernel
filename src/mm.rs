use mm::{
    frame::{allocator::BumpAllocator, LockedAllocator},
    memory::Memory,
    page::mapper::PageMapper,
    page::PageParam as _,
    Result,
};

use crate::{arch::memory::memory_range, spinlock::MutexIrq};

pub use mm::arch::page::PageParam as PageParamA;

type Allocator = BumpAllocator<{ PageParamA::PAGE_SIZE }>;
pub type Mem = Memory<'static, MutexIrq<()>, Allocator, PageParamA>;

static FRAME_ALLOCATOR: LockedAllocator<MutexIrq<()>, Allocator> =
    LockedAllocator::new(Allocator::uninit());

pub fn init() {
    let (start, end) = memory_range();
    FRAME_ALLOCATOR.init(start, end)
}

pub fn frame_allocator() -> &'static LockedAllocator<MutexIrq<()>, Allocator> {
    &FRAME_ALLOCATOR
}

pub fn new_memory() -> Result<Memory<'static, MutexIrq<()>, Allocator, PageParamA>> {
    Ok(Memory::new(PageMapper::create(frame_allocator())?))
}
