use alloc::vec::Vec;
use core::{cell::UnsafeCell, mem::MaybeUninit};

use crate::{
    arch::{self, interrupt},
    config,
};

pub fn init() {
    unsafe { CPUS = MaybeUninit::new(Cpus::new()) }
}

/// push_off and pop_off for disable and enable interrupts
/// They are matched，To undo 2 push_off operations, 2 pop_off operations are required
/// In addition, the push_off and pop_off operations will return to the original interrupt state when the pair completes
pub fn push_off() {
    unsafe { (*current()).push_off() }
}

pub fn pop_off() {
    unsafe { (*current()).pop_off() }
}

fn current() -> *mut Cpu {
    cpus().0[cpu_id()].get()
}

static mut CPUS: MaybeUninit<Cpus> = MaybeUninit::uninit();

fn cpus() -> &'static mut Cpus {
    unsafe { CPUS.assume_init_mut() }
}

struct Cpus(Vec<UnsafeCell<Cpu>>);

// Each CPU core will only access the corresponding `CPU` data
unsafe impl Sync for Cpus {}

impl Cpus {
    fn new() -> Self {
        let mut cpus = Vec::with_capacity(config::NCPU);
        for _ in 0..config::NCPU {
            cpus.push(UnsafeCell::new(Cpu::new()));
        }
        Cpus(cpus)
    }
}

#[inline(always)]
pub fn cpu_id() -> usize {
    arch::cpu_id()
}

struct Cpu {
    // depth of nesting of push_off()
    noff: isize,
    // Whether to turn on interrupts before calling push_off()
    intena: bool,
}

impl Cpu {
    fn new() -> Self {
        Self {
            noff: 0,
            intena: false,
        }
    }

    unsafe fn push_off(&mut self) {
        let old = interrupt::disable();
        if self.noff == 0 {
            self.intena = old;
        }
        self.noff += 1;
    }

    unsafe fn pop_off(&mut self) {
        self.noff -= 1;
        if self.noff == 0 && self.intena {
            interrupt::enable();
        }
    }
}
