use crate::kmain;
use core::arch::{asm, global_asm};

const BOOT_STACK_SIZE: usize = (1 << 16) * 8;

#[repr(C)]
struct BootStack([u8; BOOT_STACK_SIZE]);

#[link_section = ".bss"]
#[export_name = "_bootstack"]
static mut BOOT_STACK: BootStack = BootStack([0; BOOT_STACK_SIZE]);

extern "C" {
    static mut _boot_page_table: usize;
}

#[export_name = "_boot"]
extern "C" fn boot(hartid: usize, dtb_pa: usize) -> ! {
    // Write hartid to tp register for cpu_id()
    unsafe { asm!("mv tp, {}", in(reg) hartid) };
    // Allow kernel access to user pages
    unsafe { riscv::register::sstatus::set_sum() };
    kmain(hartid, dtb_pa);
    unreachable!();
}

global_asm!(include_str!("entry.asm"));
