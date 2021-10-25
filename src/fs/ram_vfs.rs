use core::{
    cmp, future,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};

use crate::spinlock;

use super::{mount_fs::NotDynInode, vfs, DirEntryName};
use hashbrown::HashMap;

/// A filesystem based on RAM.
pub struct RamFs {
    root_inode_id: usize,
    id_allocator: IdAllocator,
    inodes: spinlock::RwLockIrq<HashMap<usize, Arc<Inode>>>,
}

impl RamFs {
    /// Constructs a new, empty `RamFs`.
    pub fn new() -> Self {
        let root_inode_id = 1;
        Self {
            root_inode_id,
            id_allocator: IdAllocator::new(root_inode_id + 1),
            inodes: spinlock::RwLockIrq::new(Default::default()),
        }
    }
    fn load_inode(&self, inode_id: usize) -> Option<Arc<Inode>> {
        self.inodes.read().get(&inode_id).cloned()
    }

    fn remove_inode(&self, inode_id: usize) -> Option<()> {
        let mut inodes = self.inodes.write();
        inodes.remove(&inode_id).map(|_| ())
    }
}

impl vfs::Filesystem for Arc<RamFs> {
    type Inode = Arc<Inode>;

    type CreateInodeFut<'a> = future::Ready<vfs::Result<Self::Inode>>;

    type LoadInodeFut<'a> = future::Ready<vfs::Result<Option<Self::Inode>>>;

    fn root_dir_entry_raw(&self) -> vfs::RawDirEntry {
        vfs::RawDirEntry {
            inode_id: self.root_inode_id,
            name: Box::new("/".as_bytes().into()),
            file_type: Some(vfs::FileType::Dir),
        }
    }

    fn root_dir_entry(&self) -> vfs::DirEntry<Self> {
        vfs::DirEntry {
            raw: vfs::RawDirEntry {
                inode_id: self.root_inode_id,
                name: Box::new("/".as_bytes().into()),
                file_type: Some(vfs::FileType::Dir),
            },
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
        let inode_id = self.id_allocator.alloc();
        let inode = Arc::new(Inode {
            inode_id,
            inner: spinlock::RwLockIrq::new(InodeInner {
                metadata: vfs::Metadata {
                    mode,
                    uid,
                    gid,
                    size: 0,
                    atime: create_time.clone(),
                    ctime: create_time.clone(),
                    mtime: create_time,
                    links_count: 1,
                    blk_size: self.blk_size(),
                    blk_count: self.blk_count(),
                },
                content: if mode.is_dir() {
                    Content::Dir(Default::default())
                } else {
                    Content::File(Default::default())
                },
            }),
            fs: self.clone(),
        });
        let mut inodes = self.inodes.write();

        inodes.insert(inode_id, inode.clone());
        future::ready(Ok(inode))
    }

    fn load_inode(&self, inode_id: usize) -> Self::LoadInodeFut<'_> {
        future::ready(Ok(RamFs::load_inode(self, inode_id)))
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

struct IdAllocator {
    last_id: AtomicUsize,
}

impl IdAllocator {
    fn new(start: usize) -> Self {
        Self {
            last_id: AtomicUsize::new(start - 1),
        }
    }

    fn alloc(&self) -> usize {
        self.last_id.fetch_add(1, Ordering::Relaxed)
    }

    fn dealloc(&self, _id: usize) -> bool {
        true
    }
}

struct InodeInner {
    metadata: vfs::Metadata,
    content: Content,
}

struct DirEntry {
    pub inode_id: usize,
    pub file_type: vfs::FileType,
}

enum Content {
    Dir(BTreeMap<DirEntryName, DirEntry>),
    File(Vec<u8>),
}

/// RamFs Inode
pub struct Inode {
    inode_id: usize,
    inner: spinlock::RwLockIrq<InodeInner>,
    fs: Arc<RamFs>,
}

impl Inode {
    fn lookup_raw<'a>(&'a self, name: &'a super::FsStr) -> vfs::Result<Option<vfs::RawDirEntry>> {
        let inner = self.inner.read();
        match &inner.content {
            Content::Dir(dentrys) => Ok(dentrys.get(name).map(|dentry| vfs::RawDirEntry {
                inode_id: dentry.inode_id,
                name: Box::new(DirEntryName::from(name)),
                file_type: Some(dentry.file_type.clone()),
            })),
            Content::File(_) => Err(vfs::Error::NotDir),
        }
    }

    fn append(
        &self,
        dir_entry_name: super::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> vfs::Result<()> {
        let mut inner = self.inner.write();
        match &mut inner.content {
            Content::Dir(dentrys) => dentrys
                .try_insert(
                    dir_entry_name,
                    DirEntry {
                        inode_id,
                        file_type: file_type.unwrap(),
                    },
                )
                .map_err(|_| vfs::Error::EntryExist)
                .map(|_| ()),
            Content::File(_) => Err(vfs::Error::NotDir),
        }
    }

    fn unlink(&self) -> vfs::Result<()> {
        let mut inner = self.inner.write();
        if inner.metadata.links_count > 0 {
            inner.metadata.links_count -= 1;
        }

        if inner.metadata.links_count == 0 {
            self.fs.remove_inode(self.inode_id);
        }
        Ok(())
    }
}

impl NotDynInode for Arc<Inode> {}

impl vfs::Inode for Arc<Inode> {
    type FS = Arc<RamFs>;

    type MetadataFut<'a> = future::Ready<vfs::Result<vfs::Metadata>>;
    type ChownFut<'a> = future::Ready<vfs::Result<()>>;
    type ChmodFut<'a> = future::Ready<vfs::Result<()>>;
    type LinkFut<'a> = future::Ready<vfs::Result<()>>;
    type UnlinkFut<'a> = future::Ready<vfs::Result<()>>;
    type ReadAtFut<'a> = future::Ready<vfs::Result<usize>>;
    type WriteAtFut<'a> = future::Ready<vfs::Result<usize>>;
    type SyncFut<'a> = future::Ready<vfs::Result<()>>;
    type AppendDotFut<'a> = future::Ready<vfs::Result<()>>;
    type LookupRawFut<'a> = future::Ready<vfs::Result<Option<vfs::RawDirEntry>>>;
    type LookupFut<'a> = future::Ready<vfs::Result<Option<vfs::DirEntry<Self::FS>>>>;
    type AppendFut<'a> = future::Ready<vfs::Result<()>>;
    type RemoveFut<'a> = future::Ready<vfs::Result<Option<vfs::RawDirEntry>>>;
    type LsRawFut<'a> = future::Ready<vfs::Result<Vec<vfs::RawDirEntry>>>;
    type LsFut<'a> = future::Ready<vfs::Result<Vec<vfs::DirEntry<Self::FS>>>>;
    type IOCtlFut<'a> = future::Ready<vfs::Result<()>>;

    fn id(&self) -> usize {
        self.inode_id
    }

    fn metadata(&self) -> Self::MetadataFut<'_> {
        future::ready(Ok(self.inner.read().metadata.clone()))
    }

    fn chown(&self, uid: u32, gid: u32) -> Self::ChownFut<'_> {
        let mut inner = self.inner.write();
        inner.metadata.uid = uid;
        inner.metadata.gid = gid;
        future::ready(Ok(()))
    }

    fn chmod(&self, mode: vfs::Mode) -> Self::ChmodFut<'_> {
        let mut inner = self.inner.write();
        inner.metadata.mode = mode;
        future::ready(Ok(()))
    }

    fn link(&self) -> Self::LinkFut<'_> {
        let mut inner = self.inner.write();
        if inner.metadata.links_count > 0 {
            inner.metadata.links_count += 1;
        }
        future::ready(Ok(()))
    }

    fn unlink(&self) -> Self::UnlinkFut<'_> {
        future::ready(Inode::unlink(self))
    }

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        let inner = self.inner.read();
        future::ready(match &inner.content {
            Content::Dir(_) => Err(vfs::Error::Unsupport),
            Content::File(data) => {
                let len = data.len();
                let start = cmp::min(len, offset as usize);
                let end = len.min(start + buf.len());
                let src = &data[start..end];
                buf[..src.len()].copy_from_slice(src);
                Ok(src.len())
            }
        })
    }

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a> {
        let mut inner = self.inner.write();
        future::ready(match &mut inner.content {
            Content::Dir(_) => Err(vfs::Error::Unsupport),
            Content::File(data) => {
                let offset = offset as usize;
                if offset + src.len() > data.len() {
                    let out_of_size = offset + src.len();
                    data.resize(data.len() + out_of_size, 0);
                }
                data[offset..offset + src.len()].copy_from_slice(src);
                Ok(src.len())
            }
        })
    }

    fn sync(&self) -> Self::SyncFut<'_> {
        future::ready(Ok(()))
    }

    fn append_dot(&self, parent_inode_id: usize) -> Self::AppendDotFut<'_> {
        future::ready(
            Inode::append(
                self,
                "..".as_bytes().into(),
                parent_inode_id,
                Some(vfs::FileType::Dir),
            )
            .and_then(|_| {
                Inode::append(
                    self,
                    ".".as_bytes().into(),
                    self.inode_id,
                    Some(vfs::FileType::Dir),
                )
            }),
        )
    }

    fn lookup_raw<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupRawFut<'a> {
        future::ready(Inode::lookup_raw(self, name))
    }

    fn lookup<'a>(&'a self, name: &'a super::FsStr) -> Self::LookupFut<'a> {
        future::ready(Inode::lookup_raw(self, name).map(|opt_raw| {
            opt_raw.map(|raw| vfs::DirEntry {
                raw,
                fs: self.fs.clone(),
            })
        }))
    }

    fn append(
        &self,
        dir_entry_name: super::DirEntryName,
        inode_id: usize,
        file_type: Option<vfs::FileType>,
    ) -> Self::AppendFut<'_> {
        future::ready(Inode::append(self, dir_entry_name, inode_id, file_type))
    }

    fn remove<'a>(&'a self, dir_entry_name: &'a super::FsStr) -> Self::RemoveFut<'a> {
        let mut inner = self.inner.write();
        future::ready(match &mut inner.content {
            Content::Dir(dentrys) => {
                if let Some(dentry) = dentrys.remove(dir_entry_name) {
                    Inode::unlink(&RamFs::load_inode(&self.fs, dentry.inode_id).unwrap()).map(
                        |_| {
                            Some(vfs::RawDirEntry {
                                inode_id: dentry.inode_id,
                                name: Box::new(dir_entry_name.into()),
                                file_type: Some(dentry.file_type),
                            })
                        },
                    )
                } else {
                    Ok(None)
                }
            }
            Content::File(_) => Err(vfs::Error::NotDir),
        })
    }

    fn ls_raw(&self) -> Self::LsRawFut<'_> {
        let inner = self.inner.read();
        future::ready(match &inner.content {
            Content::Dir(dentrys) => Ok(dentrys.iter().map(Into::into).collect()),
            Content::File(_) => Err(vfs::Error::NotDir),
        })
    }

    fn ls(&self) -> Self::LsFut<'_> {
        let inner = self.inner.read();

        future::ready(match &inner.content {
            Content::Dir(dentrys) => Ok(dentrys
                .iter()
                .map(|entry| vfs::DirEntry {
                    raw: entry.into(),
                    fs: self.fs.clone(),
                })
                .collect()),
            Content::File(_) => Err(vfs::Error::NotDir),
        })
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Self::IOCtlFut<'_> {
        future::ready(Err(vfs::Error::Unsupport))
    }
}

impl From<(&DirEntryName, &DirEntry)> for vfs::RawDirEntry {
    fn from((name, dentry): (&DirEntryName, &DirEntry)) -> Self {
        Self {
            inode_id: dentry.inode_id,
            name: Box::new(name.clone()),
            file_type: Some(dentry.file_type.clone()),
        }
    }
}
