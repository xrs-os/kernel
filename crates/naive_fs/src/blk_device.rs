use crate::{div_round_up, Addr, BlkSize, Error, Result};
use alloc::{boxed::Box, vec::Vec};
use core::{
    any::Any,
    future::{ready, Future, Ready},
    mem, slice,
};
use future_ext::{WithArg1, WithArg1Ext};
use futures_util::{
    future::{Either, Map, MapErr, MapOk},
    FutureExt, TryFutureExt,
};

pub type DiskError = Box<dyn Any + Send>;

pub type DiskResult<T> = core::result::Result<T, DiskError>;

pub trait FromBytes {
    const BYTES_LEN: usize;

    fn from_bytes(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized;
}

pub trait ToBytes {
    fn bytes_len(&self) -> usize;

    fn to_bytes(&self, out: &mut [u8]);
}

pub trait Disk: 'static {
    type ReadAtFut<'a>: Future<Output = DiskResult<u32>> + Send + 'a
    where
        Self: 'a;
    type WriteAtFut<'a>: Future<Output = DiskResult<u32>> + Send + 'a
    where
        Self: 'a;
    type SyncFut<'a>: Future<Output = DiskResult<()>> + Send + 'a
    where
        Self: 'a;

    fn read_at<'a>(&'a self, offset: u32, buf: &'a mut [u8]) -> Self::ReadAtFut<'a>;

    fn write_at<'a>(&'a self, offset: u32, buf: &'a [u8]) -> Self::WriteAtFut<'a>;

    fn sync(&self) -> Self::SyncFut<'_>;

    fn capacity(&self) -> u32;
}

pub(crate) async fn read_val_at<DK: Disk, T: FromBytes>(disk: &DK, offset: u32) -> DiskResult<T> {
    let mut bytes = vec![0; T::BYTES_LEN];
    disk.read_at(offset, &mut bytes).await?;
    Ok(T::from_bytes(&bytes).unwrap())
}

pub type ReadAtFut<'a, DK> = MapErr<<DK as Disk>::ReadAtFut<'a>, fn(DiskError) -> Error>;

pub type ReadValAtFut<'a, T, DK> =
    Map<WithArg1<ReadAtFut<'a, DK>, Vec<u8>>, fn((Result<u32>, Vec<u8>)) -> Result<T>>;

type WriteAtFut<'a, DK> = MapErr<<DK as Disk>::WriteAtFut<'a>, fn(DiskError) -> Error>;

pub type WriteValueAtFut<'a, DK> = Map<
    WithArg1<Either<Ready<Result<u32>>, WriteAtFut<'a, DK>>, Vec<u8>>,
    fn((Result<u32>, Vec<u8>)) -> Result<()>,
>;

pub type ReadBytesFut<'a, DK> =
    Map<WithArg1<ReadAtFut<'a, DK>, Vec<u8>>, fn((Result<u32>, Vec<u8>)) -> Result<Vec<u8>>>;
/// Logic block devices
pub struct BlkDevice<DK> {
    disk: DK,
    pub blk_size: BlkSize,
    read_only: bool,
}

impl<DK: Disk> BlkDevice<DK> {
    pub fn new(disk: DK, blk_size: BlkSize, read_only: bool) -> Self {
        Self {
            disk,
            blk_size,
            read_only,
        }
    }

    /// Reads block device data by byte
    /// and returns the number of bytes of data read
    pub fn read_at<'a>(&'a self, addr: Addr, buf: &'a mut [u8]) -> ReadAtFut<'a, DK> {
        let Self { disk, blk_size, .. } = self;
        disk.read_at(addr.abs_offset(*blk_size), buf)
            .map_err(Error::DiskError)
    }

    /// Read bytes data from block device,
    /// actual read length may be less than `len`
    pub fn read_bytes(&self, addr: Addr, len: u32) -> ReadBytesFut<'_, DK> {
        let mut ret = vec![0; len as usize];

        self.read_at(addr, unsafe {
            slice::from_raw_parts_mut(ret.as_mut_ptr(), ret.len())
        })
        .with_arg1(ret)
        .map(|(read_len_res, mut ret)| {
            read_len_res.map(|read_len| {
                unsafe { ret.set_len(read_len as usize) };
                ret
            })
        })
    }

    /// Read `len` of `T` type data from block device,
    /// actual read length may be less than `len`
    #[allow(clippy::type_complexity)]
    pub fn read_vec<T: FromBytes>(
        &self,
        addr: Addr,
        len: u32,
    ) -> MapOk<ReadBytesFut<'_, DK>, fn(Vec<u8>) -> Vec<T>> {
        let ratio = T::BYTES_LEN / mem::size_of::<u8>();
        self.read_bytes(addr, len * ratio as u32).map_ok(|bytes| {
            let ratio = T::BYTES_LEN / mem::size_of::<u8>();
            let mut ret = Vec::with_capacity(div_round_up!(bytes.len(), ratio));
            for item_bytes in bytes.chunks(ratio) {
                match T::from_bytes(item_bytes) {
                    Some(item) => ret.push(item),
                    None => break,
                }
            }
            ret
        })
    }

    pub fn read_val_at<T: FromBytes>(&self, addr: Addr) -> ReadValAtFut<T, DK> {
        let mut bytes = vec![0; T::BYTES_LEN];
        self.read_at(addr, unsafe {
            slice::from_raw_parts_mut(bytes.as_mut_ptr(), bytes.len())
        })
        .with_arg1(bytes)
        .map(|(res, bytes)| res.map(|_| T::from_bytes(&bytes).unwrap()))
    }

    pub fn write_at<'a>(
        &'a self,
        addr: Addr,
        buf: &'a [u8],
    ) -> Either<Ready<Result<u32>>, WriteAtFut<'a, DK>> {
        let Self {
            disk,
            blk_size,
            read_only,
        } = self;
        if *read_only {
            return Either::Left(ready(Err(Error::ReadOnly)));
        }
        Either::Right(
            disk.write_at(addr.abs_offset(*blk_size), buf)
                .map_err(Error::DiskError),
        )
    }

    pub fn write_value_at<'a, T: ToBytes>(
        &'a self,
        addr: Addr,
        val: &'a T,
    ) -> WriteValueAtFut<'a, DK> {
        let mut bytes = vec![0; val.bytes_len()];
        val.to_bytes(&mut bytes);
        self.write_at(addr, unsafe {
            slice::from_raw_parts(bytes.as_ptr(), bytes.len())
        })
        .with_arg1(bytes)
        .map(|(res, _)| res.map(|_| ()))
    }

    #[allow(clippy::type_complexity)]
    pub fn write_slice<'a, T: ToBytes>(
        &'a self,
        addr: Addr,
        slice: &'a [T],
    ) -> Map<
        WithArg1<Either<Ready<Result<u32>>, WriteAtFut<'a, DK>>, Vec<u8>>,
        fn((Result<u32>, Vec<u8>)) -> Result<u32>,
    > {
        let mut bytes_buf = if slice.is_empty() {
            Vec::new()
        } else {
            let item_byte_len = slice[0].bytes_len();
            let mut bytes_buf = vec![0; slice.len() * item_byte_len];
            let mut offset = 0;
            for item in slice {
                item.to_bytes(&mut bytes_buf[offset..offset + item_byte_len]);
                offset += item_byte_len;
            }
            bytes_buf
        };

        self.write_at(addr, unsafe {
            slice::from_raw_parts_mut(bytes_buf.as_mut_ptr(), bytes_buf.len())
        })
        .with_arg1(bytes_buf)
        .map(|(res, _)| res)
    }

    pub fn disk(&self) -> &DK {
        &self.disk
    }

    pub fn sync(&self) -> MapErr<DK::SyncFut<'_>, fn(DiskError) -> Error> {
        let Self { disk, .. } = self;
        disk.sync().map_err(Error::DiskError)
    }
}
