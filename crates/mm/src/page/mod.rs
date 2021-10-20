pub mod flush;
pub mod mapper;
pub mod table;

use super::{frame, Frame, Page, PhysicalAddress, VirtualAddress};

pub type Flag = usize;

pub trait PageParam {
    // Page readable flag bit
    const FLAG_PTE_READABLE: Flag;
    // Page writable flag bit
    const FLAG_PTE_WRITEABLE: Flag;
    // Page executable flag bit
    const FLAG_PTE_EXECUTABLE: Flag;
    // Page visited flag bit
    const FLAG_PTE_ACCESSED: Flag;
    // Page has written flag bit
    const FLAG_PTE_DIRTY: Flag;
    // Page valid flag bit
    const FLAG_PTE_VALID: Flag;

    // Number of page table levels
    const PAGE_LEVELS: usize;

    // page size shift
    const PAGE_SIZE_SHIFT: usize;
    // Page size (in bytes)
    const PAGE_SIZE: usize = 1 << Self::PAGE_SIZE_SHIFT;
    // Total number of page table entries in a page table
    const PTE_COUNT: usize;

    // Page table entry size (in bytes)
    const PAGE_ENTRY_SIZE: usize = Self::PAGE_SIZE / Self::PTE_COUNT;

    // Linear mapping of physical address offsets
    const LINEAR_MAPPING_PHYS_OFFSET: usize;

    /// # Safety
    /// flush tlb
    unsafe fn flush_tlb(asid: Option<usize>, addr: Option<VirtualAddress>);

    /// # Safety
    /// activate page table
    unsafe fn activate_root_table(root_table_addr: PhysicalAddress, asid: Option<usize>);

    // Create page table entry data
    fn create_pte(addr: PhysicalAddress, flags: Flag) -> usize;

    // Create page table entry data that points to the next level of page tables
    fn create_nonleaf_pte(addr: PhysicalAddress) -> usize;

    // Set the pte user flag bit
    fn flag_set_user(flags: Flag) -> Flag;
    // Set the pte kernel flag bit
    fn flag_set_kernel(flags: Flag) -> Flag;

    #[inline(always)]
    fn pte_readable(pte: usize) -> bool {
        (pte & Self::FLAG_PTE_READABLE) == Self::FLAG_PTE_READABLE
    }

    #[inline(always)]
    fn pte_writeable(pte: usize) -> bool {
        (pte & Self::FLAG_PTE_WRITEABLE) == Self::FLAG_PTE_WRITEABLE
    }

    #[inline(always)]
    fn pte_executable(pte: usize) -> bool {
        (pte & Self::FLAG_PTE_EXECUTABLE) == Self::FLAG_PTE_EXECUTABLE
    }

    #[inline(always)]
    fn pte_accessed(pte: usize) -> bool {
        (pte & Self::FLAG_PTE_ACCESSED) == Self::FLAG_PTE_ACCESSED
    }

    #[inline(always)]
    fn pte_is_valid(pte: usize) -> bool {
        (pte & Self::FLAG_PTE_VALID) == Self::FLAG_PTE_VALID
    }

    #[inline(always)]
    fn pte_set_invalid(pte: usize) -> usize {
        pte & (!Self::FLAG_PTE_VALID)
    }

    fn pte_address(pte: usize) -> PhysicalAddress;

    // `pte` existence of next level page table
    fn pte_has_next_table(pte: usize) -> bool;

    // Get the index of each page table entry at each level in `va`
    fn pte_idxs(va: VirtualAddress) -> [usize; Self::PAGE_LEVELS];

    /// Copy `pte` and make it unwritable
    fn pte_borrow(pte: usize) -> usize {
        pte & (!Self::FLAG_PTE_WRITEABLE)
    }

    // Linear mapping of physical addresses to virtual addresses
    fn linear_phys_to_virt(pa: PhysicalAddress) -> VirtualAddress {
        VirtualAddress(pa.0 + Self::LINEAR_MAPPING_PHYS_OFFSET)
    }

    // Virtual address to physical address for linear mapping
    fn linear_virt_to_phys(va: VirtualAddress) -> PhysicalAddress {
        PhysicalAddress(va.0 - Self::LINEAR_MAPPING_PHYS_OFFSET)
    }
}
