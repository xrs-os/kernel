use crate::{page::Flag, PhysicalAddress, VirtualAddress};
use riscv::{
    asm::{sfence_vma, sfence_vma_all},
    register::satp,
};

// Linear mapping
#[cfg(target_arch = "riscv32")]
const LINEAR_MAPPING_PHYS_OFFSET: usize = 0x0000_0000;
#[cfg(target_arch = "riscv64")]
const LINEAR_MAPPING_PHYS_OFFSET: usize = 0xFFFF_FFFF_0000_0000;

pub type PageParam = PageParamSv39;
pub struct PageParamSv39;

impl crate::page::PageParam for PageParamSv39 {
    const FLAG_PTE_READABLE: Flag = 1 << 1;

    const FLAG_PTE_WRITEABLE: Flag = 1 << 2;

    const FLAG_PTE_EXECUTABLE: Flag = 1 << 3;

    const FLAG_PTE_ACCESSED: Flag = 1 << 6;

    const FLAG_PTE_DIRTY: Flag = 1 << 7;

    const FLAG_PTE_VALID: Flag = 1 << 0;

    const PAGE_SIZE_SHIFT: usize = 12;

    const PTE_COUNT: usize = 512;

    const PAGE_LEVELS: usize = 3;

    const LINEAR_MAPPING_PHYS_OFFSET: usize = LINEAR_MAPPING_PHYS_OFFSET;

    #[inline(always)]
    unsafe fn flush_tlb(asid: Option<usize>, addr: Option<VirtualAddress>) {
        if let (None, None) = (asid, addr) {
            sfence_vma_all();
        } else {
            sfence_vma(asid.unwrap_or(0), addr.map(|addr| addr.0).unwrap_or(0));
        }
    }

    #[inline(always)]
    unsafe fn activate_root_table(root_table_addr: PhysicalAddress, asid: Option<usize>) {
        satp::write((8 << 60) | asid.unwrap_or(0) | (root_table_addr.0 >> 12))
    }

    #[inline(always)]
    fn flag_set_user(flags: Flag) -> Flag {
        flags | (1 << 4)
    }

    #[inline(always)]
    fn flag_set_kernel(flags: Flag) -> Flag {
        flags & (!(1 << 4))
    }

    #[inline(always)]
    fn create_pte(addr: PhysicalAddress, flags: Flag) -> usize {
        ((addr.0 >> 2) & 0x3F_FFFF_FFFF_FC00) | flags
    }

    #[inline(always)]
    fn create_nonleaf_pte(addr: PhysicalAddress) -> usize {
        ((addr.0 >> 2) & 0x3F_FFFF_FFFF_FC00) | Self::FLAG_PTE_VALID
    }

    #[inline(always)]
    fn pte_is_kernel(pte: usize) -> bool {
        (pte & (1 << 4)) == 0
    }

    #[inline(always)]
    fn pte_address(pte: usize) -> PhysicalAddress {
        ((pte & 0x3F_FFFF_FFFF_FC00) << 2).into()
    }

    #[inline(always)]
    fn pte_has_next_table(pte: usize) -> bool {
        pte & (Self::FLAG_PTE_READABLE | Self::FLAG_PTE_WRITEABLE | Self::FLAG_PTE_EXECUTABLE) == 0
    }

    #[inline(always)]
    fn pte_idxs(va: VirtualAddress) -> [usize; Self::PAGE_LEVELS] {
        [
            (va.0 & 0x7F_C000_0000) >> 30, // level 1
            (va.0 & 0x3FE0_0000) >> 21,    // level 2
            (va.0 & 0x1F_F000) >> 12,      // level 3
        ]
    }
}
