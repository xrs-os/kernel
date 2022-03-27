use crate::proc::signal;
use core::arch::asm;
use core::mem;

use super::interrupt;

pub struct Context {
    pub ra: usize,
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
}

impl Context {
    pub fn from_interr_ctx(interr_ctx: &interrupt::Context) -> Self {
        Self {
            ra: interr_ctx.ra,
            sp: interr_ctx.sp,
            gp: interr_ctx.gp,
            tp: interr_ctx.tp,
            t0: interr_ctx.t0,
            t1: interr_ctx.t1,
            t2: interr_ctx.t2,
            s0: interr_ctx.s0,
            s1: interr_ctx.s1,
            a0: interr_ctx.a0,
            a1: interr_ctx.a1,
            a2: interr_ctx.a2,
            a3: interr_ctx.a3,
            a4: interr_ctx.a4,
            a5: interr_ctx.a5,
            a6: interr_ctx.a6,
            a7: interr_ctx.a7,
            s2: interr_ctx.s2,
            s3: interr_ctx.s3,
            s4: interr_ctx.s4,
            s5: interr_ctx.s5,
            s6: interr_ctx.s6,
            s7: interr_ctx.s7,
            s8: interr_ctx.s8,
            s9: interr_ctx.s9,
            s10: interr_ctx.s10,
            s11: interr_ctx.s11,
            t3: interr_ctx.t3,
            t4: interr_ctx.t4,
            t5: interr_ctx.t5,
            t6: interr_ctx.t6,
        }
    }

    pub fn fill_interr_ctx(&self, interr_ctx: &mut interrupt::Context) {
        interr_ctx.ra = self.ra;
        interr_ctx.sp = self.sp;
        interr_ctx.gp = self.gp;
        interr_ctx.tp = self.tp;
        interr_ctx.t0 = self.t0;
        interr_ctx.t1 = self.t1;
        interr_ctx.t2 = self.t2;
        interr_ctx.s0 = self.s0;
        interr_ctx.s1 = self.s1;
        interr_ctx.a0 = self.a0;
        interr_ctx.a1 = self.a1;
        interr_ctx.a2 = self.a2;
        interr_ctx.a3 = self.a3;
        interr_ctx.a4 = self.a4;
        interr_ctx.a5 = self.a5;
        interr_ctx.a6 = self.a6;
        interr_ctx.a7 = self.a7;
        interr_ctx.s2 = self.s2;
        interr_ctx.s3 = self.s3;
        interr_ctx.s4 = self.s4;
        interr_ctx.s5 = self.s5;
        interr_ctx.s6 = self.s6;
        interr_ctx.s7 = self.s7;
        interr_ctx.s8 = self.s8;
        interr_ctx.s9 = self.s9;
        interr_ctx.s10 = self.s10;
        interr_ctx.s11 = self.s11;
        interr_ctx.t3 = self.t3;
        interr_ctx.t4 = self.t4;
        interr_ctx.t5 = self.t5;
        interr_ctx.t6 = self.t6;
    }
}

pub fn set_signal_handler(
    interr_ctx: &mut interrupt::Context,
    sp: usize,
    handler: usize,
    flags: signal::SigActionFlags,
    signo: usize,
    siginfo: *const signal::Info,
) {
    interr_ctx.sp = sp;
    interr_ctx.epc = signal_handler_wapper as usize;
    interr_ctx.a0 = handler;
    interr_ctx.a1 = flags.bits();
    interr_ctx.a2 = signo;
    interr_ctx.a3 = siginfo as usize;
}

pub fn signal_handler_wapper() {
    #[inline(never)]
    unsafe fn inner(
        handler: usize,
        flags: signal::SigActionFlags,
        signo: usize,
        info: *const signal::Info,
    ) {
        let h: signal::SigHandler = mem::transmute::<usize, _>(handler);
        if flags.contains(signal::SigActionFlags::SIGINFO) {
            (h.info_handler)(signo, info)
        } else {
            (h.handler)(signo)
        }
    }

    let handler: usize;
    let signo: usize;
    let flags: usize;
    let info: usize;
    unsafe {
        asm!("mv {}, a0",  out(reg) handler);
        asm!("mv {}, a1",  out(reg) signo);
        asm!("mv {}, a2",  out(reg) flags);
        asm!("mv {}, a3",  out(reg) info);

        inner(
            handler,
            signal::SigActionFlags::from_bits_unchecked(flags),
            signo,
            info as *const signal::Info,
        );

        asm!("li a7, 139"); // SYS_rt_sigreturn
        asm!("ecall");
    }
}
