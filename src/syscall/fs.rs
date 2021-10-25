use core::slice;

use alloc::sync::Arc;

use super::{Error, Result};
use crate::{
    fs::{vfs, FsStr},
    proc::{file::SeekFrom, thread::Thread},
    time::Timespec,
};

// If pathname is relative and fd is the special value AT_FDCWD, then pathname is interpreted relative to the current working directory of the calling process.
const AT_FDCWD: isize = -100;

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

pub async fn sys_lseek(
    thread: &Arc<Thread>,
    fd: usize,
    offset: i64,
    whence: LSeekWhence,
) -> Result {
    let mut descriptor = thread.proc().get_file(fd as usize).ok_or(Error::EBADF)?;
    let seek_from = match whence {
        LSeekWhence::Set => SeekFrom::Start(offset as u64),
        LSeekWhence::Cur => SeekFrom::Current(offset),
        LSeekWhence::End => SeekFrom::End(offset),
    };
    Ok(descriptor.seek(seek_from).await? as usize)
}

pub async fn sys_read(thread: &Arc<Thread>, fd: usize, buf: *mut u8, count: usize) -> Result {
    let mut descriptor = thread.proc().get_file(fd as usize).ok_or(Error::EBADF)?;
    let buf = unsafe { slice::from_raw_parts_mut(buf, count) };
    let len = descriptor.read(buf).await?;
    Ok(len)
}

pub async fn sys_write(thread: &Arc<Thread>, fd: usize, buf: *const u8, count: usize) -> Result {
    let mut descriptor = thread.proc().get_file(fd as usize).ok_or(Error::EBADF)?;
    let buf = unsafe { slice::from_raw_parts(buf, count) };
    let len = descriptor.write(buf).await?;
    Ok(len)
}

pub async fn sys_fstat(thread: &Arc<Thread>, fd: isize, stat: &mut Stat) -> Result {
    sys_fstatat(thread, fd, &[], stat, FStatAtFlags::empty()).await
}

pub async fn sys_fstatat(
    thread: &Arc<Thread>,
    fd: isize,
    path: &[u8],
    stat: &mut Stat,
    _flag: FStatAtFlags,
) -> Result {
    // TODO: flag AT_SYMLINK_NOFOLLOW
    let proc = thread.proc();
    let mut inode = if fd == AT_FDCWD {
        proc.cwd.read().inode().await?.ok_or(Error::ENOENT)?
    } else {
        proc.get_file(fd as usize).ok_or(Error::EBADF)?.inode
    };

    if path.is_empty() {
        inode = inode
            .lookup(FsStr::from_bytes(path))
            .await?
            .ok_or(Error::ENOENT)?
            .inode()
            .await?
            .ok_or(Error::ENOENT)?
    }

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
