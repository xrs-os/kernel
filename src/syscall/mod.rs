use crate::proc::thread::Thread;
use alloc::sync::Arc;
use core::{mem, ptr, slice};

mod fs;
mod proc;
mod syscall_table;

use crate::fs::{vfs, Path};
use fs::{
    sys_fstat, sys_fstatat, sys_lseek, sys_openat, sys_read, sys_write, FStatAtFlags, LSeekWhence,
    OpenFlags, Stat,
};
use proc::{sys_exit, sys_fork};
use syscall_table::*;
pub type Result = core::result::Result<usize, Error>;

#[repr(u8)]
#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum Error {
    UNKNOWM = 0,
    /// No such file or directory
    ENOENT = 2,
    /// No such process
    ESRCH = 3,
    /// I/O error
    EIO = 5,
    /// Exec format error
    ENOEXEC = 8,
    /// fd is not a valid file descriptor.
    EBADF = 9,
    /// Try again
    EAGAIN = 11,
    /// Out of memory
    ENOMEM = 12,
    /// File exists
    EEXIST = 17,
    /// Not a directory.
    ENOTDIR = 20,
    /// Invalid flag specified in flags.
    EINVAL = 22,
    /// Too many open files
    EMFILE = 24,
    /// No space left on device
    ENOSPC = 28,
    /// Read-only file system
    EROFS = 30,
    /// Function not implemented
    ENOSYS = 38,
}

pub async fn syscall(thread: &Arc<Thread>) {
    let (syscall_num, syscall_args) = {
        let thread_inner = thread.inner.read();
        (
            thread_inner.context.get_syscall_num(),
            thread_inner.context.get_syscall_args(),
        )
    };

    let res = match syscall_num {
        SYS_OPENAT => unsafe {
            let path_ptr = syscall_args[1] as *const u8;
            sys_openat(
                thread,
                syscall_args[0] as isize,
                path(path_ptr),
                mem::transmute::<_, OpenFlags>(syscall_args[2]),
                mem::transmute::<_, vfs::Mode>(syscall_args[3] as u16),
            )
            .await
        },
        SYS_LSEEK => match LSeekWhence::from_primitive(syscall_args[2] as u8) {
            Some(whence) => {
                sys_lseek(thread, syscall_args[0], syscall_args[1] as i64, whence).await
            }
            None => Err(Error::EINVAL),
        },
        SYS_READ => {
            sys_read(
                thread,
                syscall_args[0],
                syscall_args[1] as *mut u8,
                syscall_args[2],
            )
            .await
        }
        SYS_WRITE => {
            sys_write(
                thread,
                syscall_args[0],
                syscall_args[1] as *const u8,
                syscall_args[2],
            )
            .await
        }
        SYS_NEWFSTATAT => unsafe {
            let path_ptr = syscall_args[1] as *const u8;
            sys_fstatat(
                thread,
                syscall_args[0] as isize,
                path(path_ptr),
                mem::transmute::<_, &mut Stat>(syscall_args[2]),
                mem::transmute::<_, FStatAtFlags>(syscall_args[3] as u32),
            )
            .await
        },
        SYS_FSTAT => unsafe {
            sys_fstat(
                thread,
                syscall_args[0] as isize,
                mem::transmute::<_, &mut Stat>(syscall_args[1]),
            )
            .await
        },
        SYS_EXIT => sys_exit(thread, syscall_args[0] as isize),
        SYS_CLONE => sys_fork(thread),
        _ => Err(Error::ENOSYS),
    };

    match res {
        Ok(ret) => thread.inner.write().context.set_syscall_ret(ret),
        Err(_) => todo!(),
    }
}

unsafe fn path(path_ptr: *const u8) -> &'static Path {
    Path::from_bytes(slice::from_raw_parts(path_ptr, c_str_len(path_ptr)))
}

unsafe fn c_str_len(mut str_ptr: *const u8) -> usize {
    if str_ptr.is_null() {
        0
    } else {
        let mut cnt = 0;
        loop {
            let c = ptr::read(str_ptr);
            if c == 0 {
                break cnt;
            }
            str_ptr = str_ptr.add(1);
            cnt += 1;
        }
    }
}
