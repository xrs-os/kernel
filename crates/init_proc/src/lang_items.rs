use core::alloc::Layout;
use core::panic::PanicInfo;

use crate::allocator::{init_heap, Allocator};
use crate::main;
use crate::syscall::sys_exit;

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[no_mangle]
pub extern "C" fn _start(_argc: isize, _argv: *const *const u8) -> ! {
    init_heap();
    main();
    sys_exit(0)
}

#[lang = "eh_personality"]
fn eh_personality() {}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // println!("\n\n{}", info);
    sys_exit(1)
}

#[lang = "oom"]
fn oom(_: Layout) -> ! {
    panic!("out of memory");
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    panic!("abort");
}
