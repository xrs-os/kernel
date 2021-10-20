use core::{mem::MaybeUninit, ptr};

use mm::{Addr, VirtualAddress};

static mut PLIC: MaybeUninit<Plic> = MaybeUninit::uninit();

pub fn init(base_addr: VirtualAddress, hart: usize) {
    unsafe {
        PLIC = MaybeUninit::new(Plic::new(base_addr, hart));

        // set this hart's S-mode priority threshold to 0.
        ptr::write_volatile(
            base_addr
                .add(0x201000)
                .add(hart.wrapping_mul(0x2000))
                .as_mut_ptr(),
            0,
        );
    }
}

pub fn plic() -> &'static mut Plic {
    unsafe { PLIC.assume_init_mut() }
}

pub struct Plic {
    base_addr: VirtualAddress,
    hart: usize,
}

impl Plic {
    pub fn new(base_addr: VirtualAddress, hart: usize) -> Self {
        Self { base_addr, hart }
    }

    pub unsafe fn register_external_irq(&mut self, irq_num: u32) {
        let senable_p: *mut u32 = self
            .base_addr
            .add(0x2080)
            .add(self.hart.wrapping_mul(0x100))
            .as_mut_ptr();
        ptr::write_volatile(senable_p, *senable_p | 1 << irq_num);
        // set priority to 7
        ptr::write_volatile(self.base_addr.add(irq_num as usize * 4).as_mut_ptr(), 7);
    }

    fn plic_sclaim(&self) -> *mut u32 {
        self.base_addr
            .add(0x201004)
            .add(self.hart.wrapping_mul(0x2000))
            .as_mut_ptr()
    }

    /// ask the PLIC what interrupt we should serve.
    pub unsafe fn plic_claim(&self) -> u32 {
        ptr::read_volatile(self.plic_sclaim())
    }

    /// tell the PLIC we've served this IRQ.
    pub unsafe fn plic_complete(&self, irq: u32) {
        ptr::write_volatile(self.plic_sclaim(), irq)
    }
}
