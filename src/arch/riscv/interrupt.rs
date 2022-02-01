#[cfg(target_arch = "riscv32")]
global_asm!(".equ XLENB, 4");
#[cfg(target_arch = "riscv64")]
global_asm!(".equ XLENB, 8");

global_asm!(include_str!("trap.asm"));

use crate::driver::{self, set_driver_irq_ack_fn};

use super::{plic::plic, sbi};
use alloc::boxed::Box;

use mm::VirtualAddress;
use riscv::register::{scause, sie, stval, stvec};

#[derive(Debug, Clone)]
#[repr(C)]
#[rustfmt::skip]
pub struct Context {
    pub epc: usize,     // Save the user program's PC
    pub sstatus: usize, // Save the data in the sstatus register when trap.
    pub ra: usize, pub sp: usize,  pub gp: usize,  pub tp: usize,
    pub t0: usize, pub t1: usize,  pub t2: usize,  pub s0: usize,
    pub s1: usize, pub a0: usize,  pub a1: usize,  pub a2: usize,
    pub a3: usize, pub a4: usize,  pub a5: usize,  pub a6: usize,
    pub a7: usize, pub s2: usize,  pub s3: usize,  pub s4: usize,
    pub s5: usize, pub s6: usize,  pub s7: usize,  pub s8: usize,
    pub s9: usize, pub s10: usize, pub s11: usize, pub t3: usize,
    pub t4: usize, pub t5: usize,  pub t6: usize,
}

impl Context {
    pub fn sp(&self) -> usize {
        self.sp
    }

    pub fn set_syscall_ret(&mut self, val: usize) {
        self.a0 = val;
    }

    pub fn get_syscall_num(&self) -> usize {
        self.a7
    }

    pub fn get_syscall_args(&self) -> [usize; 6] {
        [self.a0, self.a1, self.a2, self.a3, self.a4, self.a5]
    }

    pub fn set_init_stack(&mut self, sp: VirtualAddress) {
        self.sp = sp.0;
    }

    pub fn set_entry_point(&mut self, pc: VirtualAddress) {
        self.epc = pc.0;
    }

    pub fn run_user(&mut self) -> *mut Trap {
        let trap = unsafe { _run_user(self) };
        unsafe {
            if let Trap::Syscall = *trap {
                // Skip ecall instruction
                self.epc += 4;
            }
        }

        trap
    }
}

#[derive(Debug)]
#[repr(C)]
pub enum Trap {
    PageFault(VirtualAddress),
    Syscall,
    Interrupt,
    Timer,
    Other,
}

impl Default for Context {
    fn default() -> Self {
        let mut sstatus: usize;
        unsafe { asm!("csrr {}, sstatus", out(reg) sstatus) };
        // Set spp to user
        sstatus &= !(1 << 8);
        // Set spie bit to 1, cpu will turn on interrupts after executing sret
        sstatus |= 1 << 5;
        #[rustfmt::skip]
        Self {
            epc: 0,
            sstatus,
            ra: 0, sp: 0,  gp: 0,  tp: 0,
            t0: 0, t1: 0,  t2: 0,  s0: 0,
            s1: 0, a0: 0,  a1: 0,  a2: 0,
            a3: 0, a4: 0,  a5: 0,  a6: 0,
            a7: 0, s2: 0,  s3: 0,  s4: 0,
            s5: 0, s6: 0,  s7: 0,  s8: 0,
            s9: 0, s10: 0, s11: 0, t3: 0,
            t4: 0, t5: 0,  t6: 0,
        }
    }
}

extern "C" {
    fn _trap_entry();
    fn _run_user(ctx: &mut Context) -> *mut Trap;
}

// Interrupt initialization
pub fn init() {
    // the interrupt handler entry address
    unsafe {
        stvec::write(
            _trap_entry as usize,
            riscv::register::mtvec::TrapMode::Direct,
        );
        init_timer();
        init_ext_irq();
        enable();
    }
}

/// Enables interrupts and returns the interrupt state before enabling
/// (true - enable interrupts, false - disable interrupts)
#[inline(always)]
pub unsafe fn enable() -> bool {
    let old: u8;
    asm!("csrrsi {}, sstatus, {sie}", out(reg) old, sie=const 1<<1);
    (old & (1 << 1)) == 1 << 1
}

/// Enables interrupts and enters low-power mode, returning to the interrupt state before enabling
#[inline(always)]
pub unsafe fn enable_and_wfi() -> bool {
    let old: u8;
    asm!("csrrsi {}, sstatus, {sie}", "wfi", out(reg) old, sie=const 1<<1);
    (old & (1 << 1)) == 1 << 1
}

/// Disable interrupts, and return to the interrupt state before disabling
#[inline(always)]
pub unsafe fn disable() -> bool {
    let old: u8;
    asm!("csrrci {}, sstatus, {sie}", out(reg) old,sie=const 1<<1);
    (old & (1 << 1)) == 1 << 1
}

/// Wait for the next interrupt
pub unsafe fn wfi() {
    asm!("wfi");
}

#[export_name = "_user_trap_handler"]
extern "C" fn user_trap_handler(_tf: &mut Context) -> *mut Trap {
    let scause = scause::read();
    let _stval = stval::read();
    // crate::println!("ucause: {:?}", scause.cause());
    // crate::println!("ustval: 0x{:x}", _stval);
    // crate::println!("usepc: 0x{:x}", riscv::register::sepc::read());
    Box::into_raw(Box::new(match scause.cause() {
        scause::Trap::Interrupt(scause::Interrupt::SupervisorTimer) => {
            crate::handler::on_timer(false);
            set_next_timer_interrupt();
            Trap::Timer
        }
        scause::Trap::Interrupt(scause::Interrupt::SupervisorExternal) => {
            external_handler();
            Trap::Interrupt
        }
        scause::Trap::Exception(scause::Exception::UserEnvCall) => Trap::Syscall,
        _ => Trap::Other,
    }))
}

#[export_name = "_kernel_trap_handler"]
extern "C" fn kernel_trap_handler(_ctx: &mut Context) {
    let scause = scause::read();
    let _stval = stval::read();
    // crate::println!("kernal cause: {:?}", scause.cause());
    // crate::println!("kernal stval: 0x{:x}", _stval);
    // crate::println!("kernal sepc: 0x{:x}", riscv::register::sepc::read());
    match scause.cause() {
        scause::Trap::Interrupt(scause::Interrupt::SupervisorTimer) => {
            crate::handler::on_timer(true);
            set_next_timer_interrupt();
        }
        scause::Trap::Interrupt(scause::Interrupt::SupervisorExternal) => external_handler(),
        _ => {}
    }
}

fn external_handler() {
    let irq_num = unsafe { plic().plic_claim() };
    if let Some(ack_fn) = driver::driver_irq_ack_fn(&irq_num) {
        ack_fn();
    }
    unsafe { plic().plic_complete(irq_num) }
}

// init timer
unsafe fn init_timer() {
    sie::set_stimer();
    set_next_timer_interrupt();
}

fn set_next_timer_interrupt() {
    #[cfg(target_arch = "riscv64")]
    pub fn get_cycle() -> u64 {
        use riscv::register::time;
        time::read() as u64
    }

    #[cfg(target_arch = "riscv32")]
    pub fn get_cycle() -> u64 {
        use riscv::register::{time, timeh};
        loop {
            let hi = timeh::read();
            let lo = time::read();
            let tmp = timeh::read();
            if hi == tmp {
                return ((hi as u64) << 32) | (lo as u64);
            }
        }
    }
    sbi::set_timer(get_cycle() + 9650000);
}

/// Enable external interrupt
unsafe fn init_ext_irq() {
    sie::set_sext();
}

pub unsafe fn register_external_irq(
    _interrupt_controller_num: u32,
    irq_num: u32,
    irq_ack_fn: Box<dyn Fn()>,
) {
    plic().register_external_irq(irq_num);
    set_driver_irq_ack_fn(irq_num, irq_ack_fn);
}
