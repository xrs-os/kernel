use alloc::{boxed::Box, sync::Arc, vec::Vec};

use crate::{spinlock::MutexIrq, time::Timespec};

use super::{mount_fs::BoxFuture, vfs};

pub type Filesystem<InnerFs> = Arc<CacheFs<InnerFs>>;

pub struct CacheFs<InnerFs: vfs::Filesystem> {
    inner: InnerFs,
    inodes_cache: MutexIrq<lru::LruCache<usize, Arc<CInode<InnerFs>>>>,
}

impl<InnerFs: vfs::Filesystem + 'static> vfs::Filesystem for Filesystem<InnerFs> {
    type Inode = Arc<CInode<InnerFs>>;

    type CreateInodeFut<'a> = BoxFuture<'a, vfs::Result<Self::Inode>>;
    type LoadInodeFut<'a> = BoxFuture<'a, vfs::Result<Option<Self::Inode>>>;

    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        self.inner.root_dir_entry_raw()
    }

    fn root_dir_entry(&self) -> vfs::DirEntry<Filesystem<InnerFs>> {
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
        create_time: Timespec,
    ) -> Self::CreateInodeFut<'_> {
        Box::pin(async move {
            let new_inode = Arc::new(CInode {
                cache_fs: self.clone(),
                inner: self.inner.create_inode(mode, uid, gid, create_time).await?,
            });
            self.inodes_cache
                .lock()
                .put(vfs::Inode::id(&new_inode), new_inode.clone());
            Ok(new_inode)
        })
    }

    fn load_inode(&self, inode_id: usize) -> Self::LoadInodeFut<'_> {
        Box::pin(async move {
            if let Some(inode) = self.inodes_cache.lock().get(&inode_id) {
                return Ok(Some(inode.clone()));
            }
            // TODO: If the inode_id is not in the LRU cache, the same inode_id may be loaded repeatedly
            Ok(self.inner.load_inode(inode_id).await?.map(|inode| {
                let inode = Arc::new(CInode {
                    cache_fs: self.clone(),
                    inner: inode,
                });
                self.inodes_cache.lock().put(inode_id, inode.clone());
                inode
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

pub struct CInode<InnerFs: vfs::Filesystem> {
    cache_fs: Arc<CacheFs<InnerFs>>,
    inner: InnerFs::Inode,
}

impl<InnerFs: vfs::Filesystem + 'static> vfs::Inode for Arc<CInode<InnerFs>> {
    type FS = Filesystem<InnerFs>;

    type MetadataFut<'a> = <InnerFs::Inode as vfs::Inode>::MetadataFut<'a>;
    type ChownFut<'a> = <InnerFs::Inode as vfs::Inode>::ChownFut<'a>;
    type ChmodFut<'a> = <InnerFs::Inode as vfs::Inode>::ChmodFut<'a>;
    type LinkFut<'a> = <InnerFs::Inode as vfs::Inode>::LinkFut<'a>;
    type UnlinkFut<'a> = <InnerFs::Inode as vfs::Inode>::UnlinkFut<'a>;
    type ReadAtFut<'a> = <InnerFs::Inode as vfs::Inode>::ReadAtFut<'a>;
    type WriteAtFut<'a> = <InnerFs::Inode as vfs::Inode>::WriteAtFut<'a>;
    type SyncFut<'a> = <InnerFs::Inode as vfs::Inode>::SyncFut<'a>;
    type AppendDotFut<'a> = <InnerFs::Inode as vfs::Inode>::AppendDotFut<'a>;
    type LookupRawFut<'a> = <InnerFs::Inode as vfs::Inode>::LookupRawFut<'a>;
    type LookupFut<'a> = BoxFuture<'a, vfs::Result<Option<vfs::DirEntry<Self::FS>>>>;
    type AppendFut<'a> = <InnerFs::Inode as vfs::Inode>::AppendFut<'a>;
    type RemoveFut<'a> = <InnerFs::Inode as vfs::Inode>::RemoveFut<'a>;
    type LsRawFut<'a> = <InnerFs::Inode as vfs::Inode>::LsRawFut<'a>;
    type LsFut<'a> = BoxFuture<'a, vfs::Result<Vec<vfs::DirEntry<Self::FS>>>>;

    fn id(&self) -> usize {
        self.inner.id()
    }

    fn metadata(&self) -> Self::MetadataFut<'_> {
        self.inner.metadata()
    }

    fn chown(&self, uid: u32, gid: u32) -> Self::ChownFut<'_> {
        self.inner.chown(uid, gid)
    }

    fn chmod(&self, mode: vfs::Mode) -> Self::ChmodFut<'_> {
        self.inner.chmod(mode)
    }

    fn link(&self) -> Self::LinkFut<'_> {
        self.inner.link()
    }

    fn unlink(&self) -> Self::UnlinkFut<'_> {
        self.inner.unlink()
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        self.inner.read_at(offset, buf)
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        self.inner.write_at(offset, src)
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        self.inner.sync()
    }

    fn append_dot(&self, parent_inode_id: usize) -> Self::AppendDotFut<'_> {
        self.inner.append_dot(parent_inode_id)
    }

    fn lookup_raw<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupRawFut<'a> {
        self.inner.lookup_raw(name)
    }

    fn lookup<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupFut<'a> {
        Box::pin(async move {
            Ok(self
                .inner
                .lookup_raw(name)
                .await?
                .map(|raw_dir_entry| vfs::DirEntry {
                    raw: raw_dir_entry,
                    fs: self.cache_fs.clone(),
                }))
        })
    }

    fn append(
        &self,
        dir_entry_name: super::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> Self::AppendFut<'_> {
        self.inner.append(dir_entry_name, inode_id, file_type)
    }

    fn remove<'a>(&'a self, dir_entry_name: &'a super::FsStr) -> Self::RemoveFut<'a> {
        self.inner.remove(dir_entry_name)
    }

    fn ls_raw(&self) -> Self::LsRawFut<'_> {
        self.inner.ls_raw()
    }

    fn ls(&self) -> Self::LsFut<'_> {
        Box::pin(async move {
            Ok(self
                .inner
                .ls_raw()
                .await?
                .into_iter()
                .map(|raw_dir_entry| vfs::DirEntry {
                    raw: raw_dir_entry,
                    fs: self.cache_fs.clone(),
                })
                .collect())
        })
    }
}
