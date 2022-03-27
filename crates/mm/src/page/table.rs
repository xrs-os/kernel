use core::{marker::PhantomData, option::Option};

use crate::{frame::LockedAllocator, Addr, Error, Result};

use super::{frame::Allocator, Frame, PageParam, PhysicalAddress, VirtualAddress};

pub enum NextPageError {
    NoNext,
    Invalid,
}

pub struct PageTable<Param> {
    pub frame: Frame,
    _maker: PhantomData<Param>,
}

impl<Param: PageParam> PageTable<Param> {
    pub fn new(frame: Frame) -> Self {
        Self {
            frame,
            _maker: PhantomData,
        }
    }

    /// # Safety
    /// Get the specified page table entry
    pub unsafe fn get_entry(&self, idx: usize) -> Option<PageTableEntry<Param>> {
        if idx < Param::PTE_COUNT {
            Some(self.get_entry_unchecked(idx))
        } else {
            None
        }
    }

    /// # Safety
    /// Get the specified page table entry
    pub unsafe fn get_entry_unchecked(&self, idx: usize) -> PageTableEntry<Param> {
        let pte_virt_addr = self.entry_virt_unchecked(idx);
        PageTableEntry::new(pte_virt_addr.0 as *mut usize)
    }

    pub fn free<MutexType, A>(&mut self, allocator: &LockedAllocator<MutexType, A>)
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
    {
        unsafe { self.entry_iter() }.for_each(|mut pte| {
            pte.free(allocator);
        });
        allocator.dealloc(&self.frame);
    }

    pub fn borrow_memory<MutexType, A>(
        &self,
        allocator: &LockedAllocator<MutexType, A>,
    ) -> Result<Self>
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
    {
        let target_frame = allocator.alloc().ok_or(Error::NoSpace)?;

        for (idx, pte) in unsafe { self.entry_iter() }.enumerate() {
            let target_pte_addr =
                Param::linear_phys_to_virt(target_frame.start().add(idx * Param::PAGE_ENTRY_SIZE));
            pte.borrow_memory(PageTableEntry::new(target_pte_addr.as_mut_ptr()), allocator)?;
        }

        Ok(Self::new(target_frame))
    }

    unsafe fn entry_iter(&self) -> impl Iterator<Item = PageTableEntry<Param>> + '_ {
        (0..Param::PTE_COUNT)
            .map(move |idx| self.get_entry_unchecked(idx))
            .filter(PageTableEntry::is_valid)
    }

    // Get the virtual address of the specified page table entry
    #[inline(always)]
    unsafe fn entry_virt_unchecked(&self, idx: usize) -> VirtualAddress {
        // The virtual addresses of page tables
        // and page table entries are linearly mapped from their physical addresses
        Param::linear_phys_to_virt(self.frame.start().add(idx * Param::PAGE_ENTRY_SIZE))
    }
}

impl<Param: PageParam> Clone for PageTable<Param> {
    fn clone(&self) -> Self {
        Self::new(self.frame.clone())
    }
}

pub struct PageTableEntry<Param> {
    data: *mut usize,
    _maker: PhantomData<Param>,
}

impl<Param> PageTableEntry<Param> {
    fn new(data: *mut usize) -> Self {
        Self {
            data,
            _maker: PhantomData,
        }
    }
}

impl<Param: PageParam> PageTableEntry<Param> {
    pub fn set(&mut self, addr: PhysicalAddress, flags: usize) {
        self.set_data(Param::create_pte(addr, flags | Param::FLAG_PTE_VALID))
    }

    pub fn set_nonleaf(&mut self, addr: PhysicalAddress) {
        self.set_data(Param::create_nonleaf_pte(addr))
    }

    pub fn frame(&self) -> Frame {
        Frame::of_addr(Param::pte_address(self.data()))
    }

    // Get next level page table
    pub fn next_page_table(&self) -> core::result::Result<PageTable<Param>, NextPageError> {
        if !self.is_valid() {
            return Err(NextPageError::Invalid);
        }
        if !self.has_next_table() {
            return Err(NextPageError::NoNext);
        }
        Ok(PageTable::new(self.frame()))
    }

    pub fn free<MutexType, A>(&mut self, allocator: &LockedAllocator<MutexType, A>) -> bool
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
    {
        match self.next_page_table() {
            Ok(mut tab) => tab.free(allocator),
            Err(NextPageError::NoNext) => {
                allocator.dealloc(&self.frame());
            }
            Err(NextPageError::Invalid) => return false,
        }
        self.set_invalid();
        true
    }

    pub fn borrow_memory<MutexType, A>(
        &self,
        mut target: PageTableEntry<Param>,
        allocator: &LockedAllocator<MutexType, A>,
    ) -> Result<()>
    where
        MutexType: lock_api::RawMutex,
        A: Allocator,
    {
        match self.next_page_table() {
            Ok(tab) => {
                let new_tab = tab.borrow_memory(allocator)?;
                target.set_nonleaf(new_tab.frame.start());
                Ok(())
            }
            Err(NextPageError::Invalid) => Err(Error::InvalidPageTable(self.data())),
            Err(NextPageError::NoNext) => {
                target.set_data(Param::pte_borrow(self.data()));
                Ok(())
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        Param::pte_is_valid(self.data())
    }

    fn set_invalid(&mut self) {
        self.set_data(Param::pte_set_invalid(self.data()))
    }

    fn data(&self) -> usize {
        unsafe { *self.data }
    }

    fn set_data(&self, new_data: usize) {
        unsafe { *self.data = new_data }
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.set_data(0)
    }

    fn has_next_table(&self) -> bool {
        Param::pte_has_next_table(self.data())
    }
}
