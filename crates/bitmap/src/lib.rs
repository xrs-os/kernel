#![no_std]

#[macro_use]
extern crate alloc;

use core::{convert::TryInto, mem};

use alloc::{boxed::Box, vec::Vec};

macro_rules! div_round_up {
    ($n:expr, $d:expr) => {
        ($n + ($d - 1)) / $d
    };
}

pub struct Bitmap(Box<[u64]>);

impl Bitmap {
    pub fn new(nbits: u32) -> Self {
        let size = div_round_up!(nbits, u64::BITS);
        Self(vec![0; size as usize].into())
    }

    pub fn to_bytes_be(&self, out: &mut [u8]) {
        let mut offset = 0;
        for row in &*self.0 {
            let row_be = row.to_be_bytes();
            out[offset..offset + row_be.len()].copy_from_slice(&row_be);
            offset += row_be.len();
        }
    }

    pub fn from_bytes_be(bytes: &[u8]) -> Self {
        let ratio = mem::size_of::<u64>() / mem::size_of::<u8>();
        let mut data: Vec<u64> = Vec::with_capacity(div_round_up!(bytes.len(), ratio));
        for b in bytes.chunks(ratio) {
            data.push(u64::from_be_bytes(b.try_into().unwrap()));
        }
        Self(data.into())
    }

    pub fn capacity(&self) -> u32 {
        self.0.len() as u32 * u64::BITS
    }

    /// Returns the bit of the `offset` position.
    /// true - 1
    /// false - 0
    pub fn test(&self, offset: u32) -> bool {
        let bit_mask = Self::bit_mask(offset);
        let idx = (offset / u64::BITS) as usize;
        let row = self.0[idx];
        (row & bit_mask) == bit_mask
    }

    /// Set the bit at the `offset` position to `val`,
    /// and return the value before it was set.
    /// true - 1
    /// false - 0
    pub fn test_and_set(&mut self, offset: u32, val: bool) -> bool {
        let bit_mask = Self::bit_mask(offset);
        let idx = (offset / u64::BITS) as usize;
        let row = self.0[idx];
        self.0[idx] = if val { row | bit_mask } else { row & !bit_mask };
        (row & bit_mask) == bit_mask
    }

    /// Returns the position of the next 0,
    /// after `offset` (including `offset`) and before `end` (excluding `end`).
    /// None means not existing
    pub fn find_next_zero(&self, offset: u32, end: Option<u32>) -> Option<u32> {
        let mut next_zero = None;
        let col = offset & (u64::BITS - 1);
        if col != 0 {
            // offset in the middle of usize
            let row = offset / u64::BITS;
            let num = self.0[row as usize] | (((1_u64 << col) - 1) << (u64::BITS - col));

            if num != u64::MAX {
                next_zero = Some(row * u64::BITS + num.leading_ones());
            }
        }

        if next_zero.is_none() {
            for i in div_round_up!(offset, u64::BITS)..self.0.len() as u32 {
                let num = unsafe { *self.0.get_unchecked(i as usize) };
                if num == 0 {
                    next_zero = Some(i * u64::BITS);
                    break;
                } else if num == u64::MAX {
                    continue;
                } else {
                    next_zero = Some(i * u64::BITS + num.leading_ones());
                    break;
                }
            }
        }
        next_zero.and_then(|nz| match end {
            Some(end) if nz >= end => None,
            _ => Some(nz),
        })
    }

    #[inline(always)]
    fn bit_mask(offset: u32) -> u64 {
        (1 << (u64::BITS - 1)) >> (offset & (u64::BITS - 1))
    }
}

#[cfg(test)]
mod test {

    use super::Bitmap;

    #[test]
    fn len_of_bitmap() {
        let cases = vec![(1, 1), (usize::BITS + 1, 2)];
        for (nbits, expected) in cases {
            assert_eq!(Bitmap::new(nbits).0.len(), expected);
        }
    }

    #[test]
    fn bitmap_test() {
        let mut bitmap = Bitmap::new(128);
        assert!(!bitmap.test(1));
        bitmap.test_and_set(1, true);
        assert!(bitmap.test(1));

        assert!(!bitmap.test(127));
        bitmap.test_and_set(127, true);
        assert!(bitmap.test(127));
    }

    #[test]
    fn bitmap_test_and_set() {
        let mut bitmap = Bitmap::new(32767);
        assert!(!bitmap.test_and_set(0, true));
        assert!(bitmap.test_and_set(0, true));

        assert!(!bitmap.test_and_set(1, true));
        assert!(bitmap.test_and_set(1, true));

        assert!(!bitmap.test_and_set(63, true));
        assert!(bitmap.test_and_set(63, true));
        assert!(!bitmap.test_and_set(64, true));
        assert!(bitmap.test_and_set(64, true));
        assert!(!bitmap.test_and_set(65, true));
        assert!(bitmap.test_and_set(65, true));

        let len = bitmap.0.len() as u32 * usize::BITS;
        assert!(!bitmap.test_and_set(len - 1, true));
        assert!(bitmap.test_and_set(len - 1, true));
    }

    #[test]
    fn bitmap_clear() {
        let mut bitmap = Bitmap::new(32767);
        bitmap.test_and_set(0, true);
        bitmap.test_and_set(0, false);
        assert!(!bitmap.test_and_set(0, true));

        bitmap.test_and_set(1, true);
        bitmap.test_and_set(2, true);
        bitmap.test_and_set(0, false);
        assert!(!bitmap.test_and_set(0, true));
        assert!(bitmap.test_and_set(1, true));

        bitmap.test_and_set(1, false);
        assert!(!bitmap.test_and_set(1, true));

        bitmap.test_and_set(63, true);
        bitmap.test_and_set(63, false);
        assert!(!bitmap.test_and_set(63, true));

        bitmap.test_and_set(64, true);
        bitmap.test_and_set(64, false);
        assert!(!bitmap.test_and_set(64, true));

        bitmap.test_and_set(100, false);
        assert!(!bitmap.test_and_set(100, true));
    }

    #[test]
    fn bitmap_find_next_zero() {
        let mut bitmap = Bitmap::new(32767);
        assert_eq!(bitmap.find_next_zero(0, None), Some(0));

        assert!(!bitmap.test_and_set(63, true));
        assert_eq!(bitmap.find_next_zero(0, None), Some(0));
        assert_eq!(bitmap.find_next_zero(63, None), Some(64));

        assert!(!bitmap.test_and_set(0, true));
        assert_eq!(bitmap.find_next_zero(0, None), Some(1));

        assert!(!bitmap.test_and_set(1, true));
        assert_eq!(bitmap.find_next_zero(0, None), Some(2));

        assert!(!bitmap.test_and_set(300, true));
        assert_eq!(bitmap.find_next_zero(300, None), Some(301));
        assert_eq!(bitmap.find_next_zero(400, None), Some(400));

        assert!(!bitmap.test_and_set(64, true));
        assert_eq!(bitmap.find_next_zero(64, None), Some(65));

        assert!(!bitmap.test_and_set(65, true));
        assert_eq!(bitmap.find_next_zero(64, None), Some(66));

        assert!(!bitmap.test_and_set(32767, true));
        assert_eq!(bitmap.find_next_zero(32766, None), Some(32766));
        assert_eq!(bitmap.find_next_zero(32767, None), None);

        let mut bitmap = Bitmap::new(32767);
        for i in 0..=32766 {
            bitmap.test_and_set(i, true);
        }
        assert_eq!(bitmap.find_next_zero(0, None), Some(32767));
        bitmap.test_and_set(32767, true);
        assert_eq!(bitmap.find_next_zero(0, None), None);
    }

    #[test]
    fn bitmap_find_next_zero_with_end() {
        let mut bitmap = Bitmap::new(10);
        assert_eq!(bitmap.find_next_zero(0, Some(10)), Some(0));

        bitmap.test_and_set(0, true);
        bitmap.test_and_set(1, true);
        assert_eq!(bitmap.find_next_zero(0, None), Some(2));
        assert_eq!(bitmap.find_next_zero(0, Some(3)), Some(2));
        assert_eq!(bitmap.find_next_zero(0, Some(2)), None);
    }
}
