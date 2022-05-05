#![feature(lang_items)]
#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use alloc::format;
use syscall::{sys_clone, sys_openat, sys_write};

mod allocator;
mod lang_items;
mod syscall;

const AT_FDCWD: isize = -100;

#[allow(clippy::empty_loop)]
pub fn main() {
    let _tty0 = sys_openat(AT_FDCWD, b"/dev/tty\0", 2, 0);
    let _tty1 = sys_openat(AT_FDCWD, b"/dev/tty\0", 2, 0);

    sys_write(
        _tty0,
        r#"
    ██      ██   ███████    ████████       ███████    ████████
    ░░██   ██   ██░░░░░██  ██░░░░░░       ░██░░░░██  ██░░░░░░ 
     ░░██ ██   ██     ░░██░██             ░██   ░██ ░██       
      ░░███   ░██      ░██░█████████ █████░███████  ░█████████
       ██░██  ░██      ░██░░░░░░░░██░░░░░ ░██░░░██  ░░░░░░░░██
      ██ ░░██ ░░██     ██        ░██      ░██  ░░██        ░██
     ██   ░░██ ░░███████   ████████       ░██   ░░██ ████████ 
    ░░     ░░   ░░░░░░░   ░░░░░░░░        ░░     ░░ ░░░░░░░░  
"#
        .as_bytes(),
    );

    let pid = sys_clone();
    sys_write(_tty0, format!("pid: {}\n", pid).as_bytes());
    loop {}
}
