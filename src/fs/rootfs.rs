use core::mem::MaybeUninit;

use alloc::sync::Arc;

use super::{
    mount_fs::{DynFilesystem, MountFs},
    vfs::Vfs,
};

static mut ROOT_FS: MaybeUninit<Vfs<Arc<dyn DynFilesystem>>> = MaybeUninit::uninit();

pub fn root_fs() -> &'static Vfs<Arc<dyn DynFilesystem>> {
    unsafe { ROOT_FS.assume_init_ref() }
}

pub fn init(root_fs_inner: Arc<dyn DynFilesystem>) {
    unsafe { ROOT_FS = MaybeUninit::new(Vfs::new(Arc::new(MountFs::new(root_fs_inner)))) }
}
