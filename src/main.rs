#![feature(lang_items)]
#![feature(global_asm)]
#![feature(asm)]
#![feature(const_generics, const_evaluatable_checked)]
// make `std` available when testing
// #![cfg_attr(not(test), no_std)]
// #![cfg_attr(not(test), no_main)]
#![no_std]
#![no_main]
#![feature(const_fn_trait_bound)]
#![feature(fn_align)]
#![feature(stmt_expr_attributes)]
#![feature(test)]
#![feature(generic_associated_types)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(linked_list_cursors)]
#![feature(const_btree_new)]
#![feature(maybe_uninit_extra)]
#![feature(map_try_insert)]
#![allow(incomplete_features)]
#![allow(dead_code)]

use arch::interrupt as interruptA;

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate bitflags;

mod arch;
mod config;
mod console;
mod cpu;
// #[cfg(not(test))]
mod heap;
mod mm;
// #[cfg(not(test))]
mod panic;
mod proc;
mod spinlock;
#[macro_use]
mod macros;
mod driver;
mod fs;
mod sleeplock;
mod syscall;
mod time;

extern "C" {
    fn _bootstack();
}

// Kernel entry.
// #[cfg(not(test))]
fn kmain(_hartid: usize, dtb_pa: usize) {
    console::init();
    heap::init();
    interruptA::init();
    cpu::init();
    mm::init();
    driver::init(dtb_pa);
    fs::init();
    proc::init();

    loop {
        proc::executor::run_ready_tasks();
        unsafe { interruptA::wfi() };
    }
}

mod handler {
    pub fn on_timer(kernel: bool) {
        println!("timer tiggered. {}", if kernel { "kernel" } else { "user" });
    }
}
