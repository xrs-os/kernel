#![feature(lang_items)]
// make `std` available when testing
// #![cfg_attr(not(test), no_std)]
// #![cfg_attr(not(test), no_main)]
#![feature(ready_macro)]
#![feature(asm_const)]
#![no_std]
#![no_main]
#![feature(fn_align)]
#![feature(test)]
#![feature(generic_associated_types)]
#![feature(linked_list_cursors)]
#![feature(map_try_insert)]
#![feature(stmt_expr_attributes)]
#![allow(incomplete_features)]
#![allow(dead_code)]
#![feature(const_btree_new)]

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
        unsafe {
            // When there is no task in the operating system,
            // it is necessary to turn on interrupts to allow external interrupts so that wake can be called
            interruptA::enable_and_wfi();
        };
    }
}

mod handler {
    pub fn on_timer(kernel: bool) {
        // println!("timer tiggered. {}", if kernel { "kernel" } else { "user" });
    }
}
