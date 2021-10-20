use alloc::vec::Vec;

use super::{vfs::Result, Inode};

pub async fn read_all(file: Inode) -> Result<Vec<u8>> {
    let size = file.metadata().await?.size as usize;
    let mut buf = Vec::with_capacity(size);
    unsafe {
        buf.set_len(size);
    }

    file.read_at(0, &mut buf).await?;
    Ok(buf)
}
