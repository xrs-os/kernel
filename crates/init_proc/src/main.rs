#![feature(lang_items)]
#![feature(asm)]
#![no_std]
#![no_main]

use syscall::{sys_openat, sys_write};

mod allocator;
mod lang_items;
mod syscall;

const AT_FDCWD: isize = -100;

#[allow(clippy::empty_loop)]
pub fn main() {
    let tty = sys_openat(AT_FDCWD, "/dev/tty", 2, 0);
    sys_write(tty, "hello world".as_bytes());
    loop {}
}
