use core::mem::MaybeUninit;

use alloc::sync::Arc;

use super::{mount_fs, vfs};

static mut ROOT_FS: MaybeUninit<vfs::Vfs<Arc<dyn mount_fs::DynFilesystem>>> = MaybeUninit::uninit();

pub fn root_fs() -> &'static vfs::Vfs<Arc<dyn mount_fs::DynFilesystem>> {
    unsafe { ROOT_FS.assume_init_ref() }
}

pub fn init(root_fs_inner: Arc<dyn mount_fs::DynFilesystem>) {
    unsafe {
        ROOT_FS = MaybeUninit::new(vfs::Vfs::new(Arc::new(mount_fs::MountFs::new(
            root_fs_inner,
        ))))
    }
}
