use crate::fs;
use crate::spinlock::RwLockIrq;

use crate::fs::vfs::{Error, Result};

/// Enumeration of possible methods to seek within an [File](File).
///
/// It is used by the [`seek`](File::seek) method.
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum SeekFrom {
    /// Sets the offset to the provided number of bytes.
    Start(u64),

    /// Sets the offset to the size of this file plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of a file, but it's an error to
    /// seek before byte 0.
    End(i64),

    /// Sets the offset to the current position plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of a file, but it's an error to
    /// seek before byte 0.
    Current(i64),
}

bitflags! {
    pub struct OpenOptions: u8 {
        const READ = 0x0;
        const WRITE = 0x1;
        const CREATE = 0x2;
        const APPEND = 0x3;
        const TRUNC = 0x4;
    }
}

#[derive(Debug, Clone)]
pub struct Description {
    offset: u64,
    opts: OpenOptions,
}

pub struct Descriptor {
    pub inode: fs::Inode,
    description: RwLockIrq<Description>,
    cloexec: bool,
}

impl Descriptor {
    pub fn new(inode: fs::Inode, opts: OpenOptions, cloexec: bool) -> Self {
        Self {
            inode,
            description: RwLockIrq::new(Description { offset: 0, opts }),
            cloexec,
        }
    }

    /// Seek to an offset, in bytes.
    pub async fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        Ok(match pos {
            SeekFrom::Start(offset) => {
                self.description.write().offset = offset;
                offset as u64
            }
            SeekFrom::End(delta) => {
                let metadata = self.inode.metadata().await?;
                let offset = metadata.size as i64 - delta;
                if offset < 0 {
                    return Err(Error::InvalidSeekOffset);
                }
                self.description.write().offset = offset as u64;
                offset as u64
            }
            SeekFrom::Current(delta) => {
                let mut desc = self.description.write();
                let offset = desc.offset as i64 - delta;
                if offset < 0 {
                    return Err(Error::InvalidSeekOffset);
                }
                desc.offset = offset as u64;
                offset as u64
            }
        })
    }

    /// Read some bytes from this file into the specified buffer, returning how many bytes were read.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut desc = self.description.write();
        let read_size = self.inode.read_at(desc.offset, buf).await?;
        desc.offset += read_size as u64;
        Ok(read_size)
    }

    /// Write a buffer into this file, returning how many bytes were written.
    pub async fn write(&mut self, src: &[u8]) -> Result<usize> {
        let mut desc = self.description.write();
        if !desc.opts.contains(OpenOptions::WRITE) {
            return Err(Error::ReadOnly);
        }
        let write_size = self.inode.write_at(desc.offset, src).await?;
        desc.offset += write_size as u64;
        Ok(write_size)
    }

    /// Flush this file, ensuring that all intermediately buffered contents reach their underlying device.
    pub async fn flush(&self) -> Result<()> {
        let opts = self.description.read().opts;
        if opts.contains(OpenOptions::WRITE) {
            self.inode.sync().await?;
        }
        Ok(())
    }
}

impl Clone for Descriptor {
    fn clone(&self) -> Self {
        Self {
            inode: self.inode.clone(),
            description: RwLockIrq::new(self.description.read().clone()),
            cloexec: self.cloexec,
        }
    }
}
