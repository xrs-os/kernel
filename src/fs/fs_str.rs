use alloc::str;
use core::{borrow::Borrow, fmt, hash::Hash, ops::Deref};
pub const DIR_ENTRY_NAME_CAP: usize = 255;

#[repr(transparent)]
pub struct FsStr {
    inner: [u8],
}

impl FsStr {
    pub fn from_bytes(bytes: &[u8]) -> &Self {
        unsafe { &*(bytes as *const [u8] as *const Self) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn to_dir_entry_name(&self) -> DirEntryName {
        let mut bytes = [0; DIR_ENTRY_NAME_CAP];
        (&mut bytes[..self.inner.len()]).copy_from_slice(&self.inner);
        DirEntryName::new(bytes, self.len() as u8)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.inner.iter()
    }
}

impl fmt::Debug for FsStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", str::from_utf8(self.as_bytes()).unwrap())
    }
}

impl PartialEq for FsStr {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for FsStr {}

impl PartialOrd for FsStr {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_bytes().partial_cmp(other.as_bytes())
    }
}
impl Ord for FsStr {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

pub type DirEntryName = FsString<{ DIR_ENTRY_NAME_CAP }>;

impl<const CAP: usize> Deref for FsString<{ CAP }> {
    type Target = FsStr;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

#[derive(Clone)]
pub struct FsString<const CAP: usize> {
    inner: [u8; CAP],
    len: u8,
}

impl<const CAP: usize> FsString<CAP> {
    pub fn new(bytes: [u8; CAP], len: u8) -> Self {
        Self { inner: bytes, len }
    }

    pub fn into_inner(self) -> ([u8; CAP], u8) {
        (self.inner, self.len)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.inner[..self.len as usize]
    }
}

impl<const CAP: usize> AsRef<FsStr> for FsString<CAP> {
    fn as_ref(&self) -> &FsStr {
        FsStr::from_bytes(self.as_slice())
    }
}

impl<const CAP: usize> fmt::Debug for FsString<CAP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(AsRef::<FsStr>::as_ref(self), f)
    }
}

impl<const CAP: usize> PartialEq for FsString<CAP> {
    fn eq(&self, other: &Self) -> bool {
        AsRef::<FsStr>::as_ref(self) == other.as_ref()
    }
}
impl<const CAP: usize> Eq for FsString<CAP> {}

impl<const CAP: usize> Hash for FsString<CAP> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
        self.len.hash(state);
    }
}

impl<const CAP: usize> PartialOrd for FsString<CAP> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<const CAP: usize> Ord for FsString<CAP> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<const CAP: usize> Borrow<FsStr> for FsString<CAP> {
    fn borrow(&self) -> &FsStr {
        FsStr::from_bytes(self.as_bytes())
    }
}

impl<const CAP: usize> From<&str> for FsString<CAP> {
    fn from(s: &str) -> Self {
        let mut inner = [0; CAP];
        (&mut inner[..s.len()]).copy_from_slice(s.as_bytes());
        Self {
            inner,
            len: s.len() as u8,
        }
    }
}

impl<const CAP: usize> From<&FsStr> for FsString<CAP> {
    fn from(vfs_str: &FsStr) -> Self {
        let mut inner = [0; CAP];
        (&mut inner[..vfs_str.len()]).copy_from_slice(vfs_str.as_bytes());
        Self {
            inner,
            len: vfs_str.len() as u8,
        }
    }
}

impl<const CAP: usize> From<&[u8]> for FsString<CAP> {
    fn from(bytes: &[u8]) -> Self {
        let mut inner = [0; CAP];
        (&mut inner[..bytes.len()]).copy_from_slice(bytes);
        Self {
            inner,
            len: bytes.len() as u8,
        }
    }
}
