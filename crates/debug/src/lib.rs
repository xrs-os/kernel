#![no_std]

mod arch;

#[macro_use]
extern crate alloc;

use alloc::fmt;

/// Print a string to the console.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::_print(format_args!($($arg)*));
    });
}

/// Print a string to the console,  with a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}

pub fn _print(args: fmt::Arguments) {
    for &c in format!("{}", args).as_bytes() {
        arch::console_putchar(c as usize)
    }
}
