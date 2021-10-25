use crate::{inode::Inode, InodeId};

use super::{blk_device::Disk, Error, Result};
use alloc::{boxed::Box, str, string::String, vec::Vec};
use core::{fmt, mem, pin::Pin};
use futures_util::{pin_mut, stream, Stream, StreamExt};

/// RawDirEntry
#[repr(C, packed)]
pub struct RawDirEntry {
    /// inode number of the directory entry.
    pub inode_id: InodeId,
    /// 16bit unsigned displacement to the next directory entry
    /// from the start of the current directory entry.
    /// Directory entries must be 4-byte aligned
    /// and cannot span multiple blocks.
    pub rec_len: u16,
    /// file type
    pub file_type: FileType,
    /// File name length
    pub name_len: u8,
    /// name
    name: [u8; 255],
}

impl RawDirEntry {
    pub fn new(inode_id: InodeId, name: DirEntryName, file_type: FileType) -> Self {
        Self::with_rec_len(inode_id, name, file_type, mem::size_of::<Self>() as u16)
    }

    pub fn with_rec_len(
        inode_id: InodeId,
        name: DirEntryName,
        file_type: FileType,
        rec_len: u16,
    ) -> Self {
        let (name_bytes, name_len) = name.into_inner();
        Self {
            inode_id,
            rec_len,
            name_len,
            file_type,
            name: name_bytes,
        }
    }

    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }

    pub fn raw_name(self) -> [u8; 255] {
        self.name
    }
}

impl<MutexType, DK> Inode<MutexType, DK>
where
    MutexType: lock_api::RawMutex,
    DK: Disk + Sync,
{
    /// Append ".", ".." to this directory.
    pub async fn append_dot(&self, parent_inode_id: InodeId) -> Result<()> {
        self.check_dir().await?;
        let dot_raw_dir_entry =
            RawDirEntry::new(self.inode_id, ".".as_bytes().into(), FileType::Dir);
        self.write(0, &dot_raw_dir_entry).await?;

        let dotdot_raw_dir_entry =
            RawDirEntry::new(parent_inode_id, "..".as_bytes().into(), FileType::Dir);
        self.write(dot_raw_dir_entry.rec_len as u32, &dotdot_raw_dir_entry)
            .await?;
        Ok(())
    }

    pub async fn lookup(&self, name: &[u8]) -> Result<Option<RawDirEntry>> {
        self.check_dir().await?;
        let mut dir_entry_stream = self.dir_entry_stream();
        let mut dir_entry_stream_pinned = unsafe { Pin::new_unchecked(&mut dir_entry_stream) };
        loop {
            match dir_entry_stream_pinned.next().await {
                Some(Ok((dir_entry, _))) => {
                    if dir_entry.name() == name {
                        return Ok(Some(dir_entry));
                    }
                }
                Some(Err(e)) => return Err(e),
                None => return Ok(None),
            }
        }
    }

    pub async fn append(
        &self,
        inode_id: InodeId,
        name: DirEntryName,
        file_type: FileType,
    ) -> Result<()> {
        check_dir_entry_name(name.as_slice())?;
        self.check_dir().await?;
        let mut dir_entry_stream = self.dir_entry_stream();
        let mut dir_entry_stream_pinned = unsafe { Pin::new_unchecked(&mut dir_entry_stream) };

        // Skip '.' and '..'
        dir_entry_stream_pinned
            .next()
            .await
            .expect("Expect `.` dir entry.")?;
        let (_, mut insert_offset) = dir_entry_stream_pinned
            .next()
            .await
            .expect("Expect `..` dir entry.")?;

        let raw_dir_entry_size = mem::size_of::<RawDirEntry>() as u16;
        let new_rec_len = loop {
            match dir_entry_stream_pinned.next().await {
                Some(Ok((mut dir_entry, offset))) => {
                    insert_offset = offset + dir_entry.rec_len as u32;

                    if dir_entry.rec_len >= raw_dir_entry_size * 2 {
                        // There is enough space in the current dir_entry to store a new dir_entry
                        let origin_rev_len = dir_entry.rec_len;
                        dir_entry.rec_len = raw_dir_entry_size;
                        self.write(offset, &dir_entry).await?;
                        break origin_rev_len - raw_dir_entry_size;
                    }
                }
                Some(Err(e)) => return Err(e),
                None => break raw_dir_entry_size,
            }
        };

        let raw_dir_entry = RawDirEntry::with_rec_len(inode_id, name, file_type, new_rec_len);
        self.write(insert_offset, &raw_dir_entry).await?;

        Ok(())
    }

    pub async fn remove(&self, name: &[u8]) -> Result<Option<RawDirEntry>> {
        check_dir_entry_name(name)?;
        self.check_dir().await?;
        let dir_entry_stream = self.dir_entry_stream();
        pin_mut!(dir_entry_stream);

        let mut last_dir_entry: Option<RawDirEntry> = None;

        loop {
            match dir_entry_stream.next().await {
                Some(Ok((dir_entry, offset))) => {
                    if dir_entry.name() == name {
                        // Delete by merging into the previous dir_entry
                        if let Some(mut last_raw_dir_entry) = last_dir_entry {
                            last_raw_dir_entry.rec_len += dir_entry.rec_len;
                            self.write(offset, &last_raw_dir_entry).await?;
                            return Ok(Some(dir_entry));
                        }
                    }

                    last_dir_entry = Some(dir_entry);
                }
                Some(Err(e)) => return Err(e),
                None => return Ok(None),
            }
        }
    }

    pub async fn ls(&self) -> Result<Vec<RawDirEntry>> {
        self.check_dir().await?;

        let dir_entry_stream = self.dir_entry_stream();
        pin_mut!(dir_entry_stream);
        let mut dentrys = Vec::new();
        loop {
            match dir_entry_stream.next().await {
                Some(Ok((dir_entry, _))) => dentrys.push(dir_entry),
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }

        Ok(dentrys)
    }

    fn dir_entry_stream(&self) -> impl Stream<Item = Result<(RawDirEntry, u32)>> + '_ {
        stream::try_unfold(0, move |offset| async move {
            match self.read::<RawDirEntry>(offset).await? {
                Some(raw_dir_entry) if raw_dir_entry.inode_id != 0 => {
                    let rec_len = raw_dir_entry.rec_len;
                    let next_offset = offset + rec_len as u32;
                    Ok::<_, Error>(Some(((raw_dir_entry, next_offset), next_offset)))
                }
                _ => Ok(None),
            }
        })
    }

    async fn check_dir(&self) -> Result<()> {
        let mode = self.mode().await;
        if !mode.is_dir() {
            Err(Error::NotDir)
        } else {
            Ok(())
        }
    }
}

fn check_dir_entry_name(name: &[u8]) -> Result<()> {
    if name == ".".as_bytes() || name == "..".as_bytes() {
        Err(Error::InvalidDirEntryName(Box::new(name.into())))
    } else {
        Ok(())
    }
}

pub struct DirEntryName {
    bytes: [u8; 255],
    len: u8,
}

impl DirEntryName {
    pub fn new(bytes: [u8; 255], len: u8) -> Self {
        Self { bytes, len }
    }

    pub fn into_inner(self) -> ([u8; 255], u8) {
        (self.bytes, self.len)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub fn into_string(mut self) -> String {
        unsafe { String::from_raw_parts(self.bytes.as_mut_ptr(), self.len as usize, 255) }
    }
}

impl From<&[u8]> for DirEntryName {
    fn from(s: &[u8]) -> Self {
        let mut bytes = [0; 255];
        (&mut bytes[..s.len()]).copy_from_slice(s);
        Self::new(bytes, s.len() as u8)
    }
}

impl From<DirEntryName> for String {
    fn from(den: DirEntryName) -> Self {
        den.into_string()
    }
}

impl fmt::Debug for DirEntryName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", str::from_utf8(self.as_slice()).unwrap())
    }
}

/// DirEntry file type
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
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
