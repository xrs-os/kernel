use core::{
    future::{ready, Future, Ready},
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::{boxed::Box, collections::VecDeque, sync::Arc, vec::Vec};

use crate::{
    fs::{ioctl, vfs},
    proc::{executor, pid::Pid},
    spinlock::{MutexIrq, RwLockIrq},
};
use futures_util::future::BoxFuture;

use super::{
    termios::{Termios, Winsize},
    DevFs,
};

const TTY_INODE_ID: vfs::InodeId = 2;

pub struct TtyInode {
    foreground_pgid: RwLockIrq<Option<Pid>>,
    buf: MutexIrq<VecDeque<u8>>,
    wakers: MutexIrq<VecDeque<Waker>>,
    termios: RwLockIrq<Termios>,
    winsize: RwLockIrq<Winsize>,
}

impl TtyInode {
    pub fn new() -> Self {
        Self {
            foreground_pgid: RwLockIrq::new(None),
            buf: MutexIrq::new(VecDeque::new()),
            wakers: MutexIrq::new(VecDeque::new()),
            termios: RwLockIrq::new(Default::default()),
            winsize: RwLockIrq::new(Default::default()),
        }
    }

    pub fn push(&self, c: u8) {
        self.buf.lock().push_back(c);
        let mut wakers = self.wakers.lock();
        while let Some(w) = wakers.pop_front() {
            w.wake()
        }
    }

    pub fn pop(&self) -> Option<u8> {
        self.buf.lock().pop_front()
    }
}

impl super::DevInode for TtyInode {
    fn id(&self) -> vfs::InodeId {
        TTY_INODE_ID
    }

    fn metadata(&self) -> BoxFuture<'_, vfs::Result<vfs::Metadata>> {
        Box::pin(ready(Ok(vfs::Metadata {
            mode: vfs::Mode::TY_CHR
                | vfs::Mode::PERM_RX_GRP
                | vfs::Mode::PERM_RW_USR
                | vfs::Mode::PERM_RW_GRP
                | vfs::Mode::PERM_RW_OTH,
            links_count: 1,
            ..Default::default()
        })))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> BoxFuture<'a, vfs::Result<usize>> {
        Box::pin(ReadAtFut {
            tty_inode: self,
            buf,
        })
    }

    fn write_at<'a>(&'a self, _offset: u64, src: &'a [u8]) -> BoxFuture<'a, vfs::Result<usize>> {
        let s = unsafe { core::str::from_utf8_unchecked(src) };
        crate::print!("{}", s);
        Box::pin(ready(Ok(src.len())))
    }

    fn sync(&self) -> BoxFuture<'_, vfs::Result<()>> {
        Box::pin(ready(Ok(())))
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> BoxFuture<'_, vfs::Result<()>> {
        Box::pin(ready(match cmd {
            ioctl::CMD_TCGETS => {
                let termios = arg as *mut Termios;
                unsafe {
                    *termios = self.termios.read().clone();
                }
                Ok(())
            }
            // TODO: handle these differently
            ioctl::CMD_TCSETS | ioctl::CMD_TCSETSW | ioctl::CMD_TCSETSF => {
                let termois = arg as *const Termios;
                unsafe {
                    *self.termios.write() = (&*termois).clone();
                }
                Ok(())
            }
            ioctl::CMD_TIOCGWINSZ => {
                let winsize = arg as *mut Winsize;
                unsafe {
                    *winsize = self.winsize.read().clone();
                }
                Ok(())
            }
            ioctl::CMD_TIOCGPGRP => {
                let argp = arg as *mut i32;
                let fpgid = self
                    .foreground_pgid
                    .read()
                    .as_ref()
                    .map(|pgid| *pgid.id())
                    .unwrap_or_default();

                unsafe {
                    *argp = fpgid as i32;
                }
                Ok(())
            }

            ioctl::CMD_TIOCSPGRP => {
                let fpgid = unsafe { *(arg as *const i32) } as u32;
                match executor::thread(&fpgid) {
                    Some(thread) => {
                        *self.foreground_pgid.write() = Some(Pid::new(thread.proc().clone()));
                        Ok(())
                    }
                    None => Err(vfs::Error::NoSuchProcess(fpgid)),
                }
            }

            _ => Err(vfs::Error::Unsupport),
        }))
    }
}

pub struct ReadAtFut<'a> {
    tty_inode: &'a TtyInode,
    buf: &'a mut [u8],
}

impl Future for ReadAtFut<'_> {
    type Output = vfs::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(c) = self.tty_inode.pop() {
            return if !self.buf.is_empty() {
                self.buf[0] = c;
                Poll::Ready(Ok(1))
            } else {
                Poll::Ready(Ok(0))
            };
        }
        self.tty_inode.wakers.lock().push_back(cx.waker().clone());
        Poll::Pending
    }
}
