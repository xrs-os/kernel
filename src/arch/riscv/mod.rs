mod boot;
pub mod consts;
pub mod interrupt;
pub mod memory;
pub mod plic;
#[allow(dead_code)]
mod sbi;
pub mod signal;

pub fn putchar(c: u8) {
    sbi::console_putchar(c as usize);
}

pub fn getchar() -> u8 {
    sbi::console_getchar() as u8
}

pub fn cpu_id() -> usize {
    let id: usize;
    unsafe {
        asm!("mv {},tp", out(reg) id);
    }
    id
}
