use core::slice;

use alloc::sync::Arc;

use super::{Error, Result};
use crate::{
    fs::{self, rootfs::root_fs, vfs},
    proc::{
        file::{self, SeekFrom},
        thread::Thread,
    },
    time::Timespec,
};

// If pathname is relative and fd is the special value AT_FDCWD, then pathname is interpreted relative to the current working directory of the calling process.
const AT_FDCWD: isize = -100;

#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// File serial number
    ino: u64,
    /// Mode of file
    mode: u32,
    /// Number of hard links
    nlink: u32,
    /// User ID of the file
    uid: u32,
    /// Group ID of the file
    gid: u32,
    /// Device ID
    rdev: u64,
    /// padding
    _pad: u64,
    /// file size, in bytes
    size: u64,
    /// optimal blocksize for I/O
    blk_size: u32,
    /// padding2
    _pad2: u32,
    /// blocks allocated for file
    blk_cnt: u32,
    /// time of last access
    atime: Timespec,
    /// time of last data modification
    mtime: Timespec,
    /// time of last status change
    ctime: Timespec,
}

bitflags! {
    pub struct FStatAtFlags: u32 {
        const AT_SYMLINK_NOFOLLOW = 0x100;
        const AT_NO_AUTOMOUNT = 0x800;
    }
}

bitflags! {
    pub struct OpenFlags: usize {
        /// read only
        const RDONLY = 0;
        /// write only
        const WRONLY = 1;
        /// read write
        const RDWR = 2;
        /// create file if it does not exist
        const CREATE = 1 << 6;
        /// error if CREATE and the file exists
        const EXCLUSIVE = 1 << 7;
        /// truncate file upon open
        const TRUNCATE = 1 << 9;
        /// append on each write
        const APPEND = 1 << 10;
        /// close on exec
        const CLOEXEC = 1 << 19;
    }
}

impl OpenFlags {
    fn readable(&self) -> bool {
        let b = self.bits() & 0b11;
        b == OpenFlags::RDONLY.bits() || b == OpenFlags::RDWR.bits()
    }
    fn writable(&self) -> bool {
        let b = self.bits() & 0b11;
        b == OpenFlags::WRONLY.bits() || b == OpenFlags::RDWR.bits()
    }
}

num_enum::num_enum! (
    pub LSeekWhence:u8 {
        // The file offset is set to offset bytes.
        Set = 0,
        // The file offset is set to its current location plus offset bytes.
        Cur = 1,
        // The file offset is set to the size of the file plus offset bytes.
        End = 2,
    }
);

pub async fn sys_openat(
    thread: &Arc<Thread>,
    dirfd: isize,
    path: &fs::Path,
    flags: OpenFlags,
    mode: fs::vfs::Mode,
) -> Result {
    let inode = if flags.contains(OpenFlags::CREATE) {
        let (dirpath, basename) = match path.pop() {
            (path, Some(basename)) => (path, basename),
            (path, None) => (fs::Path::from_bytes(".".as_bytes()), path.inner()),
        };
        let dir_inode = lookup_inode_at(thread, dirfd, dirpath).await?;
        match dir_inode.lookup(basename).await? {
            Some(file) => {
                if flags.contains(OpenFlags::EXCLUSIVE) {
                    return Err(Error::EEXIST);
                }
                // TODO: TRUNCATE
                file.inode().await?.ok_or(Error::ENOENT)?
            }
            None => {
                root_fs()
                    .create(&dir_inode, basename, mode, 0, 0, Default::default())
                    .await?
            }
        }
    } else {
        lookup_inode_at(thread, dirfd, path).await?
    };

    let descriptor = file::Descriptor::new(inode, flags.into(), flags.contains(OpenFlags::CLOEXEC));
    let fd = thread
        .proc()
        .open_files
        .add_file(descriptor)
        .ok_or(Error::EMFILE)?;
    Ok(fd)
}

pub fn sys_close(thread: &Arc<Thread>, fd: isize) -> Result {
    let proc = thread.proc();
    proc.open_files
        .remove_file(fd as usize)
        .ok_or(Error::EBADF)?;
    Ok(0)
}

pub async fn sys_lseek(
    thread: &Arc<Thread>,
    fd: isize,
    offset: i64,
    whence: LSeekWhence,
) -> Result {
    let mut descriptor = thread
        .proc()
        .open_files
        .get_file(fd as usize)
        .ok_or(Error::EBADF)?;
    let seek_from = match whence {
        LSeekWhence::Set => SeekFrom::Start(offset as u64),
        LSeekWhence::Cur => SeekFrom::Current(offset),
        LSeekWhence::End => SeekFrom::End(offset),
    };
    Ok(descriptor.seek(seek_from).await? as usize)
}

pub async fn sys_read(thread: &Arc<Thread>, fd: isize, buf: *mut u8, count: usize) -> Result {
    let mut descriptor = thread
        .proc()
        .open_files
        .get_file(fd as usize)
        .ok_or(Error::EBADF)?;
    let buf = unsafe { slice::from_raw_parts_mut(buf, count) };
    let len = descriptor.read(buf).await?;
    Ok(len)
}

pub async fn sys_write(thread: &Arc<Thread>, fd: isize, buf: *const u8, count: usize) -> Result {
    let mut descriptor = thread
        .proc()
        .open_files
        .get_file(fd as usize)
        .ok_or(Error::EBADF)?;
    let buf = unsafe { slice::from_raw_parts(buf, count) };
    let len = descriptor.write(buf).await?;
    Ok(len)
}

pub async fn sys_fstat(thread: &Arc<Thread>, fd: isize, stat: &mut Stat) -> Result {
    sys_fstatat(
        thread,
        fd,
        fs::Path::from_bytes(&[]),
        stat,
        FStatAtFlags::empty(),
    )
    .await
}

pub async fn sys_fstatat(
    thread: &Arc<Thread>,
    dirfd: isize,
    path: &fs::Path,
    stat: &mut Stat,
    _flag: FStatAtFlags,
) -> Result {
    // TODO: flag AT_SYMLINK_NOFOLLOW
    let inode = lookup_inode_at(thread, dirfd, path).await?;
    let metadata = inode.metadata().await?;
    stat.dev = 0;
    stat.ino = inode.id() as u64;
    stat.mode = metadata.mode.bits() as u32;
    stat.nlink = metadata.links_count as u32;
    stat.uid = metadata.uid;
    stat.gid = metadata.gid;
    stat.rdev = 0;
    stat.size = metadata.size;
    stat.blk_size = metadata.blk_size;
    stat.blk_cnt = metadata.blk_count as u32;
    stat.atime = metadata.atime;
    stat.mtime = metadata.mtime;
    stat.ctime = metadata.ctime;
    Ok(0)
}

//  If the `dirfd` is the special value `AT_FDCWD`, then the directory is
//   current working directory of the process.
pub async fn lookup_inode_at(
    thread: &Arc<Thread>,
    dirfd: isize,
    path: &fs::Path,
) -> core::result::Result<fs::Inode, Error> {
    let proc = thread.proc();
    let mut inode = if dirfd == AT_FDCWD {
        proc.cwd.read().await.inode().await?.ok_or(Error::ENOENT)?
    } else {
        proc.open_files
            .get_file(dirfd as usize)
            .ok_or(Error::EBADF)?
            .inode
    };

    if !path.is_empty() {
        inode = root_fs()
            .find(&inode, path)
            .await?
            .ok_or(Error::ENOENT)?
            .inode()
            .await?
            .ok_or(Error::ENOENT)?;
    }
    Ok(inode)
}

impl From<OpenFlags> for file::OpenOptions {
    fn from(flags: OpenFlags) -> Self {
        let mut open_options = Self::empty();
        if flags.readable() {
            open_options |= Self::READ;
        }
        if flags.writable() {
            open_options |= Self::WRITE;
        }
        if flags.contains(OpenFlags::TRUNCATE) {
            open_options |= Self::TRUNC;
        }
        if flags.contains(OpenFlags::APPEND) {
            open_options |= Self::APPEND;
        }
        if flags.contains(OpenFlags::CREATE) {
            open_options |= Self::CREATE;
        }
        open_options
    }
}

impl From<vfs::Error> for Error {
    fn from(vfs_error: vfs::Error) -> Self {
        match vfs_error {
            vfs::Error::NotDir => Error::ENOTDIR,
            vfs::Error::NoRootDir => Error::ENOTDIR,
            vfs::Error::NoSuchFileOrDirectory => Error::ENOENT,
            vfs::Error::EntryExist => Error::EEXIST,
            vfs::Error::NoSpace => Error::ENOSPC,
            vfs::Error::BlkErr(_) => Error::EIO,
            vfs::Error::Eof => todo!(),
            vfs::Error::InvalidDirEntryName(_) => Error::EINVAL,
            vfs::Error::WrongFS => Error::EINVAL,
            vfs::Error::ReadOnly => Error::EROFS,
            vfs::Error::UnsupportedFs(_) => Error::ENOSYS,
            vfs::Error::InvalidSeekOffset => Error::EINVAL,
            vfs::Error::Unsupport => Error::ENOSYS,
            vfs::Error::NoSuchProcess(_) => Error::ESRCH,
        }
    }
}
