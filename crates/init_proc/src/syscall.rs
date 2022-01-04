enum SyscallNum {
    Openat = 56,
    Close = 57,
    Read = 63,
    Write = 64,
    Exit = 93,
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

#[allow(dead_code)]
pub fn sys_exit(status: isize) -> ! {
    unsafe { syscall1(SyscallNum::Exit, status as usize) };
    unreachable!()
}

#[allow(dead_code)]
pub fn sys_openat(dirfd: isize, path: &[u8], flags: usize, mode: u16) -> isize {
    unsafe {
        syscall4(
            SyscallNum::Openat,
            dirfd as usize,
            path.as_ptr() as usize,
            flags,
            mode as usize,
        ) as isize
    }
}

#[allow(dead_code)]
pub fn sys_close(fd: isize) -> usize {
    unsafe { syscall1(SyscallNum::Close, fd as usize) }
}

#[allow(dead_code)]
pub fn sys_read(fd: isize, buf: &mut [u8]) -> usize {
    unsafe {
        syscall3(
            SyscallNum::Read,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
        )
    }
}

#[allow(dead_code)]
pub fn sys_write(fd: isize, buf: &[u8]) -> usize {
    unsafe {
        syscall3(
            SyscallNum::Write,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
        )
    }
}
