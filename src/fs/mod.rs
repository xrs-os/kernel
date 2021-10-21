pub mod blk;
mod cache_fs;
mod disk;
mod fs_str;
#[allow(clippy::type_complexity)]
#[cfg(feature = "naive_fs")]
pub mod naive_fs_vfs;
mod path;
mod ram_blk;
mod ram_vfs;
pub mod rootfs;
pub mod util;
pub mod vfs;

use alloc::sync::Arc;
pub use disk::Disk;
pub use fs_str::{DirEntryName, FsStr, FsString};
pub use path::*;

#[allow(clippy::type_complexity)]
pub mod mount_fs;

pub type Inode = Arc<dyn mount_fs::DynInode>;
pub type DirEntry = vfs::DirEntry<Arc<dyn mount_fs::DynFilesystem>>;

pub fn init(root_fs_inner: Arc<dyn mount_fs::DynFilesystem>) {
    rootfs::init(root_fs_inner)
}
