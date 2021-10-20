mod linked_list;

#[cfg(not(feature = "slab"))]
pub use linked_list::Allocator;

pub fn init_heap() {
    const HEAP_SIZE: usize = 0x1024;
    static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
    Allocator::init_heap(unsafe { HEAP.as_ptr() as usize }, HEAP_SIZE);
}
