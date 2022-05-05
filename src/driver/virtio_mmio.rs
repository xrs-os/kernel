use super::setup_registry_fn;
use crate::{
    arch,
    driver::{add_blk_drivers, virtio_blk},
    mm::{frame_allocator, PageParamA},
};
use alloc::{boxed::Box, sync::Arc};
use device_tree::util::SliceRead;
use mm::{page::PageParam, Addr, PhysicalAddress, VirtualAddress};

pub fn init() {
    setup_registry_fn("virtio,mmio", -999, virtio_probe);
}

/// Detects a specific type of virtio protocol from a node in the device tree
pub fn virtio_probe(node: &device_tree::Node) {
    let reg = match node.prop_raw("reg") {
        Some(reg) => reg,
        _ => return,
    };
    let pa = PhysicalAddress(reg.as_slice().read_be_u64(0).unwrap() as usize);
    let va = PageParamA::linear_phys_to_kvirt(pa);
    let header = unsafe { &mut *(va.0 as *mut virtio_drivers::VirtIOHeader) };
    if !header.verify() {
        return;
    }

    if let (Ok(irq), Ok(intc)) = (
        node.prop_u32("interrupts"),
        node.prop_u32("interrupt-parent"),
    ) {
        match header.device_type() {
            virtio_drivers::DeviceType::Block => match virtio_blk::VirtioBlk::new(header) {
                Ok(virt_blk) => {
                    let virt_blk = Arc::new(virt_blk);
                    add_blk_drivers(virt_blk.clone());
                    unsafe {
                        arch::interrupt::register_external_irq(
                            intc,
                            irq,
                            Box::new(move || {
                                let _ = virt_blk.handle_interrupt();
                            }),
                        )
                    }
                }
                Err(e) => panic!("Failed to create VirtioBlk. err: {:?}", e),
            },
            device => println!("unrecognized virtio device: {:?}", device),
        };
    }
}

#[no_mangle]
extern "C" fn virtio_dma_alloc(pages: usize) -> usize {
    let frames = frame_allocator().alloc_consecutive(pages);
    frames
        .first()
        .map(|f| f.start().inner())
        .unwrap_or_default()
}

#[no_mangle]
extern "C" fn virtio_dma_dealloc(paddr: usize, pages: usize) -> i32 {
    for page in 0..pages {
        frame_allocator()
            .dealloc(&PhysicalAddress::new((paddr + page) << PageParamA::PAGE_SIZE_SHIFT).into());
    }
    0
}

#[no_mangle]
extern "C" fn virtio_phys_to_virt(paddr: usize) -> usize {
    PageParamA::linear_phys_to_kvirt(PhysicalAddress::new(paddr)).inner()
}

#[no_mangle]
extern "C" fn virtio_virt_to_phys(vaddr: usize) -> usize {
    PageParamA::linear_kvirt_to_phys(VirtualAddress::new(vaddr)).inner()
}
