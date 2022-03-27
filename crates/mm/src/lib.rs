#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![no_std]

extern crate alloc;

pub mod arch;
pub mod frame;
pub mod memory;
pub mod page;

use core::{fmt, iter::Iterator, ops::Range};

pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    AddressOverlap(Range<VirtualAddress>, Range<VirtualAddress>),
    NoSpace,
    InvalidVirtualAddress(VirtualAddress),
    InvalidPageTable(usize),
}

pub trait Addr: Sized {
    fn new(inner: usize) -> Self;

    fn inner(&self) -> usize;

    #[inline(always)]
    fn add(self, offset: usize) -> Self {
        Self::new(self.inner().wrapping_add(offset))
    }

    fn is_align_to(&self, align_shift: usize) -> bool {
        self.inner() & ((1 << align_shift) - 1) == 0
    }

    fn align_down_to(&self, to_size: usize) -> Self {
        Self::new(self.inner() / to_size * to_size)
    }
}

/// Physical memory address
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PhysicalAddress(pub usize);

impl Addr for PhysicalAddress {
    #[inline(always)]
    fn new(address: usize) -> Self {
        Self(address)
    }

    fn inner(&self) -> usize {
        self.0
    }
}

/// Virtual memory address
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct VirtualAddress(pub usize);

impl VirtualAddress {
    pub fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }
}

impl Addr for VirtualAddress {
    fn new(inner: usize) -> Self {
        Self(inner)
    }

    fn inner(&self) -> usize {
        self.0
    }
}

impl From<usize> for PhysicalAddress {
    fn from(raw: usize) -> Self {
        Self(raw)
    }
}

impl From<usize> for VirtualAddress {
    fn from(raw: usize) -> Self {
        Self(raw)
    }
}

impl fmt::Display for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::write!(f, "0x{:x}", self.0)
    }
}

impl fmt::Display for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::write!(f, "0x{:x}", self.0)
    }
}

impl fmt::Debug for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::write!(f, "VirtualAddress(0x{:x})", self.0)
    }
}

impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::write!(f, "PhysicalAddress(0x{:x})", self.0)
    }
}

pub type Page = Space<VirtualAddress>;
pub type Frame = Space<PhysicalAddress>;

pub type PageIter<'a, const PAGE_SIZE: usize> = SpaceIter<'a, VirtualAddress, PAGE_SIZE>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Space<T: Addr>(T);

impl<T: Addr + Clone> Space<T> {
    pub fn of_addr(addr: T) -> Self {
        Self(addr)
    }

    pub fn start(&self) -> T {
        self.0.clone()
    }
}

impl<T: Addr> From<T> for Space<T> {
    fn from(addr: T) -> Self {
        Self(addr)
    }
}

pub struct SpaceIter<'a, T: Addr, const SIZE: usize> {
    end: &'a T,
    next: T,
}

impl<'a, T: Addr + Clone, const SIZE: usize> SpaceIter<'a, T, SIZE> {
    pub fn new(range: &'a Range<T>) -> Self {
        Self {
            end: &range.end,
            next: range.start.align_down_to(SIZE),
        }
    }
}

impl<'a, T, const SIZE: usize> Iterator for SpaceIter<'a, T, SIZE>
where
    T: Addr + PartialOrd + Clone,
{
    type Item = Space<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if &self.next > self.end {
            None
        } else {
            let next = self.next.clone();
            self.next = self.next.clone().add(SIZE);
            Some(Space::of_addr(next))
        }
    }
}
