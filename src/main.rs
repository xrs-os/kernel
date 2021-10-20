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
#![allow(incomplete_features)]
#![allow(dead_code)]

use alloc::sync::Arc;
use arch::interrupt as interruptA;

use spinlock::MutexIrq;

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
    fs::init(create_fs_inner());
    proc::init();

    loop {
        proc::executor::run_ready_tasks();
        unsafe { interruptA::wfi() };
    }
}

mod handler {
    pub fn on_timer() {
        // println!("timer tiggered");
    }
}

fn create_fs_inner() -> Arc<dyn fs::mount_fs::DynFilesystem> {
    let blk_device = driver::blk_drivers()
        .first()
        .expect("No block device could be found.")
        .clone();

    #[cfg(feature = "naive_fs")]
    {
        let naivefs = proc::executor::block_on(async {
            Arc::new(
                naive_fs::NaiveFs::<MutexIrq<()>, _>::open(fs::Disk::new(blk_device), false)
                    .await
                    .expect("Failed to open naive filesystem."),
            )
        });
        Arc::new(naivefs) // TODO trace err
    }
}
