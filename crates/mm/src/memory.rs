use super::{
    frame::Allocator,
    page::{flush::FlushAllGuard, mapper::PageMapper, Flag, PageParam},
    Error, Frame, PageIter, Result, VirtualAddress,
};
use alloc::vec::Vec;
use core::ops::Range;

pub struct Memory<'a, MutexType, A, Param> {
    kernel_segments: Vec<Segment>,
    user_segments: Vec<Segment>,
    // todo for debug `pub`
    pub page_mapper: PageMapper<'a, MutexType, A, Param>,
}

impl<MutexType, A, Param> Memory<'_, MutexType, A, Param>
where
    Param: PageParam,
{
    pub fn activate(&self) {
        unsafe {
            self.page_mapper.activate();
        }
    }

    pub fn set_asid(&mut self, asid: usize) {
        self.page_mapper.set_asid(asid)
    }
}

impl<'a, MutexType, A, Param> Memory<'a, MutexType, A, Param>
where
    MutexType: lock_api::RawMutex,
    A: Allocator,
    Param: PageParam,
    [(); Param::PAGE_LEVELS]:,
    [(); Param::PAGE_SIZE]:,
{
    pub fn new(page_mapper: PageMapper<'a, MutexType, A, Param>) -> Self {
        Self {
            kernel_segments: Vec::new(),
            user_segments: Vec::new(),
            page_mapper,
        }
    }

    pub fn borrow_memory(&self, asid: usize) -> Result<Self> {
        let new_page_mapper = self.page_mapper.borrow_memory(asid)?;

        Ok(Self {
            kernel_segments: self.kernel_segments.clone(),
            user_segments: self.user_segments.clone(),
            page_mapper: new_page_mapper,
        })
    }

    pub fn add_kernel_segment(&mut self, segment: Segment) -> Result<FlushAllGuard<Param>> {
        self.check_overlap(&segment.addr_range)?;
        let flush_all_guard = segment.map(&mut self.page_mapper, &[])?;
        self.kernel_segments.push(segment);
        Ok(flush_all_guard)
    }

    pub fn add_user_segment(
        &mut self,
        segment: Segment,
        init_data: &[u8],
    ) -> Result<FlushAllGuard<Param>> {
        self.check_overlap(&segment.addr_range)?;
        let flush_all_guard = segment.map(&mut self.page_mapper, init_data)?;
        self.user_segments.push(segment);
        Ok(flush_all_guard)
    }

    pub fn remove_user_segments(&mut self) -> Result<Option<FlushAllGuard<Param>>> {
        if self.user_segments.is_empty() {
            return Ok(None);
        }
        let flush_all_guard = self
            .user_segments
            .first()
            .unwrap()
            .unmap(&mut self.page_mapper)?;
        for segment in self.user_segments.iter().skip(1) {
            segment.unmap(&mut self.page_mapper)?;
        }
        self.user_segments.truncate(0);
        Ok(Some(flush_all_guard))
    }

    // Check if `addr_range` and existing segments overlap
    fn check_overlap(&self, addr_range: &Range<VirtualAddress>) -> Result<()> {
        for segment in self.kernel_segments.iter().chain(self.user_segments.iter()) {
            if segment.addr_range.contains(&addr_range.start)
                || addr_range.contains(&segment.addr_range.start)
            {
                return Err(Error::AddressOverlap(
                    segment.addr_range.clone(),
                    addr_range.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MapType {
    Linear,
    Framed,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub addr_range: Range<VirtualAddress>,
    pub flags: Flag,
    pub map_type: MapType,
}

impl Segment {
    pub fn size(&self) -> usize {
        self.addr_range.end.0 - self.addr_range.start.0
    }

    // The current version of rust, using const generic when there is a life cycle, will ICE.
    // https://github.com/rust-lang/rust/issues/85031#issuecomment-842533694
    // Temporary solution: Turn off incremental compilation
    pub fn page_iter<const SIZE: usize>(&self) -> PageIter<'_, SIZE> {
        PageIter::new(&self.addr_range)
    }

    pub fn map<'a, MutexType, A, Param>(
        &self,
        page_mapper: &mut PageMapper<'a, MutexType, A, Param>,
        init_data: &[u8],
    ) -> Result<FlushAllGuard<Param>>
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
        Param: PageParam,
        [(); Param::PAGE_LEVELS]:,
        [(); Param::PAGE_SIZE]:,
    {
        unsafe {
            match self.map_type {
                MapType::Linear => {
                    for page in self.page_iter::<{ Param::PAGE_SIZE }>() {
                        let frame = Frame::of_addr(Param::linear_virt_to_phys(page.start()));
                        page_mapper.map(&page, &frame, self.flags)?.ignore()
                    }
                }
                MapType::Framed => {
                    for page in self.page_iter::<{ Param::PAGE_SIZE }>() {
                        let mut page_init_data = [0; { Param::PAGE_SIZE }];

                        let start_pos = page.start().0 as isize - self.addr_range.start.0 as isize;
                        if !init_data.is_empty() && start_pos < init_data.len() as isize {
                            // segment.addr_range.start may not be aligned to page size.
                            let page_init_data_start = if self.addr_range.start.0 > page.start().0 {
                                self.addr_range.start.0 - page.start().0
                            } else {
                                0
                            };

                            let init_data_start =
                                page.start().0 + page_init_data_start - self.addr_range.start.0;

                            let init_data_end = init_data_start
                                + (Param::PAGE_SIZE - page_init_data_start)
                                    .min(self.addr_range.end.0 - page.start().0)
                                    .min(init_data.len() - init_data_start);

                            let buf = &init_data[init_data_start..init_data_end];
                            (&mut page_init_data
                                [page_init_data_start..page_init_data_start + buf.len()])
                                .copy_from_slice(buf);
                        };

                        page_mapper
                            .alloc_and_map(&page, self.flags, &page_init_data)?
                            .ignore()
                    }
                }
            }
        }

        // todo
        Ok(FlushAllGuard::new(page_mapper.asid()))
    }

    pub fn unmap<'a, MutexType, A, Param>(
        &self,
        page_mapper: &mut PageMapper<'a, MutexType, A, Param>,
    ) -> Result<FlushAllGuard<Param>>
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
        Param: PageParam,
        [(); Param::PAGE_LEVELS]:,
        [(); Param::PAGE_SIZE]:,
    {
        match self.map_type {
            MapType::Linear => {
                for page in self.page_iter::<{ Param::PAGE_SIZE }>() {
                    unsafe {
                        if let Some((guard, _)) = page_mapper.unmap(&page)? {
                            guard.ignore()
                        }
                    }
                }
            }
            MapType::Framed => {
                for page in self.page_iter::<{ Param::PAGE_SIZE }>() {
                    unsafe {
                        if let Some(guard) = page_mapper.unmap_and_dealloc(&page)? {
                            guard.ignore()
                        }
                    }
                }
            }
        }
        // todo
        Ok(FlushAllGuard::new(page_mapper.asid()))
    }
}
