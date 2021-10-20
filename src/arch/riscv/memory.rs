use alloc::vec::Vec;
use mm::{
    arch::page::PageParam as PageParamA,
    memory::{MapType, Segment},
    page::PageParam as _,
    PhysicalAddress, VirtualAddress,
};

use super::consts;

// Symbols exported in the linker script
#[allow(dead_code)]
extern "C" {
    fn kernel_start();
    fn text_start();
    fn rodata_start();
    fn data_start();
    fn bss_start();
    fn kernel_end();
}

pub fn memory_range() -> (PhysicalAddress, PhysicalAddress) {
    let start = PageParamA::linear_virt_to_phys(VirtualAddress(kernel_end as usize));
    let end = consts::MEMORY_END_ADDRESS;
    (start, end)
}

pub const fn user_stack_offset() -> usize {
    consts::USER_STACK_OFFSET
}

pub const fn user_stack_size() -> usize {
    consts::USER_STACK_SIZE
}

pub fn kernel_segments() -> Vec<Segment> {
    vec![
        // .text segment, -x
        Segment {
            addr_range: VirtualAddress(text_start as usize)..VirtualAddress(rodata_start as usize),
            flags: PageParamA::flag_set_kernel(
                PageParamA::FLAG_PTE_READABLE | PageParamA::FLAG_PTE_EXECUTABLE,
            ),
            map_type: MapType::Linear,
        },
        // .rodata segment, r--
        Segment {
            addr_range: VirtualAddress(rodata_start as usize)..VirtualAddress(data_start as usize),
            flags: PageParamA::flag_set_kernel(PageParamA::FLAG_PTE_READABLE),
            map_type: MapType::Linear,
        },
        // .data segment, rw-
        Segment {
            addr_range: VirtualAddress(data_start as usize)..VirtualAddress(bss_start as usize),
            flags: PageParamA::flag_set_kernel(
                PageParamA::FLAG_PTE_READABLE | PageParamA::FLAG_PTE_WRITEABLE,
            ),
            map_type: MapType::Linear,
        },
        // .bss segment, rw-
        Segment {
            addr_range: VirtualAddress(bss_start as usize)..VirtualAddress(kernel_end as usize),
            flags: PageParamA::flag_set_kernel(
                PageParamA::FLAG_PTE_READABLE | PageParamA::FLAG_PTE_WRITEABLE,
            ),
            map_type: MapType::Linear,
        },
        // remaining memory space，rw-
        Segment {
            addr_range: VirtualAddress(kernel_end as usize)
                ..PageParamA::linear_phys_to_virt(consts::MEMORY_END_ADDRESS),
            flags: PageParamA::flag_set_kernel(
                PageParamA::FLAG_PTE_READABLE | PageParamA::FLAG_PTE_WRITEABLE,
            ),
            map_type: MapType::Linear,
        },
    ]
}