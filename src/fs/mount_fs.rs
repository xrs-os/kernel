use core::any::Any;

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use futures_util::{future::BoxFuture, TryFutureExt};
use hashbrown::HashMap;

use crate::{fs, spinlock::RwLockIrq, time::Timespec};

use super::vfs;

pub async fn mount(mountpoint: Arc<dyn DynInode>, fs: Arc<dyn DynFilesystem>) -> vfs::Result<()> {
    let minode = mountpoint
        .as_any_ref()
        .downcast_ref::<MInode<Arc<dyn DynFilesystem>>>()
        .ok_or(vfs::Error::Unsupport)?;
    minode.mount(fs);
    Ok(())
}

pub trait DynInode: Send + Sync {
    fn id(&self) -> usize;

    fn metadata(&self) -> BoxFuture<vfs::Result<vfs::Metadata>>;

    fn chown(&self, uid: u32, gid: u32) -> BoxFuture<vfs::Result<()>>;

    fn chmod(&self, mode: vfs::Mode) -> BoxFuture<vfs::Result<()>>;

    fn link(&self) -> BoxFuture<vfs::Result<()>>;

    fn unlink(&self) -> BoxFuture<vfs::Result<()>>;

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> BoxFuture<vfs::Result<usize>>;

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> BoxFuture<vfs::Result<usize>>;

    fn sync(&self) -> BoxFuture<vfs::Result<()>>;

    /// Append ".", ".." into this directory.
    fn append_dot(&self, parent_inode_id: usize) -> BoxFuture<vfs::Result<()>>;

    fn lookup_raw<'a>(
        &'a self,
        name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;

    fn lookup<'a>(
        &'a self,
        name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Arc<dyn DynFilesystem>>>>>;

    fn append(
        &self,
        dir_entry_name: fs::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> BoxFuture<vfs::Result<()>>;

    fn remove<'a>(
        &'a self,
        dir_entry_name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;

    fn ls_raw(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::RawDirEntry>>>;

    fn ls(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::DirEntry<Arc<dyn DynFilesystem>>>>>;

    fn ioctl(&self, cmd: u32, arg: usize) -> BoxFuture<'_, vfs::Result<()>>;

    fn as_any_ref(&self) -> &dyn Any;
}

impl vfs::Inode for Arc<dyn DynInode> {
    type FS = Arc<dyn DynFilesystem>;

    type MetadataFut<'a> = BoxFuture<'a, vfs::Result<vfs::Metadata>>;
    type ChownFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type ChmodFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type LinkFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type UnlinkFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type ReadAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;
    type WriteAtFut<'a> = BoxFuture<'a, vfs::Result<usize>>;
    type SyncFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type AppendDotFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type LookupRawFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;
    type LookupFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Self::FS>>>>;
    type AppendFut<'a> = BoxFuture<'a, vfs::Result<()>>;
    type RemoveFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>>;
    type LsRawFut<'a> = BoxFuture<'a, vfs::Result<Vec<vfs::RawDirEntry>>>;
    type LsFut<'a> = BoxFuture<'a, vfs::Result<Vec<vfs::DirEntry<Self::FS>>>>;
    type IOCtlFut<'a> = BoxFuture<'a, vfs::Result<()>>;

    fn id(&self) -> usize {
        (**self).id()
    }

    fn metadata(&self) -> Self::MetadataFut<'_> {
        (**self).metadata()
    }

    fn chown(&self, uid: u32, gid: u32) -> Self::ChownFut<'_> {
        (**self).chown(uid, gid)
    }

    fn chmod(&self, mode: vfs::Mode) -> Self::ChmodFut<'_> {
        (**self).chmod(mode)
    }

    fn link(&self) -> Self::LinkFut<'_> {
        (**self).link()
    }

    fn unlink(&self) -> Self::UnlinkFut<'_> {
        (**self).unlink()
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        (**self).read_at(offset, buf)
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        (**self).write_at(offset, src)
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        (**self).sync()
    }

    fn append_dot(&self, parent_inode_id: usize) -> Self::AppendDotFut<'_> {
        (**self).append_dot(parent_inode_id)
    }

    fn lookup_raw<'a>(&'a self, name: &'a fs::FsStr) -> Self::LookupRawFut<'a> {
        (**self).lookup_raw(name)
    }

    fn lookup<'a>(&'a self, name: &'a fs::FsStr) -> Self::LookupFut<'a> {
        (**self).lookup(name)
    }

    fn append(
        &self,
        dir_entry_name: fs::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> Self::AppendFut<'_> {
        (**self).append(dir_entry_name, inode_id, file_type)
    }

    fn remove<'a>(&'a self, dir_entry_name: &'a fs::FsStr) -> Self::RemoveFut<'a> {
        (**self).remove(dir_entry_name)
    }

    fn ls_raw(&self) -> Self::LsRawFut<'_> {
        (**self).ls_raw()
    }

    fn ls(&self) -> Self::LsFut<'_> {
        (**self).ls()
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> Self::IOCtlFut<'_> {
        (**self).ioctl(cmd, arg)
    }
}

/// NotDynInode maker trait
pub trait NotDynInode {}

impl<T: vfs::Inode + NotDynInode + 'static> DynInode for T {
    fn id(&self) -> usize {
        vfs::Inode::id(self)
    }

    fn metadata(&self) -> BoxFuture<vfs::Result<vfs::Metadata>> {
        Box::pin(vfs::Inode::metadata(self))
    }

    fn chown(&self, uid: u32, gid: u32) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::chown(self, uid, gid))
    }

    fn chmod(&self, mode: vfs::Mode) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::chmod(self, mode))
    }

    fn link(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::link(self))
    }

    fn unlink(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::unlink(self))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> BoxFuture<vfs::Result<usize>> {
        Box::pin(vfs::Inode::read_at(self, offset, buf))
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> BoxFuture<vfs::Result<usize>> {
        Box::pin(vfs::Inode::write_at(self, offset, src))
    }

    fn sync(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::sync(self))
    }

    fn append_dot(&self, parent_inode_id: usize) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::append_dot(self, parent_inode_id))
    }

    fn lookup_raw<'a>(
        &'a self,
        name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::lookup_raw(self, name))
    }

    fn lookup<'a>(
        &'a self,
        _name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Arc<dyn DynFilesystem>>>>> {
        unreachable!()
    }

    fn append(
        &self,
        dir_entry_name: fs::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::append(
            self,
            dir_entry_name,
            inode_id,
            file_type,
        ))
    }

    fn remove<'a>(
        &'a self,
        dir_entry_name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::remove(self, dir_entry_name))
    }

    fn ls_raw(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::ls_raw(self))
    }

    fn ls(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::DirEntry<Arc<dyn DynFilesystem>>>>> {
        unreachable!()
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> BoxFuture<'_, vfs::Result<()>> {
        Box::pin(vfs::Inode::ioctl(self, cmd, arg))
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

pub trait DynFilesystem: Send + Sync {
    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry;

    fn root_dir_entry(self: Arc<Self>) -> vfs::DirEntry<Arc<dyn DynFilesystem>>;

    fn create_inode(
        self: Arc<Self>,
        mode: vfs::Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> BoxFuture<'static, vfs::Result<Arc<dyn DynInode>>>;

    fn load_inode(
        self: Arc<Self>,
        inode_id: usize,
    ) -> BoxFuture<'static, vfs::Result<Option<Arc<dyn DynInode>>>>;

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32;

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize;
}

impl vfs::Filesystem for Arc<dyn DynFilesystem> {
    type Inode = Arc<dyn DynInode>;

    type CreateInodeFut<'a> = BoxFuture<'a, vfs::Result<Self::Inode>>;

    type LoadInodeFut<'a> = BoxFuture<'a, vfs::Result<Option<Self::Inode>>>;

    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        (**self).root_dir_entry_raw()
    }

    fn root_dir_entry(&self) -> vfs::DirEntry<Self> {
        let raw = (**self).root_dir_entry_raw();
        vfs::DirEntry {
            raw,
            fs: self.clone(),
        }
    }

    fn create_inode(
        &self,
        mode: vfs::Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> Self::CreateInodeFut<'_> {
        DynFilesystem::create_inode(self.clone(), mode, uid, gid, create_time)
    }

    fn load_inode(&self, inode_id: usize) -> Self::LoadInodeFut<'_> {
        DynFilesystem::load_inode(self.clone(), inode_id)
    }

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32 {
        DynFilesystem::blk_size(&**self)
    }

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize {
        DynFilesystem::blk_count(&**self)
    }
}

impl<T: vfs::Filesystem + 'static> DynFilesystem for T
where
    <T as vfs::Filesystem>::Inode: NotDynInode,
{
    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        vfs::Filesystem::root_dir_entry_raw(self)
    }

    fn root_dir_entry(self: Arc<Self>) -> vfs::DirEntry<Arc<dyn DynFilesystem>> {
        let raw = vfs::Filesystem::root_dir_entry_raw(&*self);

        vfs::DirEntry { raw, fs: self }
    }

    fn create_inode(
        self: Arc<Self>,
        mode: vfs::Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> BoxFuture<'static, vfs::Result<Arc<dyn DynInode>>> {
        Box::pin(async move {
            Ok(
                Arc::new(vfs::Filesystem::create_inode(&*self, mode, uid, gid, create_time).await?)
                    as Arc<dyn DynInode>,
            )
        })
    }

    fn load_inode(
        self: Arc<Self>,
        inode_id: usize,
    ) -> BoxFuture<'static, vfs::Result<Option<Arc<dyn DynInode>>>> {
        Box::pin(async move {
            Ok(vfs::Filesystem::load_inode(&*self, inode_id)
                .await?
                .map(|inode| Arc::new(inode) as Arc<dyn DynInode>))
        })
    }

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32 {
        vfs::Filesystem::blk_size(&*self)
    }

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize {
        vfs::Filesystem::blk_count(&*self)
    }
}

pub struct MountFs<FS> {
    inner: FS,
    mountpoints: RwLockIrq<HashMap<vfs::InodeId, Arc<dyn DynFilesystem>>>,
}

impl<FS: vfs::Filesystem> MountFs<FS> {
    pub fn new(inner: FS) -> Self {
        Self {
            inner,
            mountpoints: RwLockIrq::new(HashMap::new()),
        }
    }

    fn get_mountpoint(&self, inode_id: vfs::InodeId) -> Option<Arc<dyn DynFilesystem>> {
        self.mountpoints.read().get(&inode_id).cloned()
    }
}

impl<InnerFs: vfs::Filesystem + 'static> DynFilesystem for MountFs<InnerFs> {
    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        self.inner.root_dir_entry_raw()
    }

    fn root_dir_entry(self: Arc<Self>) -> vfs::DirEntry<Arc<dyn DynFilesystem>> {
        vfs::DirEntry {
            raw: self.inner.root_dir_entry_raw(),
            fs: self.clone(),
        }
    }

    fn create_inode(
        self: Arc<Self>,
        mode: vfs::Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> BoxFuture<'static, vfs::Result<Arc<dyn DynInode>>> {
        Box::pin(async move {
            Ok(Arc::new(MInode {
                mfs: self.clone(),
                inner: self.inner.create_inode(mode, uid, gid, create_time).await?,
            }) as Arc<dyn DynInode>)
        })
    }

    fn load_inode(
        self: Arc<Self>,
        inode_id: usize,
    ) -> BoxFuture<'static, vfs::Result<Option<Arc<dyn DynInode>>>> {
        Box::pin(async move {
            Ok(self.inner.load_inode(inode_id).await?.map(|inner| {
                Arc::new(MInode {
                    mfs: self.clone(),
                    inner,
                }) as Arc<dyn DynInode>
            }))
        })
    }

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32 {
        self.inner.blk_size()
    }

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize {
        self.inner.blk_count()
    }
}

pub struct MInode<InnerFs: vfs::Filesystem> {
    mfs: Arc<MountFs<InnerFs>>,
    inner: InnerFs::Inode,
}

impl<InnerFs: vfs::Filesystem> MInode<InnerFs> {
    pub fn mount(&self, fs: Arc<dyn DynFilesystem>) {
        self.mfs
            .mountpoints
            .write()
            .insert(vfs::Inode::id(&self.inner), fs);
    }
}

impl<InnerFs: vfs::Filesystem + 'static> DynInode for MInode<InnerFs> {
    fn id(&self) -> usize {
        vfs::Inode::id(&self.inner)
    }

    fn metadata(&self) -> BoxFuture<vfs::Result<vfs::Metadata>> {
        Box::pin(vfs::Inode::metadata(&self.inner))
    }

    fn chown(&self, uid: u32, gid: u32) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::chown(&self.inner, uid, gid))
    }

    fn chmod(&self, mode: vfs::Mode) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::chmod(&self.inner, mode))
    }

    fn link(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::link(&self.inner))
    }

    fn unlink(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::unlink(&self.inner))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> BoxFuture<vfs::Result<usize>> {
        Box::pin(vfs::Inode::read_at(&self.inner, offset, buf))
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> BoxFuture<vfs::Result<usize>> {
        Box::pin(vfs::Inode::write_at(&self.inner, offset, src))
    }

    fn sync(&self) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::sync(&self.inner))
    }

    fn append_dot(&self, parent_inode_id: usize) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::append_dot(&self.inner, parent_inode_id))
    }

    fn lookup_raw<'a>(
        &'a self,
        name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::lookup_raw(&self.inner, name))
    }

    fn lookup<'a>(
        &'a self,
        name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Arc<dyn DynFilesystem>>>>> {
        Box::pin(
            vfs::Inode::lookup_raw(&self.inner, name).map_ok(move |raw_dir_entry| {
                raw_dir_entry.map(|raw_dir_entry| {
                    match self.mfs.get_mountpoint(raw_dir_entry.inode_id) {
                        None => vfs::DirEntry {
                            raw: raw_dir_entry,
                            fs: self.mfs.clone() as Arc<dyn DynFilesystem>,
                        },
                        Some(fs) => vfs::DirEntry {
                            raw: raw_dir_entry,
                            fs,
                        },
                    }
                })
            }),
        )
    }

    fn append(
        &self,
        dir_entry_name: fs::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> BoxFuture<vfs::Result<()>> {
        Box::pin(vfs::Inode::append(
            &self.inner,
            dir_entry_name,
            inode_id,
            file_type,
        ))
    }

    fn remove<'a>(
        &'a self,
        dir_entry_name: &'a fs::FsStr,
    ) -> BoxFuture<'a, vfs::Result<Option<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::remove(&self.inner, dir_entry_name))
    }

    fn ls_raw(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::RawDirEntry>>> {
        Box::pin(vfs::Inode::ls_raw(&self.inner))
    }

    fn ls(&self) -> BoxFuture<'_, vfs::Result<Vec<vfs::DirEntry<Arc<dyn DynFilesystem>>>>> {
        Box::pin(
            vfs::Inode::ls_raw(&self.inner).map_ok(move |raw_dir_entrys| {
                raw_dir_entrys
                    .into_iter()
                    .map(
                        |raw_dir_entry| match self.mfs.get_mountpoint(raw_dir_entry.inode_id) {
                            None => vfs::DirEntry {
                                raw: raw_dir_entry,
                                fs: self.mfs.clone() as Arc<dyn DynFilesystem>,
                            },
                            Some(fs) => vfs::DirEntry {
                                raw: raw_dir_entry,
                                fs,
                            },
                        },
                    )
                    .collect()
            }),
        )
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> BoxFuture<'_, vfs::Result<()>> {
        Box::pin(vfs::Inode::ioctl(&self.inner, cmd, arg))
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
