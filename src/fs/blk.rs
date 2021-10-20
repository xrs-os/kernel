use core::{marker::PhantomData, ops};

use futures_util::future::BoxFuture;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// The device is not ready.
    NotReady,
    /// Failed to alloc DMA memory.
    DmaErr,
    /// I/O Error
    IoErr,
    /// Invalid parameter.
    InvalidParam,
}

/// BlkDevice represents a block device.
pub trait BlkDevice: Send + Sync {
    /// Read the data of the specified block into the `buf` slice.
    /// Buf slice length must equal blk_size.
    fn read_blk<'a>(&'a self, blk_id: usize, buf: &'a mut [u8]) -> BoxFuture<'a, Result<()>>;

    /// Writes `src` slice data to the specified block.
    /// Buf slice length must equal blk_size.
    fn write_blk<'a>(&'a self, blk_id: usize, src: &'a [u8]) -> BoxFuture<'a, Result<()>>;

    /// Get the BlkDevice's block_size.
    fn blk_size(&self) -> BlkSize;

    /// Get the BlkDevice's block count.
    fn blk_count(&self) -> usize;
}

/// The block size type.
#[derive(Debug, Clone, Copy)]
pub struct BlkSize<BS = u32> {
    /// log2(blk_size)
    pub blk_size_log2: u8,
    _maker: PhantomData<BS>,
}

impl<BS> BlkSize<BS>
where
    BS: From<u8> + ops::Shl<u8, Output = BS>,
{
    /// Create BlkSize
    pub fn new(blk_size: u32) -> Self {
        assert!(blk_size.is_power_of_two(), "block_size must be power of 2.");

        Self {
            blk_size_log2: (blk_size - 1).count_ones() as u8,
            _maker: PhantomData,
        }
    }

    /// Create BlkSize with log2(blk_size) value.
    pub fn with_blk_size_log2(blk_size_log2: u8) -> Self {
        Self {
            blk_size_log2,
            _maker: PhantomData,
        }
    }

    /// Returns block size.
    pub fn size(&self) -> BS {
        BS::from(1) << self.blk_size_log2
    }

    /// Performs `dividend` / `blk_size`.
    pub fn div_by<D: ops::Shr<u8, Output = D>>(&self, dividend: D) -> D {
        dividend >> self.blk_size_log2
    }

    ///  Performs `dividend` / `blk_size` and round up to the nearest integer if not evenly divisable.
    pub fn div_round_up_by<D>(&self, dividend: D) -> D
    where
        BS: ops::Sub<Output = BS>,
        D: From<BS> + ops::Add<Output = D> + ops::Shr<u8, Output = D>,
    {
        (dividend + D::from(self.size() - BS::from(1))) >> self.blk_size_log2
    }

    /// Performs `m` * `blk_size`.
    pub fn mul<M: ops::Shl<u8, Output = M>>(&self, m: M) -> M {
        m << self.blk_size_log2
    }

    /// Performs `dividend` % `blk_size`.
    pub fn mod_by<D>(&self, dividend: D) -> D
    where
        BS: ops::Sub<Output = BS>,
        D: From<BS> + ops::BitAnd<Output = D>,
    {
        dividend & D::from(self.size() - BS::from(1))
    }
}
