use core::{marker::PhantomData, ptr};

use crate::{Addr, Error, Result, VirtualAddress};

use super::{
    flush::{FlushAllGuard, FlushGuard},
    frame::{Allocator, LockedAllocator},
    table::{NextPageError, PageTable, PageTableEntry},
    Flag, Frame, Page, PageParam,
};

pub struct PageMapper<'a, MutexType, A, Param> {
    // Address space identifier, representing the specified process in tlb
    asid: Option<usize>,
    root_table: PageTable<Param>,
    allocator: &'a LockedAllocator<MutexType, A>,
    _maker: PhantomData<Param>,
}

impl<MutexType, A, Param> PageMapper<'_, MutexType, A, Param>
where
    Param: PageParam,
{
    pub fn set_asid(&mut self, asid: usize) {
        self.asid = Some(asid)
    }

    pub fn asid(&self) -> Option<usize> {
        self.asid
    }

    /// # Safety
    pub unsafe fn activate(&self) {
        // todo asid
        Param::activate_root_table(self.root_table.frame.start(), None);
        FlushAllGuard::<Param>::new(None).flush()
    }
}

impl<'a, MutexType, A, Param> PageMapper<'a, MutexType, A, Param>
where
    MutexType: lock_api::RawMutex,
    A: Allocator,
    Param: PageParam,
    [(); Param::PAGE_LEVELS]:,
{
    /// Create a new PageMapper.
    pub fn create(allocator: &'a LockedAllocator<MutexType, A>) -> Result<Self> {
        let root_table_base_addr = allocator.alloc().ok_or(Error::NoSpace)?.start();

        Ok(Self::new(
            PageTable::new(Frame::of_addr(root_table_base_addr)),
            allocator,
        ))
    }

    pub fn new(root_table: PageTable<Param>, allocator: &'a LockedAllocator<MutexType, A>) -> Self {
        Self {
            asid: None,
            root_table,
            allocator,
            _maker: PhantomData,
        }
    }

    /// # Safety
    pub unsafe fn alloc_and_map(
        &mut self,
        page: &Page,
        flags: Flag,
        init_data: &[u8],
    ) -> Result<FlushGuard<Param>> {
        let frame = self.allocator.alloc().ok_or(Error::NoSpace)?;
        let flush_guard = self.map(page, &frame, flags)?;
        let addr = Param::linear_phys_to_kvirt(frame.start());
        ptr::copy(init_data.as_ptr(), addr.0 as *mut u8, init_data.len());
        Ok(flush_guard)
    }

    /// # Safety
    pub unsafe fn map(
        &mut self,
        page: &Page,
        frame: &Frame,
        flags: Flag,
    ) -> Result<FlushGuard<Param>> {
        let mut tab = self.root_table();
        let pte_idxs = Param::pte_idxs(page.start());
        for &pte_idx in &pte_idxs[0..pte_idxs.len() - 1] {
            let mut pte = tab
                .get_entry(pte_idx)
                .ok_or_else(|| Error::InvalidVirtualAddress(page.start()))?;
            match pte.next_page_table() {
                Ok(next) => tab = next,
                Err(NextPageError::Invalid) => {
                    // Next level page table does not exist
                    // Create next level page table
                    let next = PageTable::new(self.allocator.alloc().ok_or(Error::NoSpace)?);
                    pte.set_nonleaf(next.frame.start());
                    tab = next;
                }
                Err(NextPageError::NoNext) => {
                    return Err(Error::InvalidVirtualAddress(page.start()));
                }
            }
        }

        tab.get_entry(pte_idxs[pte_idxs.len() - 1])
            .ok_or_else(|| Error::InvalidVirtualAddress(page.start()))?
            .set(frame.start(), flags);

        Ok(FlushGuard::new(self.asid, page.clone()))
    }

    /// # Safety
    pub unsafe fn unmap_and_dealloc(&mut self, page: &Page) -> Result<Option<FlushGuard<Param>>> {
        Ok(if let Some((flush_guard, pte)) = self.unmap(page)? {
            self.allocator.dealloc(&pte.frame());
            Some(flush_guard)
        } else {
            None
        })
    }

    /// # Safety
    pub unsafe fn unmap(
        &mut self,
        page: &Page,
    ) -> Result<Option<(FlushGuard<Param>, PageTableEntry<Param>)>> {
        let mut tab = self.root_table();
        for &pte_idx in Param::pte_idxs(page.start()).iter() {
            let mut pte = tab
                .get_entry(pte_idx)
                .ok_or_else(|| Error::InvalidVirtualAddress(page.start()))?;

            match pte.next_page_table() {
                Ok(next) => tab = next,
                Err(NextPageError::Invalid) => {
                    return Err(Error::InvalidVirtualAddress(page.start()));
                }
                Err(NextPageError::NoNext) => {
                    // This is already a leaf node
                    return Ok(if pte.free(self.allocator) {
                        Some((FlushGuard::new(self.asid, page.clone()), pte))
                    } else {
                        None
                    });
                }
            }
        }
        Err(Error::InvalidVirtualAddress(page.start()))
    }

    pub fn free_page_table(&mut self) -> FlushAllGuard<Param> {
        self.root_table.free(self.allocator);
        FlushAllGuard::new(self.asid)
    }

    pub fn borrow_memory(&self, asid: usize) -> Result<Self> {
        let mut new_mapper = Self::new(
            self.root_table.borrow_memory(self.allocator)?,
            self.allocator,
        );

        new_mapper.set_asid(asid);

        Ok(new_mapper)
    }

    pub fn handle_page_fault(&mut self, addr: VirtualAddress) -> Result<FlushGuard<Param>> {
        let src_page = Page::of_addr(addr.align_down_to_shift(Param::PAGE_SIZE_SHIFT));
        let target_frame = self.allocator.alloc().ok_or(Error::NoSpace)?;
        unsafe {
            let src_page_data: &[u8] =
                core::slice::from_raw_parts(src_page.start().as_mut_ptr(), Param::PAGE_SIZE);

            let target_page_data: &mut [u8] = core::slice::from_raw_parts_mut(
                Param::linear_phys_to_kvirt(target_frame.start()).as_mut_ptr(),
                Param::PAGE_SIZE,
            );
            target_page_data.copy_from_slice(src_page_data);

            let (flush, pte) = self.unmap(&src_page)?.unwrap();
            flush.ignore();
            self.map(
                &src_page,
                &target_frame,
                Param::pte_flags(Param::pte_set_writable(pte.data())),
            )
        }
    }

    pub fn root_table(&self) -> PageTable<Param> {
        self.root_table.clone()
    }
}
