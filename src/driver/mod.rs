use core::slice;

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BinaryHeap},
    str,
    sync::Arc,
    vec::Vec,
};

use crate::{fs::blk, spinlock::RwLockIrq};

mod plic;
mod virtio_blk;
mod virtio_mmio;

const DEVICE_TREE_MAGIC: u32 = 0xd00dfeed;

static mut DRIVER_IRQ_ACK_FNS: BTreeMap<u32, Box<dyn Fn()>> = BTreeMap::new();

static mut BLK_DRIVERS: Vec<Arc<dyn blk::BlkDevice>> = Vec::new();

/// Compatible lookup
#[allow(clippy::type_complexity)]
static DEVICE_TREE_REGISTRY: RwLockIrq<BTreeMap<&'static str, (isize, fn(&device_tree::Node))>> =
    RwLockIrq::new(BTreeMap::new());

pub fn driver_irq_ack_fn(irq_num: &u32) -> Option<&dyn Fn()> {
    unsafe { DRIVER_IRQ_ACK_FNS.get(irq_num).map(AsRef::as_ref) }
}

pub fn set_driver_irq_ack_fn(irq_num: u32, ack_fn: Box<dyn Fn()>) {
    unsafe {
        DRIVER_IRQ_ACK_FNS.insert(irq_num, ack_fn);
    }
}

pub fn blk_drivers() -> &'static Vec<Arc<dyn blk::BlkDevice>> {
    unsafe { &BLK_DRIVERS }
}

pub fn add_blk_drivers(blk_driver: Arc<dyn blk::BlkDevice>) {
    unsafe { BLK_DRIVERS.push(blk_driver) };
}

#[allow(clippy::type_complexity)]
pub fn device_tree_registry()
-> &'static RwLockIrq<BTreeMap<&'static str, (isize, fn(&device_tree::Node))>> {
    &DEVICE_TREE_REGISTRY
}

pub fn setup_registry_fn(driver_name: &'static str, priority: isize, f: fn(&device_tree::Node)) {
    device_tree_registry()
        .write()
        .insert(driver_name, (priority, f));
}

struct DriverRegister<'a> {
    priority: isize,
    f: fn(&device_tree::Node),
    node: &'a device_tree::Node,
}

impl PartialEq for DriverRegister<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for DriverRegister<'_> {}

impl PartialOrd for DriverRegister<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.priority.partial_cmp(&other.priority)
    }
}

impl Ord for DriverRegister<'_> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

fn walk_dt_node<'a>(
    node: &'a device_tree::Node,
    driver_registers: &mut BinaryHeap<DriverRegister<'a>>,
) {
    if let Some(compatible) = node.prop_raw("compatible") {
        let registry = device_tree_registry().read();
        for driver_name in compatible.split(|&x| x == 0) {
            if driver_name.is_empty() {
                continue;
            }
            if let Ok(driver_name) = str::from_utf8(driver_name) {
                if let Some(&(priority, f)) = registry.get(driver_name) {
                    driver_registers.push(DriverRegister { priority, f, node });
                }
            }
        }
    }

    for child in node.children.iter() {
        walk_dt_node(child, driver_registers);
    }
}

struct DtbHeader {
    magic: u32,
    size: u32,
}

pub fn init(dtb: usize) {
    plic::init();
    virtio_mmio::init();

    let header = unsafe { &*(dtb as *const DtbHeader) };
    let magic = u32::from_be(header.magic);

    if magic == DEVICE_TREE_MAGIC {
        let size = u32::from_be(header.size);
        let dtb_data = unsafe { slice::from_raw_parts(dtb as *const u8, size as usize) };
        if let Ok(dt) = device_tree::DeviceTree::load(dtb_data) {
            let mut driver_registers = BinaryHeap::new();
            walk_dt_node(&dt.root, &mut driver_registers);
            for driver_register in driver_registers {
                (driver_register.f)(driver_register.node);
            }
        }
    }
}
