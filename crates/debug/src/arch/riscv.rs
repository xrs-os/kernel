use core::arch::asm;

#[inline(always)]
fn sbi_call_legacy(which: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let ret;
    unsafe {
        asm!(
            "ecall",
            in("a0") arg0, in("a1") arg1, in("a2") arg2,
            in("a7") which,
            lateout("a0") ret,
        )
    };
    ret
}

const SBI_CONSOLE_PUTCHAR: usize = 1;

pub fn console_putchar(c: usize) {
    sbi_call_legacy(SBI_CONSOLE_PUTCHAR, c, 0, 0);
}
