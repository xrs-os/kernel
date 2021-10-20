use core::future::{self, ready};

use alloc::{boxed::Box, vec::Vec};
use lock_api::RwLock;

use crate::blk_device::{Disk, DiskResult};

pub enum Error {
    InvalidParam,
}

/// A disk based on RAM.
pub struct RamDisk<RwLockType> {
    data: RwLock<RwLockType, Vec<u8>>,
    capacity: u32,
}

impl<RwLockType> RamDisk<RwLockType>
where
    RwLockType: lock_api::RawRwLock,
{
    /// Constructs a new, empty `RamDisk`.
    pub fn new(capacity: u32) -> Self {
        let data = vec![0; capacity as usize];
        Self {
            data: RwLock::new(data),
            capacity,
        }
    }

    fn check_offset(&self, offset: u32) -> DiskResult<()> {
        if offset >= self.capacity {
            return Err(Box::new(Error::InvalidParam));
        }
        Ok(())
    }
}

impl<RwLockType> Disk for RamDisk<RwLockType>
where
    RwLockType: lock_api::RawRwLock,
{
    type ReadAtFut<'a> = future::Ready<DiskResult<u32>>;

    type WriteAtFut<'a> = future::Ready<DiskResult<u32>>;

    type SyncFut<'a> = future::Ready<DiskResult<()>>;

    fn read_at<'a>(&'a self, offset: u32, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        ready(self.check_offset(offset).map(|_| {
            let data = self.data.read();
            let end_pos = (offset + buf.len() as u32).min(self.capacity);
            buf.copy_from_slice(&data[offset as usize..end_pos as usize]);
            end_pos - offset
        }))
    }

    fn write_at<'a>(&'a self, offset: u32, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        ready(self.check_offset(offset).map(|_| {
            let mut data = self.data.write();
            let end_pos = (offset + src.len() as u32).min(self.capacity);
            (&mut data[offset as usize..end_pos as usize]).copy_from_slice(src);
            end_pos - offset
        }))
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        ready(Ok(()))
    }

    fn capacity(&self) -> u32 {
        self.capacity as u32
    }
}
