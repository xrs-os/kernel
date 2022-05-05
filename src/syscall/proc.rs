use alloc::{string::String, sync::Arc, vec::Vec};

use crate::{
    fs::{self, rootfs},
    proc::{
        self,
        executor::spawn,
        thread::{thread_future, Thread},
    },
};

use super::{Error, Result};

pub async fn sys_fork(thread: &Arc<Thread>) -> Result {
    match thread.fork(thread.inner.read().fork()).await {
        Ok(new_thread) => {
            let new_thread_id = *new_thread.id() as usize;
            // TODO handle spwan result
            spawn(thread_future(Arc::new(new_thread))).ok_or(Error::EAGAIN)?;
            Ok(new_thread_id)
        }
        Err(e) => {
            println!("error: {:?}", e);
            log::error!("sys_fork: {:?}", e);
            Err(e.into())
        }
    }
}

pub fn sys_exit(thread: &Arc<Thread>, status: isize) -> Result {
    thread.exit(status);
    Ok(0)
}

pub async fn sys_execve(
    thread: &Arc<Thread>,
    path: &fs::Path,
    argv: Vec<String>,
    envp: Vec<String>,
) -> Result {
    // kill all old threads
    thread.proc().exit(0);

    let inode = rootfs::find_inode(path)
        .await
        .map_err::<Error, _>(Into::into)?
        .ok_or(Error::ENOENT)?;
    thread
        .proc()
        .load_user_program(inode, argv, envp)
        .await
        .map_err::<Error, _>(Into::into)?;

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
