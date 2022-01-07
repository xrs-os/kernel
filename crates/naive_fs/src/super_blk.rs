use alloc::{boxed::Box, vec::Vec};
use bitmap::Bitmap;
use futures_util::{future::Map, FutureExt};
use sleeplock::{Mutex, MutexGuard};

use crate::{
    allocator::Allocator,
    blk_device::{self, BlkDevice, Disk, FromBytes, ReadBytesFut, ToBytes},
    consts,
    inode::RawInode,
    maybe_dirty::{MaybeDirty, Syncable},
    root_inode_id, scoped, Addr, BlkId, BlkSize, BoxFuture, Error, InodeId, Result,
};
use byte_struct::*;
use future_ext::{WithArg1, WithArg1Ext, WithArg3, WithArg3Ext};

/// RawSuperBlock
#[derive(ByteStruct)]
#[byte_struct_le]
pub struct RawSuperBlk {
    pub inodes_count: u16,
    pub blks_count: u16,
    /// Block size = 1 << blk_size_log2;
    pub blk_size_log2: u8,
    /// when an error is detected,
    /// What the file system driver should do
    pub on_error: u16,
    /// Volume id
    pub uuid: [u8; 16],
    /// volume name (C style string)
    pub volume_name: [u8; 16],
    /// Indicates the number of pre-allocated blocks
    /// that should be attempted when creating a new regular file
    pub prealloc_blocks: u8,
    /// Indicates the number of pre-allocated Blocks
    /// that should be attempted when creating a new directory.
    pub prealloc_dir_blocks: u8,
}

impl FromBytes for RawSuperBlk {
    const BYTES_LEN: usize = Self::BYTE_LEN;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        Some(Self::read_bytes(bytes))
    }
}

impl ToBytes for RawSuperBlk {
    fn to_bytes(&self, out: &mut [u8]) {
        self.write_bytes(out);
    }

    fn bytes_len(&self) -> usize {
        Self::BYTE_LEN
    }
}

#[derive(ByteStruct)]
#[byte_struct_le]
pub struct RawDescriptor {
    pub blk_bitmap: BlkId,
    pub inode_bitmap: BlkId,
    pub inode_table: BlkId,
    /// Total number of free blocks
    pub free_blks_count: u16,
    /// Total number of free inodes
    pub free_inodes_count: u16,
}

impl FromBytes for RawDescriptor {
    const BYTES_LEN: usize = Self::BYTE_LEN;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        Some(Self::read_bytes(bytes))
    }
}

impl ToBytes for RawDescriptor {
    fn to_bytes(&self, out: &mut [u8]) {
        self.write_bytes(out);
    }

    fn bytes_len(&self) -> usize {
        Self::BYTE_LEN
    }
}

#[repr(u16)]
pub enum OnError {
    /// Pretend nothing has happened
    #[allow(dead_code)]
    Continue = 1,
    /// Remount as read-only
    MountAsRo = 2,
    /// Causing Kernel Panic
    #[allow(dead_code)]
    Panic = 3,
}

impl Default for RawSuperBlk {
    fn default() -> Self {
        Self {
            inodes_count: 0,
            blks_count: 0,
            blk_size_log2: 12,
            on_error: OnError::MountAsRo as u16,
            uuid: [0; 16],
            volume_name: [0; 16],
            prealloc_blocks: 1,
            prealloc_dir_blocks: 1,
        }
    }
}

impl Default for RawDescriptor {
    fn default() -> Self {
        Self {
            blk_bitmap: 0,
            inode_bitmap: 0,
            inode_table: 0,
            free_blks_count: 0,
            free_inodes_count: 0,
        }
    }
}

impl RawSuperBlk {
    pub fn blk_size(&self) -> BlkSize {
        BlkSize::with_blk_size_log2(self.blk_size_log2)
    }
}

impl RawDescriptor {
    #[allow(dead_code)]
    pub fn block_full(&self) -> bool {
        self.free_blks_count == 0
    }

    #[allow(dead_code)]
    pub fn inode_full(&self) -> bool {
        self.free_inodes_count == 0
    }
}

impl Syncable for RawSuperBlk {}

impl Syncable for RawDescriptor {}

pub struct SuperBlk<MutexType> {
    pub raw_super_blk: MaybeDirty<RawSuperBlk>,
    pub inode_table: BlkId,

    pub blk_ids_count_pre_blk: u32,
    pub bytes_per_indirect_blk: BlkSize,

    pub(crate) blk_id_allocator: Mutex<MutexType, Allocator>,
    pub(crate) inode_id_allocator: Mutex<MutexType, Allocator>,
}

impl<MutexType: lock_api::RawMutex> SuperBlk<MutexType> {
    pub(crate) fn new(
        raw_super_blk: RawSuperBlk,
        is_dirty: bool,
        inode_table: BlkId,
        blk_id_allocator: Allocator,
        inode_id_allocator: Allocator,
    ) -> Self {
        let raw_super_blk = MaybeDirty::new(Addr::zerod(), raw_super_blk);

        if is_dirty {
            raw_super_blk.set_dirty(true);
        }

        let blk_ids_count_pre_blk = raw_super_blk.blk_size().size() / BlkId::BYTES_LEN as u32;
        let bytes_per_indirect_blk =
            BlkSize::new(raw_super_blk.blk_size().mul(blk_ids_count_pre_blk));

        Self {
            raw_super_blk,

            inode_table,
            blk_ids_count_pre_blk,
            bytes_per_indirect_blk,

            blk_id_allocator: Mutex::new(blk_id_allocator),
            inode_id_allocator: Mutex::new(inode_id_allocator),
        }
    }

    pub async fn load<DK: Disk>(
        disk: DK,
        read_only: bool,
    ) -> Result<(SuperBlk<MutexType>, BlkDevice<DK>)> {
        let raw_super_blk =
            blk_device::read_val_at::<DK, RawSuperBlk>(&disk, consts::SUPER_BLK_OFFSET)
                .await
                .map_err(Error::DiskError)?;

        let raw_descriptor =
            blk_device::read_val_at::<DK, RawDescriptor>(&disk, raw_descriptor_offset())
                .await
                .map_err(Error::DiskError)?;

        let blk_device = BlkDevice::new(disk, raw_super_blk.blk_size(), read_only);

        let blk_id_allocator = load_allocator(
            raw_descriptor.blk_bitmap,
            raw_super_blk.blks_count,
            raw_descriptor.free_blks_count,
            &blk_device,
        )
        .await?;

        let inode_id_allocator = load_allocator(
            raw_descriptor.inode_bitmap,
            raw_super_blk.inodes_count,
            raw_descriptor.free_inodes_count,
            &blk_device,
        )
        .await?;

        Ok((
            Self::new(
                raw_super_blk,
                false,
                raw_descriptor.inode_table,
                blk_id_allocator,
                inode_id_allocator,
            ),
            blk_device,
        ))
    }

    pub fn create_blank(raw_super_blk: RawSuperBlk) -> Self {
        let mut blk_id_allocator = Allocator::new(
            MaybeDirty::new(
                Addr::new(consts::BLK_BITMAP_BLK_ID, 0),
                Bitmap::new(raw_super_blk.blks_count as u32),
            ),
            raw_super_blk.blks_count,
            raw_super_blk.blks_count,
        );

        let inode_table_blk_count = raw_super_blk
            .blk_size()
            .div_round_up_by(raw_super_blk.inodes_count as u32 * RawInode::BYTE_LEN as u32)
            as u16;
        let reserved_blk_ids = consts::INODE_TABLE_BLK_ID + inode_table_blk_count;
        //  Pre allocate the reserved blk ids
        for _ in 1..=reserved_blk_ids {
            blk_id_allocator.alloc();
        }

        let mut inode_id_allocator = Allocator::new(
            MaybeDirty::new(
                Addr::new(consts::INODE_BITMAP_BLK_ID, 0),
                Bitmap::new(raw_super_blk.inodes_count as u32),
            ),
            raw_super_blk.inodes_count,
            raw_super_blk.inodes_count,
        );

        //  Pre allocate the reserved inode ids
        for _ in 1..=root_inode_id() {
            inode_id_allocator.alloc();
        }

        Self::new(
            raw_super_blk,
            true,
            consts::INODE_TABLE_BLK_ID,
            blk_id_allocator,
            inode_id_allocator,
        )
    }

    fn raw_descriptor(
        &self,
        blk_id_allocator: MutexGuard<MutexType, Allocator>,
        inode_id_allocator: MutexGuard<MutexType, Allocator>,
    ) -> MaybeDirty<RawDescriptor> {
        let raw_descriptor = {
            RawDescriptor {
                blk_bitmap: blk_id_allocator.bitmap_blk_id(),
                inode_bitmap: inode_id_allocator.bitmap_blk_id(),
                inode_table: self.inode_table,
                free_blks_count: blk_id_allocator.free(),
                free_inodes_count: inode_id_allocator.free(),
            }
        };

        MaybeDirty::new(Addr::new(0, raw_descriptor_offset()), raw_descriptor)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn alloc_blk(
        &self,
    ) -> Map<
        sleeplock::MutexLockFuture<MutexType, Allocator>,
        fn(MutexGuard<MutexType, Allocator>) -> Option<u16>,
    > {
        self.blk_id_allocator
            .lock()
            .map(|mut blk_id_allocator| blk_id_allocator.alloc())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn try_alloc_n_blks(
        &self,
        n: u16,
    ) -> Map<
        WithArg1<sleeplock::MutexLockFuture<MutexType, Allocator>, u16>,
        fn((MutexGuard<MutexType, Allocator>, u16)) -> Vec<BlkId>,
    > {
        self.blk_id_allocator
            .lock()
            .with_arg1(n)
            .map(|(mut blk_id_allocator, n)| {
                (0..n)
                    .into_iter()
                    .map_while(|_| blk_id_allocator.alloc())
                    .collect()
            })
    }

    #[allow(dead_code)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn dealloc_blk(
        &self,
        blk_id: BlkId,
    ) -> Map<
        WithArg1<sleeplock::MutexLockFuture<MutexType, Allocator>, BlkId>,
        fn((MutexGuard<MutexType, Allocator>, BlkId)) -> bool,
    > {
        self.blk_id_allocator
            .lock()
            .with_arg1(blk_id)
            .map(|(mut allocator, blk_id)| allocator.dealloc(blk_id))
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn try_dealloc_n_blks<I: Iterator<Item = BlkId>>(
        &self,
        blk_ids: I,
    ) -> Map<
        WithArg1<sleeplock::MutexLockFuture<MutexType, Allocator>, I>,
        fn((MutexGuard<MutexType, Allocator>, I)) -> usize,
    > {
        self.blk_id_allocator
            .lock()
            .with_arg1(blk_ids)
            .map(|(mut blk_id_allocator, blk_ids)| {
                blk_ids
                    .filter(|blk_id| blk_id_allocator.dealloc(*blk_id))
                    .count()
            })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn alloc_inode(
        &self,
    ) -> Map<
        sleeplock::MutexLockFuture<MutexType, Allocator>,
        fn(MutexGuard<MutexType, Allocator>) -> Option<InodeId>,
    > {
        self.inode_id_allocator
            .lock()
            .map(|mut allocator| allocator.alloc())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn dealloc_inode(
        &self,
        inode_id: InodeId,
    ) -> Map<
        WithArg1<sleeplock::MutexLockFuture<MutexType, Allocator>, InodeId>,
        fn((MutexGuard<MutexType, Allocator>, InodeId)) -> bool,
    > {
        self.inode_id_allocator
            .lock()
            .with_arg1(inode_id)
            .map(|(mut allocator, inode_id)| allocator.dealloc(inode_id))
    }
}

const fn raw_descriptor_offset() -> u32 {
    consts::SUPER_BLK_OFFSET + RawSuperBlk::BYTES_LEN as u32
}

impl<MutexType> SuperBlk<MutexType> {
    /// Calculates the Addr for a given `offset`
    pub fn position(&self, offset: u32) -> Addr {
        let blk_n = self.raw_super_blk.blk_size().div_by(offset) as BlkId;
        let offset_of_block = self.raw_super_blk.blk_size().mod_by(offset) as u32;
        Addr::new(blk_n, offset_of_block)
    }

    pub fn raw_inode_addr(&self, inode_id: InodeId) -> Addr {
        Addr::new(self.inode_table, 0).add_offset(
            inode_id as u32 * RawInode::BYTE_LEN as u32,
            self.raw_super_blk.blk_size(),
        )
    }
}

impl<MutexType: lock_api::RawMutex<GuardMarker = lock_api::GuardSend> + Sync> Syncable
    for SuperBlk<MutexType>
{
    fn sync<'f, DK>(&'f self, blk_device: &'f BlkDevice<DK>) -> BoxFuture<'f, Result<()>>
    where
        DK: Disk + Sync,
    {
        Box::pin(async move {
            let blk_id_allocator = scoped!(&self.blk_id_allocator).lock().await;
            let inode_id_allocator = scoped!(&self.inode_id_allocator).lock().await;
            let super_blk_is_dirty = self.raw_super_blk.is_dirty();
            scoped!(&self.raw_super_blk).sync(blk_device).await?;

            blk_id_allocator.sync(blk_device).await?;
            inode_id_allocator.sync(blk_device).await?;

            let raw_descriptor = self.raw_descriptor(blk_id_allocator, inode_id_allocator);
            if super_blk_is_dirty {
                raw_descriptor.set_dirty(true);
                raw_descriptor.sync(blk_device).await?;
            }
            Ok(())
        })
    }
}

type LoadAllocatorFut<'a, DK> = Map<
    WithArg3<ReadBytesFut<'a, DK>, Addr, u16, u16>,
    fn((Result<Vec<u8>>, Addr, u16, u16)) -> Result<Allocator>,
>;

fn load_allocator<DK: Disk>(
    bitmap_blk_id: BlkId,
    capacity: u16,
    free: u16,
    blk_device: &BlkDevice<DK>,
) -> LoadAllocatorFut<'_, DK> {
    let addr = Addr::new(bitmap_blk_id, 0);
    blk_device
        .read_bytes(addr, crate::div_round_up!(capacity as u32, u8::BITS))
        .with_arg3(addr, capacity, free)
        .map(|(bitmap_bytes_res, addr, capacity, free)| {
            bitmap_bytes_res.map(|bitmap_bytes| {
                Allocator::new(
                    MaybeDirty::new(addr, Bitmap::from_bytes_be(&bitmap_bytes)),
                    free,
                    capacity,
                )
            })
        })
}
