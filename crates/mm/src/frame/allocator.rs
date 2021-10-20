use core::usize;

use alloc::vec::Vec;

use super::{farme_round_up, Allocator, Frame, PhysicalAddress};

pub struct BumpAllocator<const FRAME_SIZE: usize> {
    next: PhysicalAddress,
    start: PhysicalAddress,
    end: PhysicalAddress,
    allocated: usize,
}

impl<const FRAME_SIZE: usize> BumpAllocator<FRAME_SIZE> {
    pub const fn uninit() -> Self {
        Self::new((PhysicalAddress(0), PhysicalAddress(0)))
    }
    pub const fn new((mut start, end): (PhysicalAddress, PhysicalAddress)) -> Self {
        start = farme_round_up(start, FRAME_SIZE);
        Self {
            next: start,
            start,
            end,
            allocated: 0,
        }
    }
}

impl<const FRAME_SIZE: usize> Allocator for BumpAllocator<FRAME_SIZE> {
    fn init(&mut self, start: PhysicalAddress, end: PhysicalAddress) {
        self.start = farme_round_up(start, FRAME_SIZE);
        self.next = self.start;
        self.end = end;
    }

    fn alloc(&mut self) -> Option<Frame> {
        if self.next.0 < self.end.0 - FRAME_SIZE {
            let frame = Frame::of_addr(self.next);
            self.next.0 += FRAME_SIZE;
            self.allocated += 1;
            Some(frame)
        } else {
            None
        }
    }

    fn alloc_consecutive(&mut self, n: usize) -> Vec<Frame> {
        let mut frames = Vec::with_capacity(n);
        for _ in 0..n {
            match self.alloc() {
                Some(f) => frames.push(f),
                None => {
                    for f in frames {
                        self.dealloc(&f);
                    }
                    return Vec::new();
                }
            }
        }

        frames
    }

    fn dealloc(&mut self, _frame: &Frame) -> bool {
        self.allocated -= 1;
        if self.allocated == 0 {
            self.next = self.start;
        }
        true
    }
}
