//! set panic handler

use core::alloc::Layout;
use core::panic::PanicInfo;

use crate::arch::interrupt;
use crate::println;

#[lang = "eh_personality"]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

#[panic_handler]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);

    println!("WFI");
    loop {
        unsafe {
            interrupt::wfi();
        }
    }
}

#[lang = "oom"]
#[no_mangle]
pub fn rust_oom(_layout: Layout) -> ! {
    panic!("kernel memory allocation failed");
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {
        unsafe {
            interrupt::wfi();
        }
    }
}
