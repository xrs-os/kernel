use crate::fs::blk::{self, BlkSize, Result};
use crate::mm::PageParamA;
use crate::spinlock::MutexIrq;
use alloc::boxed::Box;

use futures_util::TryFutureExt;
use mm::page::PageParam;
use naive_fs::BoxFuture;
use virtio_drivers::{HandleIntrError, InterruptHandler};

pub struct VirtioPageSize;

impl virtio_drivers::PageSize for VirtioPageSize {
    const PAGE_SIZE_SHIFT: u8 = PageParamA::PAGE_SIZE_SHIFT as u8;
}

pub struct VirtioBlk {
    inner: virtio_drivers::VirtIOBlk<MutexIrq<()>>,
    blk_size: BlkSize,
}

impl VirtioBlk {
    pub fn new(header: &'static mut virtio_drivers::VirtIOHeader) -> virtio_drivers::Result<Self> {
        let inner = virtio_drivers::VirtIOBlk::new::<VirtioPageSize>(header)?;
        Ok(Self {
            blk_size: BlkSize::new(inner.blk_size),
            inner,
        })
    }

    pub fn handle_interrupt(&self) -> core::result::Result<(), HandleIntrError> {
        self.inner.handle_interrupt()
    }
}

impl blk::BlkDevice for VirtioBlk {
    fn read_blk<'a>(&'a self, blk_id: usize, buf: &'a mut [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(self.inner.async_read_block(blk_id, buf).map_err(Into::into))
    }

    fn write_blk<'a>(&'a self, blk_id: usize, buf: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(
            self.inner
                .async_write_block(blk_id, buf)
                .map_err(Into::into),
        )
    }

    fn blk_size(&self) -> BlkSize {
        self.blk_size
    }

    fn blk_count(&self) -> usize {
        self.inner.capacity
    }
}

impl From<virtio_drivers::Error> for blk::Error {
    fn from(virt_err: virtio_drivers::Error) -> Self {
        match virt_err {
            virtio_drivers::Error::BufferTooSmall
            | virtio_drivers::Error::AlreadyUsed
            | virtio_drivers::Error::InvalidParam => blk::Error::InvalidParam,
            virtio_drivers::Error::NotReady => blk::Error::NotReady,
            virtio_drivers::Error::DmaError => blk::Error::DmaErr,
            virtio_drivers::Error::IoError => blk::Error::IoErr,
        }
    }
}
