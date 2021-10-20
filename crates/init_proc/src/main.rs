#![feature(lang_items)]
#![feature(asm)]
#![no_std]
#![no_main]

mod allocator;
mod lang_items;
mod syscall;

#[allow(clippy::empty_loop)]
pub fn main() {
    loop {}
}
