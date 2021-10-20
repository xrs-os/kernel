use alloc::sync::Arc;

// use self::proc::sys_fork;
use crate::proc::thread::Thread;

use self::proc::{sys_exit, sys_fork};
mod proc;

pub type Result = core::result::Result<usize, Error>;

#[repr(u8)]
#[derive(Debug)]
pub enum Error {
    /// Unknown
    Unknown = 0,
    /// Out of memory
    OutOfMem = 1,
    /// Try again
    TryAgain = 2,
    /// Function not implemented
    NoSys = 3,
    /// Exec format error
    NoExec = 4,
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
        1 => sys_fork(thread),
        2 => sys_exit(thread, syscall_args[0] as isize),
        _ => Err(Error::NoSys),
    };

    match res {
        Ok(ret) => thread.inner.write().context.set_syscall_ret(ret),
        Err(_) => todo!(),
    }
}
