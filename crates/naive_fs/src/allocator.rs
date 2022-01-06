use bitmap::Bitmap;

use crate::{
    blk_device::{BlkDevice, Disk, FromBytes, ToBytes},
    maybe_dirty::{MaybeDirty, Syncable},
    BlkId, BoxFuture, Result,
};

use alloc::boxed::Box;

pub(crate) struct Allocator {
    bitmap: MaybeDirty<Bitmap>,
    next_id: u16,
    free: u16, // Number of unassigned ids
    capacity: u16,
}

impl Allocator {
    pub(crate) fn new(bitmap: MaybeDirty<Bitmap>, free: u16, capacity: u16) -> Self {
        Self {
            bitmap,
            next_id: 0,
            free,
            capacity,
        }
    }

    #[allow(dead_code)]
    /// Returns true which means `id` has been allocated.
    pub fn contains(&self, id: u16) -> bool {
        self.bitmap.test((id - 1) as u32)
    }

    /// Allocate ids. return None means no ids are available
    pub fn alloc(&mut self) -> Option<u16> {
        if self.free == 0 {
            return None;
        }
        let mut id = if self.next_id >= self.capacity {
            0
        } else {
            self.next_id
        };

        if self.bitmap.test_and_set(id as u32, true) {
            // This id has been allocated
            id = if let Some(newid) = self.bitmap.find_next_zero(id as u32, None) {
                newid
            } else {
                self.bitmap.find_next_zero(0, None)?
            } as u16;
            self.bitmap.test_and_set(id as u32, true);
        }
        self.next_id = id + 1;
        self.free -= 1;
        Some(id + 1)
    }

    /// dealloc id,
    /// returns false which means the id has been dealloc
    /// or has never been allocated
    pub fn dealloc(&mut self, id: u16) -> bool {
        if id == 0 {
            return false;
        }
        let id = id - 1;
        let old = self.bitmap.test_and_set(id as u32, false);
        if old {
            self.free += 1;
            if self.next_id == id + 1 {
                self.next_id -= 1;
            }
        }
        old
    }

    pub fn free(&self) -> u16 {
        self.free
    }

    pub fn bitmap_blk_id(&self) -> BlkId {
        self.bitmap.addr.blk_id
    }
}

impl Syncable for Allocator {
    fn sync<'a, DK>(&'a self, blk_device: &'a BlkDevice<DK>) -> BoxFuture<'a, Result<()>>
    where
        DK: Disk + Sync,
    {
        Box::pin(async move { self.bitmap.sync(blk_device).await })
    }
}

impl Syncable for Bitmap {}

impl FromBytes for Bitmap {
    const BYTES_LEN: usize = 0;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        Some(Bitmap::from_bytes_be(bytes))
    }
}

impl ToBytes for Bitmap {
    fn bytes_len(&self) -> usize {
        (self.capacity() / u8::BITS) as usize
    }

    fn to_bytes(&self, out: &mut [u8]) {
        self.to_bytes_be(out)
    }
}

#[cfg(test)]
mod if_test {

    use crate::Addr;

    use super::{Allocator, Bitmap, MaybeDirty};

    impl Default for Allocator {
        fn default() -> Self {
            Self {
                bitmap: MaybeDirty::new(Addr::new(0, 0), Bitmap::new(0)),
                next_id: 0,
                free: 0,
                capacity: 0,
            }
        }
    }
}
