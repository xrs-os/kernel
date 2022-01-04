pub mod blk;
mod cache_fs;
pub mod devfs;
mod disk;
pub mod fs_str;
mod ioctl;
#[allow(clippy::type_complexity)]
#[cfg(feature = "naive_fs")]
pub mod naive_fs_vfs;
mod path;
mod ram_blk;
mod ram_vfs;
pub mod rootfs;
pub mod util;
pub mod vfs;

use core::mem::MaybeUninit;

use alloc::sync::Arc;
pub use disk::Disk;
pub use fs_str::{DirEntryName, FsStr, FsString};
pub use path::*;

use crate::{driver, fs::devfs::tty::TtyInode, proc};

use self::{mount_fs::DynInode, rootfs::root_fs};

#[allow(clippy::type_complexity)]
pub mod mount_fs;

pub type Inode = Arc<dyn mount_fs::DynInode>;
pub type DirEntry = vfs::DirEntry<Arc<dyn mount_fs::DynFilesystem>>;

static mut TTY: MaybeUninit<Arc<TtyInode>> = MaybeUninit::uninit();

pub fn tty() -> &'static Arc<TtyInode> {
    unsafe { TTY.assume_init_ref() }
}

pub fn init() {
    proc::executor::block_on(async move {
        rootfs::init(create_fs_inner().await);
        // mount device filesystem
        unsafe { TTY = MaybeUninit::new(Arc::new(TtyInode::new())) };

        let dev_fs = Arc::new(devfs::DevFs::new(vec![(
            "tty".into(),
            Some(vfs::FileType::ChrDev),
            tty().clone() as Arc<dyn devfs::DevInode>,
        )]));

        let dev_dir = find_or_create_dev_dir()
            .await
            .expect("field to find or create `/dev` directory");

        mount_fs::mount(dev_dir, dev_fs)
            .await
            .expect("field to mount dev fs");
    });
}

async fn create_fs_inner() -> Arc<dyn mount_fs::DynFilesystem> {
    let blk_device = driver::blk_drivers()
        .first()
        .expect("No block device could be found.")
        .clone();

    #[cfg(feature = "naive_fs")]
    {
        let naivefs = Arc::new(
            naive_fs_vfs::NaiveFs::open(Disk::new(blk_device), false)
                .await
                .expect("Failed to open naive filesystem."),
        );
        Arc::new(naivefs) // TODO trace err
    }
}

async fn find_or_create_dev_dir() -> vfs::Result<Arc<dyn DynInode>> {
    let root_dir_entry = root_fs().root().await;
    Ok(
        match root_fs()
            .find_parent_dentry(&root_dir_entry, Path::from_bytes("dev".as_bytes()))
            .await?
        {
            Some(dev) => dev.as_dir().await?.ok_or(vfs::Error::WrongFS)?,
            None => {
                let new_inode = root_fs()
                    .create_parent_dentry(
                        &root_dir_entry,
                        FsStr::from_bytes("dev".as_bytes()),
                        vfs::Mode::TY_DIR
                            | vfs::Mode::PERM_RWX_USR
                            | vfs::Mode::PERM_RX_GRP
                            | vfs::Mode::PERM_RX_OTH,
                        0,
                        0,
                        Default::default(),
                    )
                    .await?;
                new_inode
            }
        },
    )
}
