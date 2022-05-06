#![feature(lang_items)]
#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use syscall::{sys_clone, sys_nanosleep, sys_openat, sys_write, Timespec};

mod allocator;
mod lang_items;
mod syscall;

const AT_FDCWD: isize = -100;

#[allow(clippy::empty_loop)]
pub fn main() {
    let tty0 = sys_openat(AT_FDCWD, b"/dev/tty\0", 2, 0);
    // let _tty1 = sys_openat(AT_FDCWD, b"/dev/tty\0", 2, 0);

    sys_write(
        tty0,
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

    loop {
        sys_nanosleep(Timespec { sec: 1, nsec: 0 });
        if pid == 0 {
            sys_write(tty0, "subproc\n".as_bytes());
        } else {
            sys_write(tty0, "parent proc\n".as_bytes());
        }
    }
}
