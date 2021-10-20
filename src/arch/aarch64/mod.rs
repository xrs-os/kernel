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
