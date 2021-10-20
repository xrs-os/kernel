use crate::{Addr, BlkSize, Error, Result};
use alloc::{boxed::Box, vec::Vec};
use core::{
    any::Any,
    future::{ready, Future, Ready},
    mem::{self, MaybeUninit},
    slice,
};
use future_ext::{WithArg1, WithArg1Ext};
use futures_util::{
    future::{Either, Map, MapErr, MapOk},
    FutureExt, TryFutureExt,
};

pub type DiskError = Box<dyn Any + Send>;

pub type DiskResult<T> = core::result::Result<T, DiskError>;

pub trait Disk {
    type ReadAtFut<'a>: Future<Output = DiskResult<u32>> + Send + 'a;
    type WriteAtFut<'a>: Future<Output = DiskResult<u32>> + Send + 'a;
    type SyncFut<'a>: Future<Output = DiskResult<()>> + Send + 'a;

    fn read_at<'a>(&'a self, offset: u32, buf: &'a mut [u8]) -> Self::ReadAtFut<'a>;

    fn write_at<'a>(&'a self, offset: u32, buf: &'a [u8]) -> Self::WriteAtFut<'a>;

    fn sync(&self) -> Self::SyncFut<'_>;

    fn capacity(&self) -> u32;
}

pub(crate) async fn read_val_at<DK: Disk, T>(disk: &DK, offset: u32) -> DiskResult<T> {
    #[allow(clippy::uninit_assumed_init)]
    let mut ret: T = unsafe { mem::MaybeUninit::uninit().assume_init() };
    let buf =
        unsafe { slice::from_raw_parts_mut((&mut ret) as *mut _ as *mut u8, mem::size_of::<T>()) };
    disk.read_at(offset, buf).await?;
    Ok(ret)
}

pub type ReadAtFut<'a, DK> = MapErr<<DK as Disk>::ReadAtFut<'a>, fn(DiskError) -> Error>;

pub type ReadSliceFut<'a, DK> = MapOk<ReadAtFut<'a, DK>, fn(u32) -> u32>;

pub type ReadValAtFut<'a, T, DK> = Map<
    WithArg1<ReadAtFut<'a, DK>, Box<MaybeUninit<T>>>,
    fn((Result<u32>, Box<MaybeUninit<T>>)) -> Result<T>,
>;

type WriteAtFut<'a, DK> = MapErr<<DK as Disk>::WriteAtFut<'a>, fn(DiskError) -> Error>;

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
    pub fn read_at<'a>(
        &'a self,
        addr: Addr,
        buf: &'a mut [u8],
    ) -> MapErr<DK::ReadAtFut<'a>, fn(DiskError) -> Error> {
        let Self { disk, blk_size, .. } = self;
        disk.read_at(addr.abs_offset(*blk_size), buf)
            .map_err(Error::DiskError)
    }

    /// Reads block device data by `T` type slice,
    /// returning the number of `T` type elements read
    pub fn read_slice<'a, T>(&'a self, addr: Addr, buf: &'a mut [T]) -> ReadSliceFut<'a, DK> {
        let buf_u8 = unsafe {
            slice::from_raw_parts_mut(buf as *mut _ as *mut u8, mem::size_of::<T>() * buf.len())
        };
        self.read_at(addr, buf_u8)
            .map_ok(|read_u8_len| read_u8_len / mem::size_of::<T>() as u32)
    }

    /// Read `len` of `T` type data from block device,
    /// actual read length may be less than `len`
    pub async fn read_vec<'a, T: 'a>(&'a self, addr: Addr, len: u32) -> Result<Vec<T>> {
        let mut ret = Vec::with_capacity(len as usize);
        unsafe { ret.set_len(len as usize) };
        let read_len = self.read_slice(addr, ret.as_mut_slice()).await?;
        unsafe { ret.set_len(read_len as usize) };
        Ok(ret)
    }

    pub fn read_val_at<T>(&self, addr: Addr) -> ReadValAtFut<T, DK> {
        let mut val: Box<MaybeUninit<T>> = Box::new_uninit();
        let buf =
            unsafe { slice::from_raw_parts_mut(val.as_mut_ptr() as *mut u8, mem::size_of::<T>()) };
        self.read_at(addr, buf)
            .with_arg1(val)
            .map(|(res, val)| res.map(|_| *unsafe { val.assume_init() }))
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

    pub fn write_slice<'a, T>(
        &'a self,
        addr: Addr,
        buf: &'a [T],
    ) -> Either<Ready<Result<u32>>, WriteAtFut<'a, DK>> {
        let buf_u8 = unsafe {
            slice::from_raw_parts(
                buf as *const _ as *const u8,
                mem::size_of::<T>() * buf.len(),
            )
        };
        self.write_at(addr, buf_u8)
    }

    #[allow(clippy::type_complexity)]
    pub fn write_value_at<'a, T>(
        &'a self,
        addr: Addr,
        val: &'a T,
    ) -> MapOk<Either<Ready<Result<u32>>, WriteAtFut<'a, DK>>, fn(u32) -> ()> {
        let buf =
            unsafe { slice::from_raw_parts(val as *const _ as *const u8, mem::size_of::<T>()) };
        self.write_at(addr, buf).map_ok(|_| ())
    }

    pub fn disk(&self) -> &DK {
        &self.disk
    }

    pub fn sync(&self) -> MapErr<DK::SyncFut<'_>, fn(DiskError) -> Error> {
        let Self { disk, .. } = self;
        disk.sync().map_err(Error::DiskError)
    }
}
