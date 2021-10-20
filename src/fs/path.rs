use super::FsStr;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq)]
pub struct Path(FsStr);

impl Path {
    pub fn from_bytes(bytes: &[u8]) -> &Self {
        unsafe { &*(bytes as *const [u8] as *const Self) }
    }

    pub fn is_root(&self) -> bool {
        self.0.iter().all(|&c| c == b'/')
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn shift(&self) -> (&Self, Option<&FsStr>) {
        let mut bytes = self.0.as_bytes();

        // Eat leading '/'
        match bytes.iter().position(|&c| c != b'/') {
            Some(start_pos) => bytes = &bytes[start_pos..],
            None => return (self, None),
        }

        let len = bytes.iter().position(|&c| c == b'/').unwrap_or(bytes.len());
        return (
            Self::from_bytes(&bytes[len..]),
            Some(FsStr::from_bytes(&bytes[..len])),
        );
    }

    pub fn pop(&self) -> (&Self, Option<&FsStr>) {
        let mut bytes = self.0.as_bytes();

        match bytes.last() {
            Some(b'/') => {
                // Eat trailing '/'
                match bytes.iter().rposition(|&c| c != b'/') {
                    Some(end_pos) => bytes = &bytes[..end_pos],
                    None => return (self, None),
                }
            }
            None => return (self, None),
            _ => {}
        }

        let start_pos = bytes
            .iter()
            .rposition(|&c| c == b'/')
            .map(|x| x + 1)
            .unwrap_or(0);

        return (
            Self::from_bytes(&bytes[..start_pos]),
            Some(FsStr::from_bytes(&bytes[start_pos..])),
        );
    }

    pub fn inner(&self) -> &FsStr {
        &self.0
    }
}
