use core::future::Future;

use super::{DirEntryName, FsStr, Path};
use crate::time::Timespec;
use alloc::{boxed::Box, string::String, vec::Vec};

pub type Result<T> = core::result::Result<T, Error>;

pub type InodeId = usize;

#[derive(Debug)]
pub enum Error {
    NotDir,
    NoRootDir,
    NoSuchFileOrDirectory,
    EntryExist,
    NoSpace,
    BlkErr(super::blk::Error),
    Eof,
    InvalidDirEntryName(Box<DirEntryName>),
    WrongFS,
    ReadOnly,
    UnsupportedFs(String /* filesystem name */),
    InvalidSeekOffset,
    Unsupport,
    NoSuchProcess(u32 /* pid */),
}

pub struct Vfs<FS> {
    inner: FS,
}

impl<FS: Filesystem> Vfs<FS> {
    pub fn new(inner: FS) -> Self {
        Self { inner }
    }

    pub async fn root(&self) -> DirEntry<FS> {
        self.inner.root_dir_entry()
    }

    pub async fn create_parent_dentry(
        &self,
        parent_dir: &DirEntry<FS>,
        filename: &FsStr,
        mode: Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> Result<FS::Inode> {
        let parent_dir = parent_dir
            .as_dir()
            .await?
            .ok_or(Error::NoSuchFileOrDirectory)?;
        self.create(&parent_dir, filename, mode, uid, gid, create_time)
            .await
    }

    pub async fn create(
        &self,
        parent_dir: &FS::Inode,
        filename: &FsStr,
        mode: Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> Result<FS::Inode> {
        if parent_dir.lookup(filename).await?.is_some() {
            return Err(Error::EntryExist);
        }

        let new_inode = self.inner.create_inode(mode, uid, gid, create_time).await?;
        parent_dir
            .append(filename.into(), new_inode.id(), FileType::from_mode(mode))
            .await?;
        if mode.is_dir() {
            new_inode.append_dot(parent_dir.id()).await?;
        }
        parent_dir.sync().await?;
        new_inode.sync().await?;
        Ok(new_inode)
    }

    pub async fn find<'a>(
        &'a self,
        parent_dir: &DirEntry<FS>,
        path: &'a Path,
    ) -> Result<Option<DirEntry<FS>>> {
        let (mut path, basename) = match path.pop() {
            (path, Some(basename)) => (path, basename),
            _ => return Ok(None),
        };

        let mut current_dir = parent_dir
            .as_dir()
            .await?
            .ok_or(Error::NoSuchFileOrDirectory)?;

        while let (rest_path, Some(name)) = path.shift() {
            path = rest_path;
            match current_dir.lookup(name).await? {
                None => return Ok(None),
                Some(entry) => match entry.as_dir().await? {
                    Some(inode) => current_dir = inode,
                    None => return Ok(None),
                },
            }
        }

        current_dir.lookup(basename).await
    }

    pub async fn mv(
        &self,
        src_parent_dir: &DirEntry<FS>,
        src_name: &FsStr,
        target_parent_dir: &DirEntry<FS>,
        target_name: &FsStr,
    ) -> Result<()> {
        let src_dentry = src_parent_dir
            .as_dir()
            .await?
            .ok_or(Error::NoSuchFileOrDirectory)?
            .remove(src_name)
            .await?
            .ok_or(Error::NoSuchFileOrDirectory)?;
        target_parent_dir
            .as_dir()
            .await?
            .ok_or(Error::NoSuchFileOrDirectory)?
            .append(
                target_name.to_dir_entry_name(),
                src_dentry.inode_id,
                src_dentry.file_type,
            )
            .await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct RawDirEntry {
    pub inode_id: InodeId,
    pub name: Box<DirEntryName>,
    pub file_type: Option<FileType>,
}

impl RawDirEntry {
    pub fn name(&self) -> &FsStr {
        (*self.name).as_ref()
    }
}

pub struct DirEntry<FS: ?Sized> {
    pub raw: RawDirEntry,
    pub fs: FS,
}

impl<FS: Filesystem> DirEntry<FS> {
    pub async fn inode(&self) -> Result<Option<FS::Inode>> {
        self.fs.load_inode(self.raw.inode_id).await
    }

    pub async fn as_dir(&self) -> Result<Option<FS::Inode>> {
        match self.raw.file_type {
            Some(FileType::Dir) | None => self.inode().await,
            _ => Err(Error::NotDir),
        }
    }
}

impl<FS: Filesystem + Clone> Clone for DirEntry<FS> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            fs: self.fs.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum FileType {
    /// Regular File
    RegFile = 1,
    /// Directory File
    Dir = 2,
    /// Character Device
    ChrDev = 3,
    /// Block Device
    BlkDev = 4,
    /// Buffer File
    Fifo = 5,
    /// Socket File
    Sock = 6,
    /// Symbolic Link
    Symlink = 7,
}

impl FileType {
    #[allow(dead_code)]
    fn from_mode(mode: Mode) -> Option<Self> {
        Some(if mode.contains(Mode::TY_REG) {
            Self::RegFile
        } else if mode.contains(Mode::TY_DIR) {
            Self::Dir
        } else if mode.contains(Mode::TY_CHR) {
            Self::ChrDev
        } else if mode.contains(Mode::TY_BLK) {
            Self::BlkDev
        } else if mode.contains(Mode::TY_FIFO) {
            Self::Fifo
        } else if mode.contains(Mode::TY_SOCK) {
            Self::Sock
        } else if mode.contains(Mode::TY_LNK) {
            Self::Symlink
        } else {
            return None;
        })
    }
}

bitflags! {
    #[derive(Default)]
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

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub mode: Mode,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    /// the number of seconds since january 1st 1970 of the last time this inode was accessed.
    pub atime: Timespec,
    /// the number of seconds since january 1st 1970, of when the inode was created.
    pub ctime: Timespec,
    /// the number of seconds since january 1st 1970, of the last time this inode was modified.
    pub mtime: Timespec,
    /// how many times this particular inode is linked (referred to).
    pub links_count: u16,
    pub blk_size: u32,
    pub blk_count: usize,
}

#[allow(dead_code)]
impl Metadata {
    fn is_dir(&self) -> bool {
        self.mode.is_dir()
    }

    fn is_file(&self) -> bool {
        self.mode.is_file()
    }

    fn is_symlink(&self) -> bool {
        self.mode.is_symlink()
    }

    fn owner(&self, uid: u32) -> bool {
        self.uid == uid
    }

    fn in_group(&self, gid: u32) -> bool {
        self.gid == gid
    }

    fn permission(&self, uid: u32, gid: u32, p: Permission) -> bool {
        let mode = self.mode.bits;
        let mut perm = (mode & 0o7) as u8;
        if self.owner(uid) {
            perm |= (mode >> 6 & 0o7) as u8;
        }
        if self.in_group(gid) {
            perm |= (mode >> 3 & 0o7) as u8;
        }
        perm & p.bits == p.bits
    }
}

bitflags! {
    pub struct Permission: u8 {
        const READ = 0x4;
        const WRITE = 0x2;
        const EXEC = 0x1;

        const READ_WRITE = Self::READ.bits | Self::WRITE.bits;
    }
}

pub trait Filesystem: Send + Sync {
    type Inode: Inode<FS = Self>;

    type CreateInodeFut<'a>: Future<Output = Result<Self::Inode>> + Send + 'a;
    type LoadInodeFut<'a>: Future<Output = Result<Option<Self::Inode>>> + Send + 'a;

    fn root_dir_entry_raw(&self) -> RawDirEntry;

    fn root_dir_entry(&self) -> DirEntry<Self>;

    fn create_inode(
        &self,
        mode: Mode,
        uid: u32,
        gid: u32,
        create_time: Timespec,
    ) -> Self::CreateInodeFut<'_>;

    fn load_inode(&self, inode_id: InodeId) -> Self::LoadInodeFut<'_>;

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> u32;

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize;
}

pub trait Inode: Send + Sync {
    type FS: Filesystem<Inode = Self>;
    type MetadataFut<'a>: Future<Output = Result<Metadata>> + Send + 'a;
    type ChownFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type ChmodFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type LinkFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type UnlinkFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type ReadAtFut<'a>: Future<Output = Result<usize>> + Send + 'a;
    type WriteAtFut<'a>: Future<Output = Result<usize>> + Send + 'a;
    type SyncFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type AppendDotFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type LookupRawFut<'a>: Future<Output = Result<Option<RawDirEntry>>> + Send + 'a;
    type LookupFut<'a>: Future<Output = Result<Option<DirEntry<Self::FS>>>> + Send + 'a;
    type AppendFut<'a>: Future<Output = Result<()>> + Send + 'a;
    type RemoveFut<'a>: Future<Output = Result<Option<RawDirEntry>>> + Send + 'a;
    type LsRawFut<'a>: Future<Output = Result<Vec<RawDirEntry>>> + Send + 'a;
    type LsFut<'a>: Future<Output = Result<Vec<DirEntry<Self::FS>>>> + Send + 'a;
    type IOCtlFut<'a>: Future<Output = Result<()>> + Send + 'a;

    fn id(&self) -> InodeId;

    fn metadata(&self) -> Self::MetadataFut<'_>;

    fn chown(&self, uid: u32, gid: u32) -> Self::ChownFut<'_>;

    fn chmod(&self, mode: Mode) -> Self::ChmodFut<'_>;

    fn link(&self) -> Self::LinkFut<'_>;

    fn unlink(&self) -> Self::UnlinkFut<'_>;

    fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> Self::ReadAtFut<'a>;

    fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> Self::WriteAtFut<'a>;

    fn sync(&self) -> Self::SyncFut<'_>;

    /// Append ".", ".." into this directory.
    fn append_dot(&self, parent_inode_id: InodeId) -> Self::AppendDotFut<'_>;

    fn lookup_raw<'a>(&'a self, name: &'a FsStr) -> Self::LookupRawFut<'a>;

    fn lookup<'a>(&'a self, name: &'a FsStr) -> Self::LookupFut<'a>;

    fn append(
        &self,
        dir_entry_name: DirEntryName,
        inode_id: InodeId,
        file_type: Option<FileType>,
    ) -> Self::AppendFut<'_>;

    fn remove<'a>(&'a self, dir_entry_name: &'a FsStr) -> Self::RemoveFut<'a>;

    fn ls_raw(&self) -> Self::LsRawFut<'_>;

    /// List all dir entrys in the current directory
    fn ls(&self) -> Self::LsFut<'_>;

    /// Call filesystem specific ioctl methods
    fn ioctl(&self, cmd: u32, arg: usize) -> Self::IOCtlFut<'_>;
}
