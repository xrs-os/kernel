#![feature(generic_associated_types)]
#![feature(async_stream)]
#![feature(iter_map_while)]
#![feature(new_uninit)]
#![no_std]

use core::{marker::PhantomData, ops};
#[allow(unused_imports)]
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate bitflags;

mod allocator;
mod blk_device;
mod consts;
pub mod dir;
pub mod inode;
mod maybe_dirty;
#[cfg(test)]
mod ram_disk;
mod super_blk;

use alloc::{boxed::Box, sync::Arc};
use inode::{Inode, InodeLoadFut, RawInode};
use super_blk::{RawSuperBlk, SuperBlk};

pub type Result<T> = core::result::Result<T, Error>;

pub use blk_device::{BlkDevice, Disk, DiskError, DiskResult};
pub use dir::{DirEntryName, RawDirEntry};
pub use futures_util::future::BoxFuture;
pub use maybe_dirty::MaybeDirty;
pub type BlkId = u16;
pub type InodeId = u16;

#[derive(Debug)]
pub enum Error {
    NoSpace,
    NotDir,
    InvalidDirEntryName(Box<dir::DirEntryName>),
    ReadOnly,
    DiskError(blk_device::DiskError),
}

#[derive(Debug, Clone, Copy)]
pub struct Addr {
    pub blk_id: BlkId,
    pub offset_of_blk: u32,
}

impl Addr {
    pub fn zerod() -> Self {
        Self::new(0, 0)
    }

    pub fn new(blk_id: BlkId, offset_of_blk: u32) -> Self {
        Self {
            blk_id,
            offset_of_blk,
        }
    }

    /// Calculating absolute offset
    pub fn abs_offset(&self, blk_size: BlkSize) -> u32 {
        blk_size.mul(self.blk_id as u32) + self.offset_of_blk
    }

    pub fn add_offset(mut self, offset: u32, blk_size: BlkSize) -> Self {
        let offset = self.offset_of_blk + offset;
        self.blk_id += blk_size.div_by(offset) as BlkId;
        self.offset_of_blk = blk_size.mod_by(offset);
        self
    }

    pub fn add(mut self, other: Addr, blk_size: BlkSize) -> Self {
        self.blk_id += other.blk_id;
        self.add_offset(other.offset_of_blk, blk_size)
    }
}

/// The block size type.
#[derive(Debug, Clone, Copy)]
pub struct BlkSize<BS = u32> {
    /// log2(blk_size)
    pub blk_size_log2: u8,
    _maker: PhantomData<BS>,
}

impl<BS> BlkSize<BS>
where
    BS: From<u8> + ops::Shl<u8, Output = BS>,
{
    /// Create BlkSize
    pub fn new(blk_size: u32) -> Self {
        assert!(
            blk_size.is_power_of_two(),
            "block_size = {}, that must be power of 2.",
            blk_size
        );

        Self {
            blk_size_log2: (blk_size - 1).count_ones() as u8,
            _maker: PhantomData,
        }
    }

    /// Create BlkSize with log2(blk_size) value.
    pub fn with_blk_size_log2(blk_size_log2: u8) -> Self {
        Self {
            blk_size_log2,
            _maker: PhantomData,
        }
    }

    /// Returns block size.
    pub fn size(&self) -> BS {
        BS::from(1) << self.blk_size_log2
    }

    /// Performs `dividend` / `blk_size`.
    pub fn div_by<D: ops::Shr<u8, Output = D>>(&self, dividend: D) -> D {
        dividend >> self.blk_size_log2
    }

    ///  Performs `dividend` / `blk_size` and round up to the nearest integer if not evenly divisable.
    pub fn div_round_up_by<D>(&self, dividend: D) -> D
    where
        BS: ops::Sub<Output = BS>,
        D: From<BS> + ops::Add<Output = D> + ops::Shr<u8, Output = D>,
    {
        (dividend + D::from(self.size() - BS::from(1))) >> self.blk_size_log2
    }

    /// Performs `m` * `blk_size`.
    pub fn mul<M: ops::Shl<u8, Output = M>>(&self, m: M) -> M {
        m << self.blk_size_log2
    }

    /// Performs `dividend` % `blk_size`.
    pub fn mod_by<D>(&self, dividend: D) -> D
    where
        BS: ops::Sub<Output = BS>,
        D: From<BS> + ops::BitAnd<Output = D>,
    {
        dividend & D::from(self.size() - BS::from(1))
    }
}

/// https://users.rust-lang.org/t/why-need-send-when-immutably-borrow-t-in-the-async-block/60934/3?u=ty666
#[macro_export]
macro_rules! scoped {
    ( $value:expr $(,)? ) => {{
        let it = $value;
        it
    }};
}

/// Returns root inode id.
pub fn root_inode_id() -> InodeId {
    consts::NAIVE_FS_ROOT_INO
}

pub struct NaiveFs<MutexType, DK> {
    super_blk: SuperBlk<MutexType>,
    blk_device: BlkDevice<DK>,
}

impl<MutexType, DK> NaiveFs<MutexType, DK>
where
    MutexType: lock_api::RawMutex,
    DK: Disk + Sync,
{
    pub async fn open(disk: DK, read_only: bool) -> Result<NaiveFs<MutexType, DK>> {
        let (super_blk, blk_device) = SuperBlk::load(disk, read_only).await?;

        Ok(Self {
            super_blk,
            blk_device,
        })
    }

    pub fn create_blank(
        disk: DK,
        fs_blk_size: BlkSize,
        volume_uuid: [u8; 16],
        volume_name: [u8; 16],
    ) -> Self {
        let blks_count = fs_blk_size.div_by(disk.capacity()) as u16;

        let inodes_count = blks_count;

        let raw_super_blk = RawSuperBlk {
            inodes_count,
            blks_count,
            blk_size_log2: fs_blk_size.blk_size_log2,
            on_error: super_blk::OnError::MountAsRo,
            uuid: volume_uuid,
            volume_name,
            prealloc_blocks: 1,
            prealloc_dir_blocks: 1,
        };

        Self {
            super_blk: SuperBlk::create_blank(raw_super_blk),
            blk_device: BlkDevice::new(disk, fs_blk_size, false),
        }
    }

    pub async fn create_inode<RwLockType: lock_api::RawRwLock>(
        self: &Arc<Self>,
        mode: inode::Mode,
        uid: u16,
        gid: u16,
        create_unix_timestamp: u32,
    ) -> Result<Inode<RwLockType, MutexType, DK>> {
        let inode_id = self.super_blk.alloc_inode().await.ok_or(Error::NoSpace)?;
        self.create_inode_inner::<RwLockType>(inode_id, mode, uid, gid, create_unix_timestamp)
            .await
    }

    #[allow(clippy::needless_lifetimes)]
    pub fn load_inode<'a, RwLockType: lock_api::RawRwLock>(
        self: &'a Arc<Self>,
        inode_id: InodeId,
    ) -> InodeLoadFut<'a, RwLockType, MutexType, DK> {
        Inode::load(inode_id, self)
    }

    pub async fn create_root<RwLockType: lock_api::RawRwLock>(
        self: &Arc<Self>,
        create_unix_timestamp: u32,
    ) -> Result<Inode<RwLockType, MutexType, DK>> {
        let inode = self
            .create_inode_inner::<RwLockType>(
                root_inode_id(),
                inode::Mode::TY_DIR
                    | inode::Mode::PERM_RWX_USR
                    | inode::Mode::PERM_RX_GRP
                    | inode::Mode::PERM_RX_OTH,
                0,
                0,
                create_unix_timestamp,
            )
            .await?;
        inode.append_dot(root_inode_id()).await?;
        Ok(inode)
    }

    async fn create_inode_inner<RwLockType: lock_api::RawRwLock>(
        self: &Arc<Self>,
        inode_id: u16,
        mode: inode::Mode,
        uid: u16,
        gid: u16,
        create_unix_timestamp: u32,
    ) -> Result<Inode<RwLockType, MutexType, DK>> {
        let mut prealloc_blks = if mode.contains(inode::Mode::TY_REG) {
            self.super_blk.raw_super_blk.prealloc_blocks
        } else if mode.contains(inode::Mode::TY_DIR) {
            self.super_blk.raw_super_blk.prealloc_dir_blocks
        } else {
            0
        };
        prealloc_blks = prealloc_blks.max(consts::INODE_DIRECT_BLK_COUNT as u8);
        let mut direct_blks = [0; consts::INODE_DIRECT_BLK_COUNT];
        if prealloc_blks > 0 {
            self.super_blk
                .try_alloc_n_blks(prealloc_blks as u16)
                .await
                .into_iter()
                .enumerate()
                .for_each(|(idx, blk_id)| direct_blks[idx] = blk_id);
        }

        let raw_inode = MaybeDirty::new(
            self.super_blk.raw_inode_addr(inode_id),
            RawInode::new(mode, uid, gid, direct_blks, create_unix_timestamp),
        );
        raw_inode.set_dirty(true);
        Ok(Inode::new(inode_id, raw_inode, self.clone()))
    }

    pub fn super_blk(&self) -> &SuperBlk<MutexType> {
        &self.super_blk
    }
}
