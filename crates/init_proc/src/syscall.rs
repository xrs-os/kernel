enum SyscallNum {
    Exit = 2,
}

macro_rules! syscall {
    ($($name:ident($a:ident, $($b:ident, $($c:ident, $($d:ident, $($e:ident, $($f:ident, )?)?)?)?)?);)+) => {
        $(
            #[allow(dead_code)]
            unsafe fn $name($a: SyscallNum, $($b: usize, $($c: usize, $($d: usize, $($e: usize, $($f: usize)?)?)?)?)?) -> usize {
                let ret: usize;
                let syscall_num = $a as usize;
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                asm!(
                    "ecall",
                    in("a7") syscall_num,
                    $(
                        in("a0") $b,
                        $(
                            in("a1") $c,
                            $(
                                in("a2") $d,
                                $(
                                    in("a3") $e,
                                    $(
                                        in("a4") $f,
                                    )?
                                )?
                            )?
                        )?
                    )?
                    lateout("a0") ret,
                    options(nostack),
                );
                ret
            }
        )+
    };
}

syscall! {
    syscall0(a,);
    syscall1(a, b,);
    syscall2(a, b, c,);
    syscall3(a, b, c, d,);
    syscall4(a, b, c, d, e,);
    syscall5(a, b, c, d, e, f,);
}

pub fn sys_exit(status: isize) -> ! {
    unsafe { syscall1(SyscallNum::Exit, status as usize) };
    unreachable!()
}
