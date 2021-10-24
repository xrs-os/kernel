use alloc::{boxed::Box, sync::Arc, vec::Vec};
use future_ext::{WithArg1, WithArg1Ext, WithArg2, WithArg2Ext};
use futures_util::{
    future::{Map, MapErr},
    FutureExt, TryFutureExt,
};
use naive_fs::BoxFuture;

use crate::spinlock::{MutexIrq, RwLockIrq};

use super::{
    blk::{self},
    disk::{self, Disk as FsDisk},
    vfs, DirEntryName,
};

impl naive_fs::Disk for FsDisk {
    type ReadAtFut<'a> =
        Map<disk::ReadAtFut<'a>, fn(blk::Result<usize>) -> naive_fs::DiskResult<u32>>;

    type WriteAtFut<'a> =
        Map<disk::WriteAtFut<'a>, fn(blk::Result<usize>) -> naive_fs::DiskResult<u32>>;

    type SyncFut<'a> = BoxFuture<'a, naive_fs::DiskResult<()>>;

    fn read_at<'a>(&'a self, offset: u32, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        FsDisk::read_at(self, offset as u64, buf).map(|res| match res {
            Ok(len) => Ok(len as u32),
            Err(e) => Err(e.into()),
        })
    }

    fn write_at<'a>(&'a self, offset: u32, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        FsDisk::write_at(self, offset as u64, src).map(|res| match res {
            Ok(len) => Ok(len as u32),
            Err(e) => Err(e.into()),
        })
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        Box::pin(FsDisk::sync(self).map_err(Into::into))
    }

    fn capacity(&self) -> u32 {
        FsDisk::capacity(self) as u32
    }
}

type NaiveFs<DK> = Arc<naive_fs::NaiveFs<MutexIrq<()>, DK>>;
type NaiveFsInode<DK> = naive_fs::inode::Inode<RwLockIrq<()>, MutexIrq<()>, DK>;

impl<DK> vfs::Filesystem for NaiveFs<DK>
where
    DK: naive_fs::Disk + Send + Sync + 'static,
{
    type Inode = NaiveFsInode<DK>;

    type CreateInodeFut<'a> = BoxFuture<'a, vfs::Result<Self::Inode>>;

    type LoadInodeFut<'a> = MapErr<
        naive_fs::inode::InodeLoadFut<'a, RwLockIrq<()>, MutexIrq<()>, DK>,
        fn(naive_fs::Error) -> vfs::Error,
    >;

    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        vfs::RawDirEntry {
            inode_id: naive_fs::root_inode_id() as usize,
            name: Box::new("/".as_bytes().into()),
            file_type: Some(vfs::FileType::Dir),
        }
    }

    fn root_dir_entry(&self) -> vfs::DirEntry<Self> {
        vfs::DirEntry {
            raw: self.root_dir_entry_raw(),
            fs: self.clone(),
        }
    }

    fn create_inode(
        &self,
        mode: vfs::Mode,
        uid: u32,
        gid: u32,
        create_time: crate::time::Timespec,
    ) -> Self::CreateInodeFut<'_> {
        Box::pin(
            naive_fs::NaiveFs::create_inode(
                self,
                mode.into(),
                uid as u16,
                gid as u16,
                create_time.unix_timestamp(),
            )
            .map_err(Into::into),
        )
    }

    fn load_inode(&self, inode_id: vfs::InodeId) -> Self::LoadInodeFut<'_> {
        naive_fs::NaiveFs::load_inode(self, inode_id as naive_fs::InodeId).map_err(Into::into)
    }

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32 {
        naive_fs::NaiveFs::blk_size(self)
    }

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize {
        naive_fs::NaiveFs::blk_count(self)
    }
}

#[allow(clippy::type_complexity)]
impl<DK> vfs::Inode for NaiveFsInode<DK>
where
    DK: naive_fs::Disk + Send + Sync + 'static,
{
    type FS = NaiveFs<DK>;

    type MetadataFut<'a> = Map<
        WithArg1<
            sleeplock::RwLockReadFuture<
                'a,
                RwLockIrq<()>,
                naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
            >,
            Self::FS,
        >,
        fn(
            (
                sleeplock::RwLockReadGuard<
                    'a,
                    RwLockIrq<()>,
                    naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
                >,
                Self::FS,
            ),
        ) -> vfs::Result<vfs::Metadata>,
    >;

    type ChownFut<'a> = Map<
        WithArg2<
            sleeplock::RwLockWriteFuture<
                'a,
                RwLockIrq<()>,
                naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
            >,
            u32,
            u32,
        >,
        fn(
            (
                sleeplock::RwLockWriteGuard<
                    'a,
                    RwLockIrq<()>,
                    naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
                >,
                u32,
                u32,
            ),
        ) -> vfs::Result<()>,
    >;

    type ChmodFut<'a> = Map<
        WithArg1<
            sleeplock::RwLockWriteFuture<
                'a,
                RwLockIrq<()>,
                naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
            >,
            vfs::Mode,
        >,
        fn(
            (
                sleeplock::RwLockWriteGuard<
                    'a,
                    RwLockIrq<()>,
                    naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
                >,
                vfs::Mode,
            ),
        ) -> vfs::Result<()>,
    >;

    type LinkFut<'a> = Map<
        Map<
            sleeplock::RwLockWriteFuture<
                'a,
                RwLockIrq<()>,
                naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
            >,
            fn(
                sleeplock::RwLockWriteGuard<
                    RwLockIrq<()>,
                    naive_fs::MaybeDirty<naive_fs::inode::RawInode>,
                >,
            ),
        >,
        fn(()) -> vfs::Result<()>,
    >;

    type UnlinkFut<'a> = BoxFuture<'a, vfs::Result<()>>;

    type ReadAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;

    type WriteAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;

    type SyncFut<'a> =
        MapErr<BoxFuture<'a, naive_fs::Result<()>>, fn(naive_fs::Error) -> vfs::Error>;

    type AppendDotFut<'a> = BoxFuture<'a, vfs::Result<()>>;

    type LookupRawFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;

    type LookupFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Self::FS>>>>;

    type AppendFut<'a> = BoxFuture<'a, vfs::Result<()>>;

    type RemoveFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;

    type LsRawFut<'a> = BoxFuture<'a, vfs::Result<Vec<vfs::RawDirEntry>>>;

    type LsFut<'a> = BoxFuture<'a, vfs::Result<Vec<vfs::DirEntry<Self::FS>>>>;

    fn id(&self) -> vfs::InodeId {
        self.inode_id as vfs::InodeId
    }

    fn metadata(&self) -> Self::MetadataFut<'_> {
        self.raw
            .read()
            .with_arg1(self.naive_fs().clone())
            .map(|(raw, fs)| {
                Ok(vfs::Metadata {
                    mode: raw.mode.into(),
                    uid: raw.uid as u32,
                    gid: raw.gid as u32,
                    size: raw.size as u64,
                    atime: raw.atime.into(),
                    ctime: raw.ctime.into(),
                    mtime: raw.mtime.into(),
                    links_count: raw.links_count,
                    blk_size: fs.blk_size(),
                    blk_count: fs.blk_count(),
                })
            })
    }

    fn chown(&self, uid: u32, gid: u32) -> Self::ChownFut<'_> {
        self.raw
            .write()
            .with_arg2(uid, gid)
            .map(|(mut raw, uid, gid)| {
                raw.uid = uid as u16;
                raw.gid = gid as u16;
                Ok(())
            })
    }

    fn chmod(&self, mode: vfs::Mode) -> Self::ChmodFut<'_> {
        self.raw.write().with_arg1(mode).map(|(mut raw, mode)| {
            raw.mode = mode.into();
            Ok(())
        })
    }

    fn link(&self) -> Self::LinkFut<'_> {
        naive_fs::inode::Inode::link(self).map(|_| Ok(()))
    }

    fn unlink(&self) -> Self::UnlinkFut<'_> {
        Box::pin(naive_fs::inode::Inode::unlink(self).map_err(Into::into))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        Box::pin(
            naive_fs::inode::Inode::read_at(self, offset as u32, buf).map(|res| match res {
                Ok(len) => Ok(len as usize),
                Err(e) => Err(e.into()),
            }),
        )
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        Box::pin(
            naive_fs::inode::Inode::write_at(self, offset as u32, src).map(|res| match res {
                Ok(len) => Ok(len as usize),
                Err(e) => Err(e.into()),
            }),
        )
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        naive_fs::inode::Inode::sync(self).map_err(Into::into)
    }

    fn append_dot(&self, parent_inode_id: vfs::InodeId) -> Self::AppendDotFut<'_> {
        Box::pin(
            naive_fs::inode::Inode::append_dot(self, parent_inode_id as naive_fs::InodeId)
                .map_err(Into::into),
        )
    }

    fn lookup_raw<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupRawFut<'a> {
        Box::pin(async move {
            match naive_fs::inode::Inode::lookup(self, name.as_bytes()).await? {
                Some(raw_dir_entry) => Ok(Some(raw_dir_entry.into())),
                None => Ok(None),
            }
        })
    }

    fn lookup<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupFut<'a> {
        Box::pin(async move {
            match naive_fs::inode::Inode::lookup(self, name.as_bytes()).await? {
                Some(raw_dir_entry) => Ok(Some(vfs::DirEntry {
                    raw: raw_dir_entry.into(),
                    fs: self.naive_fs().clone(),
                })),
                None => Ok(None),
            }
        })
    }

    fn append(
        &self,
        dir_entry_name: DirEntryName,
        inode_id: vfs::InodeId,
        file_type: Option<vfs::FileType>,
    ) -> Self::AppendFut<'_> {
        Box::pin(
            naive_fs::inode::Inode::append(
                self,
                inode_id as naive_fs::InodeId,
                dir_entry_name.into(),
                file_type.unwrap_or(vfs::FileType::RegFile).into(),
            )
            .map_err(Into::into),
        )
    }

    fn remove<'a>(&'a self, dir_entry_name: &'a super::FsStr) -> Self::RemoveFut<'a> {
        Box::pin(
            naive_fs::inode::Inode::remove(self, dir_entry_name.as_bytes())
                .map_ok(|d| d.map(Into::into))
                .map_err(Into::into),
        )
    }

    fn ls_raw(&self) -> Self::LsRawFut<'_> {
        Box::pin(
            naive_fs::inode::Inode::ls(self)
                .map_ok(|raws| raws.into_iter().map(Into::into).collect())
                .map_err(Into::into),
        )
    }

    fn ls(&self) -> Self::LsFut<'_> {
        Box::pin(async move {
            naive_fs::inode::Inode::ls(self)
                .await
                .map(|list| {
                    list.into_iter()
                        .map(|raw_dir_entry| vfs::DirEntry {
                            raw: raw_dir_entry.into(),
                            fs: self.naive_fs().clone(),
                        })
                        .collect()
                })
                .map_err(Into::into)
        })
    }
}

impl From<blk::Error> for naive_fs::DiskError {
    fn from(_disk_err: blk::Error) -> Self {
        // todo
        Box::new(123)
    }
}

impl From<naive_fs::Error> for vfs::Error {
    fn from(naive_fs_err: naive_fs::Error) -> Self {
        match naive_fs_err {
            naive_fs::Error::DiskError(disk_err) => {
                vfs::Error::BlkErr(*disk_err.downcast::<blk::Error>().unwrap())
            }
            naive_fs::Error::NoSpace => vfs::Error::NoSpace,
            naive_fs::Error::InvalidDirEntryName(name) => {
                vfs::Error::InvalidDirEntryName(Box::new((*name).into()))
            }

            naive_fs::Error::ReadOnly => vfs::Error::ReadOnly,
            naive_fs::Error::NotDir => vfs::Error::NotDir,
        }
    }
}

impl From<naive_fs::inode::Mode> for vfs::Mode {
    fn from(naive_fs_mode: naive_fs::inode::Mode) -> Self {
        Self::from_bits(naive_fs_mode.bits()).unwrap()
    }
}

impl From<vfs::Mode> for naive_fs::inode::Mode {
    fn from(vfs_mode: vfs::Mode) -> Self {
        Self::from_bits(vfs_mode.bits()).unwrap()
    }
}

impl From<vfs::FileType> for naive_fs::dir::FileType {
    fn from(vfs_file_type: vfs::FileType) -> Self {
        match vfs_file_type {
            vfs::FileType::RegFile => Self::RegFile,
            vfs::FileType::Dir => Self::Dir,
            vfs::FileType::ChrDev => Self::ChrDev,
            vfs::FileType::BlkDev => Self::BlkDev,
            vfs::FileType::Fifo => Self::Fifo,
            vfs::FileType::Sock => Self::Sock,
            vfs::FileType::Symlink => Self::Symlink,
        }
    }
}

impl From<naive_fs::dir::FileType> for vfs::FileType {
    fn from(naive_file_type: naive_fs::dir::FileType) -> Self {
        match naive_file_type {
            naive_fs::dir::FileType::RegFile => Self::RegFile,
            naive_fs::dir::FileType::Dir => Self::Dir,
            naive_fs::dir::FileType::ChrDev => Self::ChrDev,
            naive_fs::dir::FileType::BlkDev => Self::BlkDev,
            naive_fs::dir::FileType::Fifo => Self::Fifo,
            naive_fs::dir::FileType::Sock => Self::Sock,
            naive_fs::dir::FileType::Symlink => Self::Symlink,
        }
    }
}

impl From<DirEntryName> for naive_fs::DirEntryName {
    fn from(name: DirEntryName) -> Self {
        let (bytes, len) = name.into_inner();
        Self::new(bytes, len)
    }
}

impl From<naive_fs::DirEntryName> for DirEntryName {
    fn from(name: naive_fs::DirEntryName) -> Self {
        let (bytes, len) = name.into_inner();
        Self::new(bytes, len)
    }
}

impl From<naive_fs::RawDirEntry> for vfs::RawDirEntry {
    fn from(naive_raw_dir_entry: naive_fs::RawDirEntry) -> Self {
        let inode_id = naive_raw_dir_entry.inode_id as vfs::InodeId;
        let file_type = Some(naive_raw_dir_entry.file_type.into());
        let name_len = naive_raw_dir_entry.name_len;
        vfs::RawDirEntry {
            inode_id,
            name: Box::new(DirEntryName::new(naive_raw_dir_entry.raw_name(), name_len)),
            file_type,
        }
    }
}
