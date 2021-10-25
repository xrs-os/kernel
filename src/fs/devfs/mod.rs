use core::future::{ready, Ready};

use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use futures_util::future::BoxFuture;

use crate::time::Timespec;

use super::{mount_fs::NotDynInode, vfs, DirEntryName};

pub mod termios;
pub mod tty;

const DEV_ROOT_INODE_ID: vfs::InodeId = 1;

/// Device filesystem
pub struct DevFs {
    inodes: BTreeMap<vfs::InodeId, Arc<dyn DevInode>>,
}

impl DevFs {
    pub fn new(dev_inodes: impl IntoIterator<Item = Arc<dyn DevInode>>) -> Arc<Self> {
        let inodes = dev_inodes
            .into_iter()
            .enumerate()
            .map(|(inode_id, dev)| (inode_id + DEV_ROOT_INODE_ID, dev))
            .collect::<BTreeMap<_, _>>();

        Arc::new(Self { inodes })
    }
}

impl vfs::Filesystem for Arc<DevFs> {
    type Inode = Arc<dyn DevInode>;

    type CreateInodeFut<'a> = Ready<vfs::Result<Self::Inode>>;

    type LoadInodeFut<'a> = Ready<vfs::Result<Option<Self::Inode>>>;

    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        vfs::RawDirEntry {
            inode_id: DEV_ROOT_INODE_ID,
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
        _mode: vfs::Mode,
        _uid: u32,
        _gid: u32,
        _create_time: Timespec,
    ) -> Self::CreateInodeFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn load_inode(&self, inode_id: vfs::InodeId) -> Self::LoadInodeFut<'_> {
        ready(Ok(self.inodes.get(&inode_id).map(Clone::clone)))
    }

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32 {
        0
    }

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize {
        0
    }
}

/// Device Inode trait
pub trait DevInode: Send + Sync {
    fn id(&self) -> vfs::InodeId;
    fn metadata(&self) -> BoxFuture<vfs::Result<vfs::Metadata>>;
    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> BoxFuture<'a, vfs::Result<usize>>;
    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> BoxFuture<'a, vfs::Result<usize>>;
    fn sync(&self) -> BoxFuture<vfs::Result<()>>;
    fn lookup_raw<'a>(
        &'a self,
        name: &'a super::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;
    fn ls_raw(&self) -> BoxFuture<vfs::Result<Vec<vfs::RawDirEntry>>>;
    fn ioctl(&self, cmd: u32, arg: usize) -> BoxFuture<vfs::Result<()>>;
}

impl NotDynInode for Arc<dyn DevInode> {}

impl vfs::Inode for Arc<dyn DevInode> {
    type FS = Arc<DevFs>;

    type MetadataFut<'a> = BoxFuture<'a, vfs::Result<vfs::Metadata>>;
    type ChownFut<'a> = Ready<vfs::Result<()>>;
    type ChmodFut<'a> = Ready<vfs::Result<()>>;
    type LinkFut<'a> = Ready<vfs::Result<()>>;
    type UnlinkFut<'a> = Ready<vfs::Result<()>>;
    type ReadAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;
    type WriteAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;
    type SyncFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type AppendDotFut<'a> = Ready<vfs::Result<()>>;
    type LookupRawFut<'a> = Ready<vfs::Result<Option<vfs::RawDirEntry>>>;
    type LookupFut<'a> = Ready<vfs::Result<Option<vfs::DirEntry<Self::FS>>>>;
    type AppendFut<'a> = Ready<vfs::Result<()>>;
    type RemoveFut<'a> = Ready<vfs::Result<Option<vfs::RawDirEntry>>>;
    type LsRawFut<'a> = Ready<vfs::Result<Vec<vfs::RawDirEntry>>>;
    type LsFut<'a> = Ready<vfs::Result<Vec<vfs::DirEntry<Self::FS>>>>;
    type IOCtlFut<'a> = BoxFuture<'a, vfs::Result<()>>;

    fn id(&self) -> vfs::InodeId {
        DevInode::id(&**self)
    }

    fn metadata(&self) -> Self::MetadataFut<'_> {
        DevInode::metadata(&**self)
    }

    fn chown(&self, _uid: u32, _gid: u32) -> Self::ChownFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn chmod(&self, _mode: vfs::Mode) -> Self::ChmodFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn link(&self) -> Self::LinkFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn unlink(&self) -> Self::UnlinkFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        DevInode::read_at(&**self, offset, buf)
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        DevInode::write_at(&**self, offset, src)
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        DevInode::sync(&**self)
    }

    fn append_dot(&self, _parent_inode_id: vfs::InodeId) -> Self::AppendDotFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn lookup_raw<'a>(&'a self, _name: &'a super::FsStr) -> Self::LookupRawFut<'a> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn lookup<'a>(&'a self, _name: &'a super::FsStr) -> Self::LookupFut<'a> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn append(
        &self,
        _dir_entry_name: super::DirEntryName,
        _inode_id: vfs::InodeId,
        _file_type: Option<vfs::FileType>,
    ) -> Self::AppendFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn remove<'a>(&'a self, _dir_entry_name: &'a super::FsStr) -> Self::RemoveFut<'a> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn ls_raw(&self) -> Self::LsRawFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn ls(&self) -> Self::LsFut<'_> {
        ready(Err(vfs::Error::Unsupport))
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> Self::IOCtlFut<'_> {
        DevInode::ioctl(&**self, cmd, arg)
    }
}

pub struct DevRootInode {
    dir_entrys: BTreeMap<DirEntryName, vfs::RawDirEntry>,
}

impl DevInode for DevRootInode {
    fn id(&self) -> vfs::InodeId {
        DEV_ROOT_INODE_ID
    }

    fn metadata(&self) -> BoxFuture<vfs::Result<vfs::Metadata>> {
        Box::pin(ready(Ok(vfs::Metadata {
            mode: vfs::Mode::TY_DIR
                | vfs::Mode::PERM_RWX_USR
                | vfs::Mode::PERM_RX_GRP
                | vfs::Mode::PERM_RX_OTH,
            links_count: 1,
            ..Default::default()
        })))
    }

    fn read_at<'a>(
        &'a self,
        _offset: u64,
        _buf: &'a mut [u8],
    ) -> BoxFuture<'a, vfs::Result<usize>> {
        Box::pin(ready(Err(vfs::Error::Unsupport)))
    }

    fn write_at<'a>(&'a self, _offset: u64, _src: &'a [u8]) -> BoxFuture<'a, vfs::Result<usize>> {
        Box::pin(ready(Err(vfs::Error::Unsupport)))
    }

    fn sync(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(ready(Ok(())))
    }

    fn lookup_raw<'a>(
        &'a self,
        name: &'a super::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>> {
        Box::pin(ready(Ok(self.dir_entrys.get(name).map(Clone::clone))))
    }

    fn ls_raw(&self) -> BoxFuture<vfs::Result<Vec<vfs::RawDirEntry>>> {
        Box::pin(ready(Ok(self
            .dir_entrys
            .iter()
            .map(|(_, x)| x.clone())
            .collect())))
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> BoxFuture<vfs::Result<()>> {
        Box::pin(ready(Err(vfs::Error::Unsupport)))
    }
}
