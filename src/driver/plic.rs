use super::setup_registry_fn;
use crate::{arch, cpu, mm::PageParamA};
use mm::{page::PageParam, PhysicalAddress};

pub fn init() {
    setup_registry_fn("riscv,plic0", 999, init_plic)
}

pub fn init_plic(node: &device_tree::Node) {
    let addr = node.prop_u64("reg").unwrap() as usize;
    let _phandle = node.prop_u32("phandle").unwrap();
    let plic_base_addr = PageParamA::linear_phys_to_virt(PhysicalAddress(addr));
    arch::plic::init(plic_base_addr, cpu::cpu_id());
}
