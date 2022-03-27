use core::{convert::TryInto, iter::once, ops::Range};

use crate::{
    blk_device::{self, Disk, FromBytes, ToBytes},
    consts,
    maybe_dirty::{MaybeDirty, Syncable},
    scoped,
    super_blk::SuperBlk,
    Addr, BlkDevice, BlkId, BlkSize, Error, InodeId, NaiveFs, Result,
};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use byte_struct::*;
use futures_util::{
    future::{BoxFuture, Map},
    FutureExt,
};

use future_ext::{WithArg2, WithArg2Ext};

use sleeplock::RwLock;

/// RawInode
#[derive(ByteStruct, Debug)]
#[byte_struct_le]
pub struct RawInode {
    pub mode: Mode,
    /// user id associated with the file.
    pub uid: u16,
    /// group id
    pub gid: u16,

    pub size: u32,
    /// the number of seconds since january 1st 1970 of the last time this inode was accessed.
    pub atime: u32,
    /// the number of seconds since january 1st 1970, of when the inode was created.
    pub ctime: u32,
    /// the number of seconds since january 1st 1970, of the last time this inode was modified.
    pub mtime: u32,
    /// the number of seconds since january 1st 1970, of when the inode was deleted.
    pub dtime: u32,

    /// how many times this particular inode is linked (referred to).
    /// Most files will have a link count of 1.
    /// Files with hard links pointing to them will have an additional count for each hard link.
    /// Symbolic links do not affect the link count of an inode.
    /// When the link count reaches 0 the inode and all its associated blocks are freed.
    pub links_count: u16,

    /// Direct block that points to the data Block id of this inode.
    pub direct_blks: [BlkId; consts::INODE_DIRECT_BLK_COUNT],
    pub indirect_blk: BlkId,
}

impl<DK: Disk + Sync> Syncable<DK> for RawInode {
    type SyncFut<'a> = impl core::future::Future<Output = Result<()>> + 'a;

    fn sync<'a>(&'a self, _blk_device: &'a BlkDevice<DK>) -> Self::SyncFut<'a> {
        async { Ok(()) }
    }
}

impl FromBytes for RawInode {
    const BYTES_LEN: usize = Self::BYTE_LEN;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        Some(Self::read_bytes(bytes))
    }
}

impl ToBytes for RawInode {
    fn to_bytes(&self, out: &mut [u8]) {
        self.write_bytes(out);
    }

    fn bytes_len(&self) -> usize {
        Self::BYTE_LEN
    }
}

impl Default for RawInode {
    fn default() -> Self {
        Self::new(Mode::TY_REG, 0, 0, [0; consts::INODE_DIRECT_BLK_COUNT], 0)
    }
}

impl RawInode {
    pub fn new(
        mode: Mode,
        uid: u16,
        gid: u16,
        direct_blks: [BlkId; consts::INODE_DIRECT_BLK_COUNT],
        create_unix_timestamp: u32,
    ) -> Self {
        Self {
            mode,
            uid,
            gid,
            size: 0,
            atime: create_unix_timestamp,
            ctime: create_unix_timestamp,
            mtime: create_unix_timestamp,
            dtime: create_unix_timestamp,
            links_count: 1,
            direct_blks,
            indirect_blk: 0,
        }
    }

    pub fn valid(&self) -> bool {
        self.links_count != 0
    }
}

bitflags! {
    #[derive(ByteStruct)]
    #[byte_struct_le]
    pub struct Mode: u16 {
        // File type
        /// Socket File
        const TY_SOCK = 0xC000;
        /// Symbolic Link
        const TY_LNK = 0xA000;
        /// Regular File
        const TY_REG = 0x8000;
        /// Block Device
        const TY_BLK = 0x6000;
        /// Directory File
        const TY_DIR = 0x4000;
        /// Character Device
        const TY_CHR = 0x2000;
        /// FIFO
        const TY_FIFO = 0x1000;

        /// This bit is 1. The user id of the file needs to be used to override the user id of the process.
        const S_UID = 0x0800;
        ///  This bit is 1 The group id of the file to be used overrides the group id of the process
        const S_SGID = 0x0400;
        /// Sticky bit
        /// https://zh.wikipedia.org/wiki/%E7%B2%98%E6%BB%9E%E4%BD%8D
        const S_VTX = 0x0200;

        // File access permissions
        /// Readable by the file owner
        const PERM_R_USR = 0x0100;
        /// File owner writable
        const PERM_W_USR = 0x0080;
        /// File owners executable
        const PERM_X_USR = 0x0040;
        /// Readable and writable by the file owner
        const PERM_RW_USR = Self::PERM_R_USR.bits | Self::PERM_W_USR.bits;
        /// Readable and executable by the file owner
        const PERM_RX_USR = Self::PERM_R_USR.bits | Self::PERM_X_USR.bits;
        /// Readable, writable and executable by the file owner
        const PERM_RWX_USR = Self::PERM_RW_USR.bits | Self::PERM_X_USR.bits;

        /// Same group readable
        const PERM_R_GRP = 0x0020;
        /// Same group writable
        const PERM_W_GRP = 0x0010;
        /// Same group executable
        const PERM_X_GRP = 0x0008;
        /// Same group readable and writable
        const PERM_RW_GRP = Self::PERM_R_GRP.bits | Self::PERM_W_GRP.bits;
        /// Same group readable and executable
        const PERM_RX_GRP = Self::PERM_R_GRP.bits | Self::PERM_X_GRP.bits;
        /// Same group readable, writable and executable
        const PERM_RWX_GRP = Self::PERM_RW_GRP.bits | Self::PERM_X_GRP.bits;

        /// Others readable
        const PERM_R_OTH = 0x0004;
        /// Others writable
        const PERM_W_OTH = 0x0002;
        /// Others executable
        const PERM_X_OTH = 0x0001;
        /// Others readable and writable
        const PERM_RW_OTH = Self::PERM_R_OTH.bits | Self::PERM_W_OTH.bits;
        /// Others readable and executable
        const PERM_RX_OTH = Self::PERM_R_OTH.bits | Self::PERM_X_OTH.bits;
        /// Others readable, writable and executable
        const PERM_RWX_OTH = Self::PERM_RW_OTH.bits | Self::PERM_X_OTH.bits;

    }
}

impl Mode {
    pub fn is_dir(&self) -> bool {
        self.contains(Mode::TY_DIR)
    }

    pub fn is_file(&self) -> bool {
        self.contains(Mode::TY_REG)
    }

    pub fn is_symlink(&self) -> bool {
        self.contains(Mode::TY_LNK)
    }
}

pub type InodeLoadFut<'a, MutexType, DK> = Map<
    WithArg2<blk_device::ReadValAtFut<'a, RawInode, DK>, InodeId, &'a Arc<NaiveFs<MutexType, DK>>>,
    fn(
        (Result<RawInode>, InodeId, &'a Arc<NaiveFs<MutexType, DK>>),
    ) -> Result<Option<Inode<MutexType, DK>>>,
>;

pub struct Inode<MutexType, DK> {
    pub inode_id: InodeId,
    pub raw: RwLock<MutexType, MaybeDirty<RawInode>>,
    naive_fs: Arc<NaiveFs<MutexType, DK>>,

    direct_blk_len: u32,
}

impl<MutexType, DK> Inode<MutexType, DK>
where
    MutexType: lock_api::RawMutex,
    DK: Disk + Sync,
{
    pub(crate) fn new(
        inode_id: InodeId,
        raw_inode: MaybeDirty<RawInode>,
        naive_fs: Arc<NaiveFs<MutexType, DK>>,
    ) -> Self {
        Self {
            inode_id,
            direct_blk_len: naive_fs
                .blk_device
                .blk_size
                .mul(consts::INODE_DIRECT_BLK_COUNT as u32),
            raw: RwLock::new(raw_inode),
            naive_fs,
        }
    }

    pub fn load(
        inode_id: InodeId,
        naive_fs: &Arc<NaiveFs<MutexType, DK>>,
    ) -> InodeLoadFut<'_, MutexType, DK> {
        naive_fs
            .blk_device
            .read_val_at::<RawInode>(naive_fs.super_blk.raw_inode_addr(inode_id))
            .with_arg2(inode_id, naive_fs)
            .map(|(res, inode_id, naive_fs)| {
                res.map(|raw| {
                    if raw.valid() {
                        Some(Self::new(
                            inode_id,
                            MaybeDirty::new(naive_fs.super_blk.raw_inode_addr(inode_id), raw),
                            naive_fs.clone(),
                        ))
                    } else {
                        None
                    }
                })
            })
    }

    pub fn naive_fs(&self) -> &Arc<NaiveFs<MutexType, DK>> {
        &self.naive_fs
    }

    pub fn super_blk(&self) -> &SuperBlk<MutexType> {
        self.naive_fs().super_blk()
    }

    pub fn blk_device(&self) -> &BlkDevice<DK> {
        &self.naive_fs().blk_device
    }

    pub async fn mode(&self) -> Mode {
        self.raw.read().await.mode
    }

    #[allow(clippy::type_complexity)]
    pub fn link(
        &self,
    ) -> Map<
        sleeplock::RwLockWriteFuture<MutexType, MaybeDirty<RawInode>>,
        fn(sleeplock::RwLockWriteGuard<MutexType, MaybeDirty<RawInode>>) -> (),
    > {
        self.raw.write().map(|mut raw| {
            if raw.valid() {
                raw.links_count += 1;
            }
        })
    }

    pub async fn unlink(&self) -> Result<()> {
        let mut raw_inode = self.raw.write().await;
        raw_inode.links_count -= 1;

        raw_inode.sync(self.blk_device()).await?;
        if raw_inode.links_count != 0 {
            return Ok(());
        }

        let io_blks = self.io_blks::<false>(0, raw_inode.size).await?;

        self.super_blk()
            .try_dealloc_n_blks(
                io_blks
                    .iter()
                    .map(|blk| blk.addr.blk_id)
                    .chain(once(raw_inode.indirect_blk)),
            )
            .await;

        self.super_blk().dealloc_inode(self.inode_id).await;
        Ok(())
    }

    pub async fn read_at(&self, offset: u32, mut buf: &mut [u8]) -> Result<u32> {
        let inode_size = self.raw.read().await.size;
        if offset >= inode_size {
            return Ok(0);
        }

        let remaining = (inode_size - offset) as usize;
        if remaining < buf.len() {
            buf = &mut buf[..remaining];
        }

        let io_blks = self.io_blks::<false>(offset, buf.len() as u32).await?;

        let blk_device = scoped!(self.blk_device());

        let mut read_offset = 0;
        let mut read_len = 0;
        for blk in io_blks.iter() {
            let next_offset = read_offset + blk.len(blk_device.blk_size);
            read_len += blk_device
                .read_at(
                    blk.addr,
                    &mut buf[read_offset as usize..next_offset as usize],
                )
                .await?;
            read_offset = next_offset;
        }

        Ok(read_len)
    }

    pub async fn read<T: FromBytes>(&self, offset: u32) -> Result<Option<T>> {
        let mut bytes = vec![0; T::BYTES_LEN];
        let read_size = self.read_at(offset, &mut bytes).await?;
        if read_size < T::BYTES_LEN as u32 {
            Ok(None)
        } else {
            Ok(Some(T::from_bytes(&bytes).unwrap()))
        }
    }

    pub async fn write_at(&self, offset: u32, buf: &[u8]) -> Result<u32> {
        let blk_device = scoped!(self.blk_device());

        let io_blks = self.io_blks::<true>(offset, buf.len() as u32).await?;
        let mut write_offset = 0;
        let mut write_len = 0;
        for blk in io_blks.iter() {
            let next_offset = write_offset + blk.len(blk_device.blk_size);
            write_len += blk_device
                .write_at(blk.addr, &buf[write_offset as usize..next_offset as usize])
                .await?;
            write_offset = next_offset;
        }

        let mut raw = self.raw.write().await;
        if offset + write_len > raw.size {
            raw.size = offset + write_len;
        }
        Ok(write_len)
    }

    pub async fn write<T: ToBytes>(&self, offset: u32, val: &T) -> Result<()> {
        let mut buf = vec![0; val.bytes_len()];
        val.to_bytes(&mut buf);
        self.write_at(offset, &buf).await?;
        Ok(())
    }

    async fn io_blks<const OR_ALLOC: bool>(&self, offset: u32, len: u32) -> Result<IoBlks> {
        if offset >= self.direct_blk_len {
            Ok(IoBlks {
                direct_blks: None,
                indirect_blks: Some(self.find_in_indirect_blks::<OR_ALLOC>(offset, len).await?),
            })
        } else if offset + len < self.direct_blk_len {
            Ok(IoBlks {
                direct_blks: Some(self.find_in_direct_blks::<OR_ALLOC>(offset, len).await?),
                indirect_blks: None,
            })
        } else {
            Ok(IoBlks {
                direct_blks: Some(self.find_in_direct_blks::<OR_ALLOC>(offset, len).await?),
                indirect_blks: Some(
                    self.find_in_indirect_blks::<OR_ALLOC>(0, len - (self.direct_blk_len - offset))
                        .await?,
                ),
            })
        }
    }

    async fn find_in_direct_blks<const OR_ALLOC: bool>(
        &self,
        offset: u32,
        len: u32,
    ) -> Result<DirectBlks> {
        let blk_size = self.naive_fs().blk_device.blk_size;
        let nth_blk = blk_size.div_by(offset);
        let first_blk_offset = blk_size.mod_by(offset);
        let n_blks = blk_size.div_round_up_by(first_blk_offset + len);

        let direct_blks = self.raw.read().await.direct_blks;
        let mut direct_blks = if nth_blk + n_blks >= direct_blks.len() as u32 {
            DirectBlks {
                blks: direct_blks,
                blks_slice_range: (nth_blk as usize..direct_blks.len()),
                first_blk_offset,
                last_blk_len: LenOfBlk::End,
            }
        } else {
            DirectBlks {
                blks: direct_blks,
                blks_slice_range: (nth_blk as usize..(nth_blk + n_blks) as usize),
                first_blk_offset,
                last_blk_len: if n_blks == 1 {
                    LenOfBlk::Len(len)
                } else {
                    LenOfBlk::Len(blk_size.mod_by(offset + len))
                },
            }
        };

        if OR_ALLOC {
            let mut alloced = false;

            for blk_id in &mut direct_blks.blks[direct_blks.blks_slice_range.clone()] {
                if *blk_id == 0 {
                    *blk_id = self
                        .naive_fs()
                        .super_blk
                        .alloc_blk()
                        .await
                        .ok_or(Error::NoSpace)?;
                    alloced = true;
                }
            }

            if alloced {
                self.raw.write().await.direct_blks = direct_blks.blks;
            }
        }

        Ok(direct_blks)
    }

    async fn find_in_indirect_blks<const OR_ALLOC: bool>(
        &self,
        offset: u32,
        len: u32,
    ) -> Result<IndirectBlks> {
        let mut indirect_blk = self.raw.read().await.indirect_blk;
        if indirect_blk == 0 {
            if OR_ALLOC {
                indirect_blk = self
                    .naive_fs()
                    .super_blk
                    .alloc_blk()
                    .await
                    .ok_or(Error::NoSpace)?;
                self.raw.write().await.indirect_blk = indirect_blk;
            } else {
                return Ok(IndirectBlks::empty());
            }
        }

        let blk_device = scoped!(self.blk_device());
        let blk_size = blk_device.blk_size;

        let nth_blk = blk_size.div_by(offset);
        let first_blk_offset = blk_size.mod_by(offset);
        let n_blks = blk_size
            .div_round_up_by(first_blk_offset + len)
            .min(self.super_blk().blk_ids_count_pre_blk - nth_blk);

        let mut indirect_blks: Vec<BlkId> = blk_device
            .read_vec(
                Addr::new(indirect_blk, nth_blk * BlkId::BYTES_LEN as u32),
                n_blks,
            )
            .await?;

        indirect_blks.resize(n_blks as usize, 0);

        if OR_ALLOC {
            let mut alloced = false;
            for blk_id in indirect_blks.iter_mut() {
                if *blk_id == 0 {
                    *blk_id = self
                        .naive_fs()
                        .super_blk
                        .alloc_blk()
                        .await
                        .ok_or(Error::NoSpace)?;
                    alloced = true;
                }
            }
            if alloced {
                blk_device
                    .write_slice(
                        Addr::new(indirect_blk, nth_blk * BlkId::BYTES_LEN as u32),
                        &indirect_blks,
                    )
                    .await?;
            }
        }

        Ok(IndirectBlks {
            blks: indirect_blks,
            first_blk_offset,
            last_blk_len: if n_blks == 1 {
                LenOfBlk::Len(len)
            } else {
                LenOfBlk::Len(blk_size.mod_by(offset + len))
            },
        })
    }
}

impl<MutexType, DK> Inode<MutexType, DK>
where
    MutexType: lock_api::RawMutex<GuardMarker = lock_api::GuardSend> + Sync + Send,
    DK: Disk + Sync + Send,
{
    pub fn sync(&self) -> BoxFuture<Result<()>> {
        Box::pin(<Self as Syncable<_>>::sync(
            self,
            scoped!(self.blk_device()),
        ))
    }
}

impl<MutexType, DK> Syncable<DK> for Inode<MutexType, DK>
where
    MutexType: lock_api::RawMutex<GuardMarker = lock_api::GuardSend> + Sync + Send,
    DK: Disk + Sync + Send,
{
    type SyncFut<'a> = impl core::future::Future<Output = Result<()>> + 'a where MutexType: 'a;

    fn sync<'a>(&'a self, blk_device: &'a BlkDevice<DK>) -> Self::SyncFut<'a> {
        // https://users.rust-lang.org/t/why-need-send-when-immutably-borrow-t-in-the-async-block/60934
        let Self { raw, naive_fs, .. } = self;

        async move {
            raw.read().await.sync(blk_device).await?;
            scoped!(&naive_fs.super_blk).sync(blk_device).await?;
            blk_device.sync().await
        }
    }
}

struct IoBlks {
    direct_blks: Option<DirectBlks>,
    indirect_blks: Option<IndirectBlks>,
}

impl IoBlks {
    pub fn iter(&self) -> IoBlksIter<'_, '_> {
        IoBlksIter {
            direct_blks_iter: match self.direct_blks {
                Some(ref direct_blks) => direct_blks.iter(),
                None => BlksRange::empty(),
            },
            indirect_blks_iter: match self.indirect_blks {
                Some(ref indirect_blks) => indirect_blks.iter(),
                None => BlksRange::empty(),
            },
            state: IoBlksState::DirectBlks,
        }
    }
}

struct IoBlksIter<'a, 'b> {
    direct_blks_iter: BlksRange<'a>,
    indirect_blks_iter: BlksRange<'b>,

    state: IoBlksState,
}

enum IoBlksState {
    DirectBlks,
    IndirectBlks,
}

impl Iterator for IoBlksIter<'_, '_> {
    type Item = Blk;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let new_state = match self.state {
                IoBlksState::DirectBlks => match self.direct_blks_iter.next() {
                    Some(blk) => return Some(blk),
                    None => IoBlksState::IndirectBlks,
                },

                IoBlksState::IndirectBlks => {
                    return self.indirect_blks_iter.next();
                }
            };

            self.state = new_state;
        }
    }
}

struct DirectBlks {
    blks: [BlkId; consts::INODE_DIRECT_BLK_COUNT],
    blks_slice_range: Range<usize>,

    first_blk_offset: u32,
    last_blk_len: LenOfBlk,
}

impl DirectBlks {
    pub fn iter(&self) -> BlksRange {
        BlksRange::new(
            &self.blks[self.blks_slice_range.clone()],
            self.first_blk_offset,
            self.last_blk_len,
        )
    }
}

#[derive(Debug)]
struct IndirectBlks {
    blks: Vec<BlkId>,
    first_blk_offset: u32,
    last_blk_len: LenOfBlk,
}

impl IndirectBlks {
    pub fn empty() -> Self {
        Self {
            blks: Vec::new(),
            first_blk_offset: 0,
            last_blk_len: LenOfBlk::End,
        }
    }
    pub fn iter(&self) -> BlksRange {
        BlksRange::new(&self.blks, self.first_blk_offset, self.last_blk_len)
    }
}

#[derive(Debug)]
struct Blk {
    addr: Addr,
    len: LenOfBlk,
}

impl Blk {
    pub fn len(&self, blk_size: BlkSize) -> u32 {
        match self.len {
            LenOfBlk::End => blk_size.size() - self.addr.offset_of_blk,
            LenOfBlk::Len(len) => len,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LenOfBlk {
    End,
    Len(u32),
}

struct BlksRange<'a> {
    blks: &'a [BlkId],

    first_blk_offset: u32,
    last_blk_len: LenOfBlk,

    idx: usize,
}

impl<'a> BlksRange<'a> {
    fn new(blks: &'a [BlkId], first_blk_offset: u32, last_blk_len: LenOfBlk) -> Self {
        Self {
            blks,
            first_blk_offset,
            last_blk_len,
            idx: 0,
        }
    }

    fn empty() -> Self {
        BlksRange::new(&[], 0, LenOfBlk::Len(0))
    }
}

impl Iterator for BlksRange<'_> {
    type Item = Blk;

    fn next(&mut self) -> Option<Self::Item> {
        if self.blks.is_empty() {
            return None;
        }
        let last = self.blks.len() - 1;

        self.blks.get(self.idx).and_then(|&blk_id| {
            if blk_id == 0 {
                return None;
            }
            let idx = self.idx;
            self.idx += 1;

            Some(if idx == 0 {
                Blk {
                    addr: Addr::new(blk_id, self.first_blk_offset),
                    len: if self.blks.len() == 1 {
                        self.last_blk_len
                    } else {
                        LenOfBlk::End
                    },
                }
            } else if idx == last {
                Blk {
                    addr: Addr::new(blk_id, 0),
                    len: self.last_blk_len,
                }
            } else {
                Blk {
                    addr: Addr::new(blk_id, 0),
                    len: LenOfBlk::End,
                }
            })
        })
    }
}

impl FromBytes for BlkId {
    const BYTES_LEN: usize = Self::BYTE_LEN;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        Some(BlkId::from_be_bytes(bytes.try_into().ok()?))
    }
}

impl ToBytes for BlkId {
    fn to_bytes(&self, out: &mut [u8]) {
        out.copy_from_slice(&self.to_be_bytes());
    }

    fn bytes_len(&self) -> usize {
        Self::BYTE_LEN
    }
}

#[cfg(test)]
mod test {
    use alloc::{sync::Arc, vec::Vec};
    use tokio_test::block_on;

    use crate::{
        blk_device::{self, BlkDevice},
        consts,
        inode::{Blk, Inode, LenOfBlk, RawInode},
        ram_disk::RamDisk,
        super_blk::{RawSuperBlk, SuperBlk},
        Addr, BlkId, BlkSize, MaybeDirty, NaiveFs,
    };

    #[test]
    fn test_find_in_direct_blks() {
        let cases = [
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                20,
                65,
                vec![
                    Blk {
                        addr: Addr::new(1, 20),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(2, 0),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(3, 0),
                        len: LenOfBlk::Len(21),
                    },
                ],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                20,
                1,
                vec![Blk {
                    addr: Addr::new(1, 20),
                    len: LenOfBlk::Len(1),
                }],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                0,
                1,
                vec![Blk {
                    addr: Addr::new(1, 0),
                    len: LenOfBlk::Len(1),
                }],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                0,
                33,
                vec![
                    Blk {
                        addr: Addr::new(1, 0),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(2, 0),
                        len: LenOfBlk::Len(1),
                    },
                ],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                65,
                33,
                vec![Blk {
                    addr: Addr::new(3, 1),
                    len: LenOfBlk::End,
                }],
            ),
        ];

        for (direct_blks, blk_size, offset, len, expected) in cases {
            let mut raw_inode = MaybeDirty::new(Addr::new(0, 0), RawInode::default());

            let mut direct_blks_arr = [0; consts::INODE_DIRECT_BLK_COUNT];
            (&mut direct_blks_arr[..direct_blks.len()]).copy_from_slice(&direct_blks);
            raw_inode.direct_blks = direct_blks_arr;

            let inode = Inode::new(1, raw_inode, Arc::new(create_naive_fs(blk_size)));

            let actual: Vec<_> = block_on(inode.find_in_direct_blks::<false>(offset, len))
                .unwrap()
                .iter()
                .collect();
            block_on(inode.raw.write()).set_dirty(false);

            assert_eq!(format!("{:?}", actual), format!("{:?}", expected));
        }
    }

    #[test]
    fn test_find_in_indirect_blks() {
        let cases = [
            (
                vec![1 as BlkId, 2, 3],
                BlkSize::<u32>::new(32),
                20,
                65,
                vec![
                    Blk {
                        addr: Addr::new(1, 20),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(2, 0),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(3, 0),
                        len: LenOfBlk::Len(21),
                    },
                ],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                20,
                1,
                vec![Blk {
                    addr: Addr::new(1, 20),
                    len: LenOfBlk::Len(1),
                }],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                0,
                1,
                vec![Blk {
                    addr: Addr::new(1, 0),
                    len: LenOfBlk::Len(1),
                }],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                0,
                33,
                vec![
                    Blk {
                        addr: Addr::new(1, 0),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(2, 0),
                        len: LenOfBlk::Len(1),
                    },
                ],
            ),
            (
                vec![1, 2, 3],
                BlkSize::<u32>::new(32),
                65,
                33,
                vec![Blk {
                    addr: Addr::new(3, 1),
                    len: LenOfBlk::End,
                }],
            ),
        ];

        for (indirect_blks, blk_size, offset, len, expected) in cases {
            let mut raw_inode = MaybeDirty::new(Addr::new(0, 0), RawInode::default());
            let indirect_blk_id = 99;
            raw_inode.indirect_blk = indirect_blk_id;
            let inode = Inode::new(1, raw_inode, Arc::new(create_naive_fs(blk_size)));

            block_on(
                inode
                    .naive_fs
                    .blk_device
                    .write_slice(Addr::new(indirect_blk_id, 0), &indirect_blks),
            )
            .unwrap();

            let actual: Vec<_> = block_on(inode.find_in_indirect_blks::<false>(offset, len))
                .unwrap()
                .iter()
                .collect();
            block_on(inode.raw.write()).set_dirty(false);
            assert_eq!(format!("{:?}", actual), format!("{:?}", expected));
        }
    }

    #[test]
    fn test_io_blks() {
        let cases = vec![
            (
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3],
                vec![4, 5, 6],
                BlkSize::<u32>::new(8),
                95,
                2,
                vec![
                    Blk {
                        addr: Addr::new(3, 7),
                        len: LenOfBlk::End,
                    },
                    Blk {
                        addr: Addr::new(4, 0),
                        len: LenOfBlk::Len(1),
                    },
                ],
            ),
            (
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3],
                vec![4, 5, 6],
                BlkSize::<u32>::new(8),
                88,
                8,
                vec![Blk {
                    addr: Addr::new(3, 0),
                    len: LenOfBlk::End,
                }],
            ),
        ];

        for (direct_blks, indirect_blks, blk_size, offset, len, expected) in cases {
            let mut raw_inode = MaybeDirty::new(Addr::new(0, 0), RawInode::default());
            let indirect_blk_id = 99;
            raw_inode.indirect_blk = indirect_blk_id;

            raw_inode.direct_blks = direct_blks;

            let inode = Inode::new(1, raw_inode, Arc::new(create_naive_fs(blk_size)));

            block_on(
                inode
                    .naive_fs
                    .blk_device
                    .write_slice(Addr::new(indirect_blk_id, 0), &indirect_blks),
            )
            .unwrap();

            let actual: Vec<_> = block_on(inode.io_blks::<false>(offset, len))
                .unwrap()
                .iter()
                .collect();
            block_on(inode.raw.write()).set_dirty(false);

            assert_eq!(format!("{:?}", actual), format!("{:?}", expected));
        }
    }

    fn create_naive_fs(blk_size: BlkSize) -> NaiveFs<spin::Mutex<()>, RamDisk<spin::RwLock<()>>> {
        create_naive_fs_with_blk_device(BlkDevice::new(RamDisk::new(4096), blk_size, false))
    }

    fn create_naive_fs_with_blk_device<DK: blk_device::Disk>(
        blk_device: BlkDevice<DK>,
    ) -> NaiveFs<spin::Mutex<()>, DK> {
        let rsb = RawSuperBlk {
            blk_size_log2: blk_device.blk_size.blk_size_log2,
            ..Default::default()
        };

        NaiveFs {
            super_blk: SuperBlk::new(rsb, false, 0, Default::default(), Default::default()),
            blk_device,
        }
    }
}
