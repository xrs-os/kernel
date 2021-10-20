use core::future::ready;

use alloc::{boxed::Box, vec::Vec};
use futures_util::future::BoxFuture;

use crate::spinlock::RwLockIrq;

use super::blk;

/// A block device based on RAM.
pub struct RamBlkDevice {
    data: Vec<RwLockIrq<Vec<u8>>>,
    blk_size: blk::BlkSize,
    blk_count: usize,
}

impl RamBlkDevice {
    /// Constructs a new, empty `RamBlkDevice`.
    pub fn new(blk_size: blk::BlkSize, blk_count: usize) -> Self {
        let mut data = Vec::with_capacity(blk_count);
        unsafe { data.set_len(blk_count) };
        data.fill_with(|| RwLockIrq::new(Vec::new()));
        Self {
            data,
            blk_size,
            blk_count,
        }
    }

    fn check_param(&self, blk_id: usize, buf: &[u8]) -> blk::Result<()> {
        if blk_id >= self.blk_count || buf.len() != self.blk_size.size() as usize {
            return Err(blk::Error::InvalidParam);
        }
        Ok(())
    }
}

impl blk::BlkDevice for RamBlkDevice {
    fn read_blk<'a>(&'a self, blk_id: usize, buf: &'a mut [u8]) -> BoxFuture<'a, blk::Result<()>> {
        Box::pin(ready(self.check_param(blk_id, buf).map(|_| {
            let blk_data = unsafe { self.data.get_unchecked(blk_id) }.read();

            if blk_data.is_empty() {
                buf.fill_with(Default::default);
            } else {
                buf.copy_from_slice(&*blk_data);
            }
        })))
    }

    fn write_blk<'a>(&'a self, blk_id: usize, src: &'a [u8]) -> BoxFuture<'a, blk::Result<()>> {
        Box::pin(ready(self.check_param(blk_id, src).map(|_| {
            let mut blk_data = unsafe { self.data.get_unchecked(blk_id) }.write();

            if blk_data.is_empty() {
                blk_data.extend_from_slice(src);
            } else {
                blk_data.copy_from_slice(src);
            }
        })))
    }

    fn blk_size(&self) -> blk::BlkSize {
        self.blk_size
    }

    fn blk_count(&self) -> usize {
        self.blk_count
    }
}
