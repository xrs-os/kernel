use core::sync::atomic::{AtomicBool, Ordering};

use futures_util::future;

use crate::blk_device::ToBytes;

use super::{
    blk_device::{BlkDevice, Disk},
    Addr, BoxFuture, Result,
};
use alloc::boxed::Box;

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

    pub async fn sync<'a, DK>(&'a self, blk_device: &'a BlkDevice<DK>) -> Result<()>
    where
        T: Syncable + ToBytes,
        DK: Disk + Sync,
    {
        if self.is_dirty() {
            self.inner.sync(blk_device).await?;
            blk_device.write_value_at(self.addr, &self.inner).await?;
            self.set_dirty(false);
        }

        Ok(())
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

pub trait Syncable {
    fn sync<'a, DK>(&'a self, _blk_device: &'a BlkDevice<DK>) -> BoxFuture<'a, Result<()>>
    where
        DK: Disk + Sync,
    {
        Box::pin(future::ready(Ok(())))
    }
}
