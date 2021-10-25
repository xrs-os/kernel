use alloc::{slice, sync::Arc, vec::Vec};
use core::{
    future::Future,
    marker::PhantomPinned,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{ready, Context, Poll},
};
use futures_util::future::BoxFuture;
use pin_project::pin_project;

use super::blk::{self, BlkDevice, BlkSize};

pub struct Disk {
    phy_blk_device: Arc<dyn BlkDevice>,
    capacity: usize,
}

impl Disk {
    pub fn new(phy_blk_device: Arc<dyn BlkDevice>) -> Self {
        Self {
            capacity: phy_blk_device.blk_size().mul(phy_blk_device.blk_count()),
            phy_blk_device,
        }
    }

    /// Read some bytes from this disk into the specified buffer, returning how many bytes were read.
    pub fn read_at<'a>(&'a self, offset: u64, buf: &'a mut [u8]) -> ReadAtFut<'a> {
        assert!(!buf.is_empty(), "buf must not be empty");
        ReadAtFut {
            phy_blk_device: &self.phy_blk_device,
            read_space: PhySpace::calc(
                offset,
                buf.len() as u64,
                self.phy_blk_device.blk_size(),
                self.phy_blk_device.blk_count(),
            ),
            buf: BufRef(buf),
            read_size: 0,
            state: ReadAtState::HeadPartialBlk(None),
        }
    }

    /// Write a buffer into this disk, returning how many bytes were written.
    pub fn write_at<'a>(&'a self, offset: u64, src: &'a [u8]) -> WriteAtFut<'a> {
        assert!(!src.is_empty(), "src must not be empty");
        WriteAtFut {
            phy_blk_device: &self.phy_blk_device,
            write_space: PhySpace::calc(
                offset,
                src.len() as u64,
                self.phy_blk_device.blk_size(),
                self.phy_blk_device.blk_count(),
            ),
            src,
            written_size: 0,
            state: WriteAtState::HeadPartialBlk(None),
        }
    }

    /// Sync disk, ensuring that all intermediately buffered contents reach their destination.
    pub async fn sync(&self) -> blk::Result<()> {
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Future for the [`read_at`](Disk::read_at)
#[pin_project]
pub struct ReadAtFut<'a> {
    phy_blk_device: &'a Arc<dyn BlkDevice>,
    read_space: Option<PhySpace>,
    buf: BufRef<'a>,
    read_size: usize,
    #[pin]
    state: ReadAtState<'a>,
}

#[pin_project(project = ReadAtStateProj)]
enum ReadAtState<'a> {
    /// If the offset starts in the middle of a block in the underlying block device,
    /// Need to read out the entire block and copy the required part to the buf.
    HeadPartialBlk(#[pin] Option<ReadPartialBlkFutAndData<'a>>),
    /// If the last part of the data to be read is less than one block,
    /// Need to read out the entire block and copy the required part to the buf.
    TailPartialBlk(#[pin] Option<ReadPartialBlkFutAndData<'a>>),
    FullBlks {
        blk_id: usize,
        #[pin]
        fut: Option<BoxFuture<'a, blk::Result<()>>>,
    },
}

#[pin_project]
struct ReadPartialBlkFutAndData<'a> {
    #[pin]
    fut: BoxFuture<'a, blk::Result<()>>,
    #[pin]
    blk_data: BlkData,
    #[pin]
    _pin: PhantomPinned,
}

impl<'a> ReadPartialBlkFutAndData<'a> {
    fn new(fut: BoxFuture<'a, blk::Result<()>>, blk_data: BlkData) -> Self {
        Self {
            fut,
            blk_data,
            _pin: PhantomPinned,
        }
    }
}

impl Future for ReadAtFut<'_> {
    type Output = blk::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        let read_space = match this.read_space {
            Some(read_space) => read_space,
            None => return Poll::Ready(Ok(0)),
        };

        let blk_size = this.phy_blk_device.blk_size().size() as usize;
        loop {
            let new_state = match this.state.as_mut().project() {
                ReadAtStateProj::HeadPartialBlk(fut_and_data) => match fut_and_data.as_pin_mut() {
                    None => {
                        if !read_space.has_partial_head_blk() {
                            ReadAtState::FullBlks {
                                blk_id: read_space.start_blk_id,
                                fut: None,
                            }
                        } else {
                            let mut blk_data = BlkData(vec![0; blk_size]);

                            ReadAtState::HeadPartialBlk(Some(ReadPartialBlkFutAndData::new(
                                this.phy_blk_device
                                    .read_blk(read_space.start_blk_id, unsafe {
                                        blk_data.as_mut_slice()
                                    }),
                                blk_data,
                            )))
                        }
                    }

                    Some(fut_and_data) => {
                        let fut_and_data_proj = fut_and_data.project();
                        ready!(fut_and_data_proj.fut.poll(cx)?);

                        if read_space.start_blk_id == read_space.end_blk_id {
                            let src = &fut_and_data_proj.blk_data[read_space
                                .pos_of_head_partial_blk
                                .unwrap()
                                ..read_space.pos_of_tail_partial_blk.unwrap()];
                            (&mut this.buf[..src.len()]).copy_from_slice(src);
                            return Poll::Ready(Ok(src.len()));
                        }

                        let src = &fut_and_data_proj.blk_data
                            [read_space.pos_of_head_partial_blk.unwrap()..];
                        (&mut this.buf[..src.len()]).copy_from_slice(src);
                        *this.read_size += src.len();
                        ReadAtState::FullBlks {
                            blk_id: read_space.start_blk_id + 1,
                            fut: None,
                        }
                    }
                },

                ReadAtStateProj::TailPartialBlk(fut_and_data) => match fut_and_data.as_pin_mut() {
                    None => {
                        if !read_space.has_partial_tail_blk() {
                            return Poll::Ready(Ok(*this.read_size));
                        }
                        let mut blk_data =
                            BlkData(vec![0; this.phy_blk_device.blk_size().size() as usize]);
                        ReadAtState::TailPartialBlk(Some(ReadPartialBlkFutAndData::new(
                            this.phy_blk_device.read_blk(read_space.end_blk_id, unsafe {
                                blk_data.as_mut_slice()
                            }),
                            blk_data,
                        )))
                    }

                    Some(fut_and_data) => {
                        let fut_and_data_proj = fut_and_data.project();
                        ready!(fut_and_data_proj.fut.poll(cx)?);
                        let src = &fut_and_data_proj.blk_data
                            [..read_space.pos_of_tail_partial_blk.unwrap()];
                        (&mut this.buf[*this.read_size..]).copy_from_slice(src);
                        *this.read_size += src.len();
                        return Poll::Ready(Ok(*this.read_size));
                    }
                },

                ReadAtStateProj::FullBlks { blk_id, fut } => match fut.as_pin_mut() {
                    None => {
                        if *blk_id as isize > read_space.last_full_blk() {
                            // FullBlocks finished reading, try to read the last part of the data if necessary.
                            ReadAtState::TailPartialBlk(None)
                        } else {
                            let buf = unsafe { this.buf.extend_lifetime() };
                            ReadAtState::FullBlks {
                                blk_id: *blk_id,
                                fut: Some(this.phy_blk_device.read_blk(
                                    *blk_id,
                                    &mut buf[*this.read_size..*this.read_size + blk_size],
                                )),
                            }
                        }
                    }
                    Some(fut) => {
                        ready!(fut.poll(cx)?);
                        *this.read_size += blk_size;
                        // Read next full-block data
                        ReadAtState::FullBlks {
                            blk_id: *blk_id + 1,
                            fut: None,
                        }
                    }
                },
            };

            this.state.set(new_state);
        }
    }
}

/// Future for the [write_at](Disk::write_at) method.
#[pin_project]
pub struct WriteAtFut<'a> {
    phy_blk_device: &'a Arc<dyn BlkDevice>,
    write_space: Option<PhySpace>,
    src: &'a [u8],
    written_size: usize,
    #[pin]
    state: WriteAtState<'a>,
}

#[pin_project(project = WriteAtStateProj)]
enum WriteAtState<'a> {
    HeadPartialBlk(#[pin] Option<WriteHeadPartialBlkFut<'a>>),
    TailPartialBlk(#[pin] Option<WriteTailPartialBlkFut<'a>>),
    FullBlks {
        blk_id: usize,
        #[pin]
        fut: Option<BoxFuture<'a, blk::Result<()>>>,
    },
}

impl Future for WriteAtFut<'_> {
    type Output = blk::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        let write_space = match this.write_space {
            Some(write_space) => write_space,
            None => return Poll::Ready(Ok(0)),
        };

        let blk_size = this.phy_blk_device.blk_size().size() as usize;
        loop {
            let new_state = match this.state.as_mut().project() {
                WriteAtStateProj::HeadPartialBlk(write_partial_blk_fut) => {
                    match write_partial_blk_fut.as_pin_mut() {
                        None => {
                            if !write_space.has_partial_head_blk() {
                                WriteAtState::FullBlks {
                                    blk_id: write_space.start_blk_id,
                                    fut: None,
                                }
                            } else {
                                let src = if write_space.start_blk_id == write_space.end_blk_id {
                                    &this.src[..write_space.pos_of_tail_partial_blk.unwrap()
                                        - write_space.pos_of_head_partial_blk.unwrap()]
                                } else {
                                    &this.src
                                        [..blk_size - write_space.pos_of_head_partial_blk.unwrap()]
                                };

                                WriteAtState::HeadPartialBlk(Some(write_head_partial_blk(
                                    *this.phy_blk_device,
                                    write_space.start_blk_id,
                                    src,
                                )))
                            }
                        }
                        Some(write_partial_blk_fut) => {
                            let written_size = ready!(write_partial_blk_fut.poll(cx)?);
                            if write_space.start_blk_id == write_space.end_blk_id {
                                return Poll::Ready(Ok(written_size));
                            }
                            *this.written_size += written_size;
                            WriteAtState::FullBlks {
                                blk_id: write_space.start_blk_id + 1,
                                fut: None,
                            }
                        }
                    }
                }
                WriteAtStateProj::TailPartialBlk(write_partial_blk_fut) => {
                    match write_partial_blk_fut.as_pin_mut() {
                        None => {
                            if !write_space.has_partial_tail_blk() {
                                return Poll::Ready(Ok(*this.written_size));
                            }
                            WriteAtState::TailPartialBlk(Some(write_tail_partial_blk(
                                *this.phy_blk_device,
                                write_space.end_blk_id,
                                &this.src[*this.written_size..],
                            )))
                        }
                        Some(write_partial_blk_fut) => {
                            *this.written_size += ready!(write_partial_blk_fut.poll(cx)?);
                            return Poll::Ready(Ok(*this.written_size));
                        }
                    }
                }
                WriteAtStateProj::FullBlks { blk_id, fut } => match fut.as_pin_mut() {
                    None => {
                        if *blk_id as isize > write_space.last_full_blk() {
                            // FullBlocks finished writing, try to write the last part of the data if necessary.
                            WriteAtState::TailPartialBlk(None)
                        } else {
                            WriteAtState::FullBlks {
                                blk_id: *blk_id,
                                fut: Some(this.phy_blk_device.write_blk(
                                    *blk_id,
                                    &this.src[*this.written_size..*this.written_size + blk_size],
                                )),
                            }
                        }
                    }
                    Some(fut) => {
                        ready!(fut.poll(cx)?);

                        *this.written_size += blk_size;
                        // Write next full-block data
                        WriteAtState::FullBlks {
                            blk_id: *blk_id + 1,
                            fut: None,
                        }
                    }
                },
            };
            this.state.set(new_state);
        }
    }
}

/// Future for [write_partial_blk](write_partial_blk) function.
/// To write PartialBlock data, need to read the entire block data,
/// Then modify the data, finally write it back
#[pin_project]
struct WritePartialBlkFut<'a, const HEAD_OR_TAIL: bool> {
    phy_blk_device: &'a Arc<dyn BlkDevice>,
    blk_id: usize,
    blk_data: BlkData,
    src: &'a [u8],
    #[pin]
    state: WritePartialBlkState<'a>,
}

type WriteHeadPartialBlkFut<'a> = WritePartialBlkFut<'a, true>;
type WriteTailPartialBlkFut<'a> = WritePartialBlkFut<'a, false>;

fn write_head_partial_blk<'a>(
    phy_blk_device: &'a Arc<dyn BlkDevice>,
    blk_id: usize,
    src: &'a [u8],
) -> WriteHeadPartialBlkFut<'a> {
    WriteHeadPartialBlkFut {
        state: WritePartialBlkState::Init,
        phy_blk_device,
        blk_id,
        blk_data: BlkData(vec![0u8; phy_blk_device.blk_size().size() as usize]),
        src,
    }
}

fn write_tail_partial_blk<'a>(
    phy_blk_device: &'a Arc<dyn BlkDevice>,
    blk_id: usize,
    src: &'a [u8],
) -> WriteTailPartialBlkFut<'a> {
    WriteTailPartialBlkFut {
        state: WritePartialBlkState::Init,
        phy_blk_device,
        blk_id,
        blk_data: BlkData(vec![0u8; phy_blk_device.blk_size().size() as usize]),
        src,
    }
}

#[pin_project(project = WritePartialBlkStateProj)]
enum WritePartialBlkState<'a> {
    Init,
    ReadBlk(#[pin] BoxFuture<'a, blk::Result<()>>),
    WriteBlk(#[pin] BoxFuture<'a, blk::Result<()>>),
}

impl<'a, const HEAD_OR_TAIL: bool> Future for WritePartialBlkFut<'a, HEAD_OR_TAIL> {
    type Output = blk::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        loop {
            let new_state = match this.state.as_mut().project() {
                WritePartialBlkStateProj::Init => WritePartialBlkState::ReadBlk(
                    this.phy_blk_device
                        .read_blk(*this.blk_id, unsafe { this.blk_data.as_mut_slice() }),
                ),
                WritePartialBlkStateProj::ReadBlk(read_fut) => {
                    ready!(read_fut.poll(cx)?);
                    if HEAD_OR_TAIL {
                        (&mut this.blk_data[..this.src.len()]).copy_from_slice(this.src);
                    } else {
                        let blk_data_len = this.blk_data.len();
                        (&mut this.blk_data[blk_data_len - this.src.len()..])
                            .copy_from_slice(this.src);
                    }

                    WritePartialBlkState::WriteBlk(
                        this.phy_blk_device
                            .write_blk(*this.blk_id, unsafe { this.blk_data.as_mut_slice() }),
                    )
                }
                WritePartialBlkStateProj::WriteBlk(write_fut) => {
                    ready!(write_fut.poll(cx)?);
                    return Poll::Ready(Ok(this.src.len()));
                }
            };
            this.state.set(new_state);
        }
    }
}

/// PhySpace represents space range of blk device.
#[derive(Debug)]
struct PhySpace {
    start_blk_id: usize,
    end_blk_id: usize,
    /// The starting position of this PhySpace in the first block.
    /// if None, the first block is a full-block.
    pos_of_head_partial_blk: Option<usize>,
    /// The end position of this PhySpace in the last block.
    /// if None, the last block is a full-block.
    pos_of_tail_partial_blk: Option<usize>,
}

impl PhySpace {
    fn calc(abs_offset: u64, len: u64, blk_size: BlkSize, blk_count: usize) -> Option<Self> {
        let start_blk_id = blk_size.div_by(abs_offset) as usize;
        if start_blk_id >= blk_count {
            return None;
        }

        let pos_of_head_partial_blk = blk_size.mod_by(abs_offset) as usize;
        let len_of_head_partial_blk = blk_size.size() as usize - pos_of_head_partial_blk;

        let (end_blk_id, pos_of_tail_partial_blk) = if len < len_of_head_partial_blk as u64 {
            (start_blk_id, pos_of_head_partial_blk + len as usize)
        } else {
            let remainder_len = len - len_of_head_partial_blk as u64;
            let end_blk_id = start_blk_id + blk_size.div_round_up_by(remainder_len) as usize;

            if end_blk_id >= blk_count {
                (blk_count - 1, 0)
            } else {
                (end_blk_id, blk_size.mod_by(remainder_len) as usize)
            }
        };

        Some(Self {
            start_blk_id,
            end_blk_id,
            pos_of_head_partial_blk: if pos_of_head_partial_blk > 0 {
                Some(pos_of_head_partial_blk)
            } else {
                None
            },
            pos_of_tail_partial_blk: if pos_of_tail_partial_blk > 0 {
                Some(pos_of_tail_partial_blk)
            } else {
                None
            },
        })
    }

    fn has_partial_head_blk(&self) -> bool {
        self.pos_of_head_partial_blk.is_some()
    }

    fn has_partial_tail_blk(&self) -> bool {
        self.pos_of_tail_partial_blk.is_some()
    }

    fn last_full_blk(&self) -> isize {
        if self.has_partial_tail_blk() {
            self.end_blk_id as isize - 1
        } else {
            self.end_blk_id as isize
        }
    }
}

struct BufRef<'a>(&'a mut [u8]);

impl BufRef<'_> {
    unsafe fn extend_lifetime<'a>(&mut self) -> &'a mut [u8] {
        slice::from_raw_parts_mut(self.0.as_mut_ptr(), self.0.len())
    }
}

impl Deref for BufRef<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl DerefMut for BufRef<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

struct BlkData(Vec<u8>);

impl BlkData {
    unsafe fn as_mut_slice<'a>(&mut self) -> &'a mut [u8] {
        slice::from_raw_parts_mut(self.0.as_mut_ptr(), self.0.len())
    }
}

impl Deref for BlkData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for BlkData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}
