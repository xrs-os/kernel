use core::{
    future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{ready, Context, Poll},
};

use crate::blk_device::{ToBytes, WriteValueAtFut};

use super::{
    blk_device::{BlkDevice, Disk},
    Addr, BoxFuture, Result,
};
use alloc::boxed::Box;
use pin_project::pin_project;

pub struct MaybeDirty<T> {
    inner: T,
    pub is_dirty: AtomicBool,
    pub addr: Addr,
}

impl<T> MaybeDirty<T> {
    pub fn new(addr: Addr, inner: T) -> Self {
        Self {
            inner,
            is_dirty: AtomicBool::new(false),
            addr,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty.load(Ordering::Acquire)
    }

    pub fn set_dirty(&self, dirty: bool) {
        self.is_dirty.store(dirty, Ordering::Release);
    }

    pub fn sync<'a, DK>(&'a self, blk_device: &'a BlkDevice<DK>) -> MaybeDirtySyncFut<'a, T, DK>
    where
        T: Syncable<DK> + ToBytes,
        DK: Disk + Sync,
    {
        MaybeDirtySyncFut {
            md: self,
            blk_device,
            state: MaybeDirtySyncFutState::Init,
        }
    }
}

impl<T> core::ops::Deref for MaybeDirty<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> core::ops::DerefMut for MaybeDirty<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.set_dirty(true);
        &mut self.inner
    }
}

impl<T> Drop for MaybeDirty<T> {
    fn drop(&mut self) {
        assert!(
            !self.is_dirty.load(Ordering::Acquire),
            "data dirty when dropping"
        );
    }
}

pub trait Syncable<DK: Disk + Sync> {
    type SyncFut<'a>: future::Future<Output = Result<()>> + Send + 'a
    where
        Self: 'a;

    fn sync<'a>(&'a self, _blk_device: &'a BlkDevice<DK>) -> Self::SyncFut<'a>;
}

#[pin_project]
pub struct MaybeDirtySyncFut<'a, T, DK: Disk> {
    md: &'a MaybeDirty<T>,
    blk_device: &'a BlkDevice<DK>,
    #[pin]
    state: MaybeDirtySyncFutState<'a, DK>,
}

#[pin_project(project = MaybeDirtySyncFutStateProj)]
enum MaybeDirtySyncFutState<'a, DK: Disk + 'a> {
    Init,
    InnerSync(BoxFuture<'a, Result<()>>),
    BDWriteValueAt(#[pin] WriteValueAtFut<'a, DK>),
}

impl<'a, T, DK> future::Future for MaybeDirtySyncFut<'a, T, DK>
where
    T: Syncable<DK> + ToBytes,
    DK: Disk + Sync,
{
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            let new_state = match this.state.as_mut().project() {
                MaybeDirtySyncFutStateProj::Init => {
                    if !this.md.is_dirty() {
                        return Poll::Ready(Ok(()));
                    }
                    MaybeDirtySyncFutState::InnerSync(Box::pin(this.md.inner.sync(this.blk_device)))
                }
                MaybeDirtySyncFutStateProj::InnerSync(fut) => {
                    ready!(fut.as_mut().poll(cx)?);
                    MaybeDirtySyncFutState::BDWriteValueAt(
                        this.blk_device.write_value_at(this.md.addr, &this.md.inner),
                    )
                }
                MaybeDirtySyncFutStateProj::BDWriteValueAt(fut) => {
                    ready!(fut.poll(cx)?);
                    this.md.set_dirty(false);
                    return Poll::Ready(Ok(()));
                }
            };
            this.state.set(new_state);
        }
    }
}
