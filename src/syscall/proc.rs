use alloc::sync::Arc;

use crate::proc::{
    self,
    executor::spawn,
    thread::{thread_future, Thread},
};

use super::{Error, Result};

pub fn sys_fork(thread: &Arc<Thread>) -> Result {
    match thread.fork(thread.inner.read().fork()) {
        Ok(new_thread) => {
            let new_thread_id = *new_thread.id() as usize;
            spawn(thread_future(Arc::new(new_thread))).ok_or(Error::EAGAIN)?;
            Ok(new_thread_id)
        }
        Err(e) => {
            log::error!("sys_fork: {:?}", e);
            Err(e.into())
        }
    }
}

pub fn sys_exit(thread: &Arc<Thread>, status: isize) -> Result {
    thread.exit(status);
    Ok(0)
}

impl From<proc::Error> for Error {
    fn from(proc_err: proc::Error) -> Self {
        match proc_err {
            proc::Error::ThreadIdNotEnough => Error::UNKNOWM,
            proc::Error::MemoryErr(mem_err) => match mem_err {
                mm::Error::NoSpace => Error::ENOMEM,
                _ => Error::UNKNOWM,
            },
            proc::Error::ElfErr(_e) => Error::ENOEXEC,
        }
    }
}
