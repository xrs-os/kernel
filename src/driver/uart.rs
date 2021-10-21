use crate::arch::{getchar, interrupt::register_external_irq};
use crate::mm::PageParamA;
use alloc::boxed::Box;
use core::ptr;
use mm::PhysicalAddress;
use mm::{page::PageParam, Addr};

use super::setup_registry_fn;

const UART_INT_EN_OFFSET: usize = 1;
const UART_MODEM_CONTROL_OFFSET: usize = 4;

pub fn init() {
    setup_registry_fn("ns16550a", -999, init_uart)
}

pub fn init_uart(node: &device_tree::Node) {
    let addr = node.prop_usize("reg").unwrap();
    if let (Ok(irq), Ok(intc)) = (
        node.prop_u32("interrupts"),
        node.prop_u32("interrupt-parent"),
    ) {
        unsafe {
            register_external_irq(
                intc,
                irq,
                Box::new(|| {
                    println!("getchar: {}", getchar());
                }),
            );
            let uart_base = PageParamA::linear_phys_to_virt(PhysicalAddress(addr));
            ptr::write_volatile(uart_base.add(UART_INT_EN_OFFSET).as_mut_ptr(), 0x01);
            ptr::write_volatile(uart_base.add(UART_MODEM_CONTROL_OFFSET).as_mut_ptr(), 0x0b);
        }
    }
}
