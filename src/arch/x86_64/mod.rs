pub struct Printer;

impl crate::console::Printer for Printer {
    fn putchar(_: u8) {
        todo!()
    }
}

pub struct CpuId;

impl crate::cpu::CpuId for CpuId {
    fn cpu_id() -> usize {
        todo!()
    }
}

pub struct Interrupt;
impl crate::cpu::Interrupt for Interrupt {
    unsafe fn enable() -> bool {
        todo!()
    }

    unsafe fn enable_and_halt() -> bool {
        todo!()
    }

    unsafe fn disable() -> bool {
        todo!()
    }
}

pub mod mm {
    use alloc::vec::Vec;

    use crate::mm::memory::Segment;

    pub fn kernel_segments() -> Vec<Segment> {
        todo!()
    }
}

pub mod consts {
    use crate::mm::PhysicalAddress;

    // Start address of the user stack
    pub const USER_STACK_OFFSET: usize = 0x3f_ffff_f000;
    // User Stack Size (1MB)
    pub const USER_STACK_SIZE: usize = 1024 * 1024;
    // Memory end address
    pub const MEMORY_END_ADDRESS: PhysicalAddress = PhysicalAddress(0x88000000);
}
