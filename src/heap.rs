use core::alloc::{GlobalAlloc, Layout};

/// Heap size used by the kernel to dynamically allocate memory（8M）
pub const KERNEL_HEAP_SIZE: usize = 0x80_0000;

#[link_section = ".bss"]
static mut HEAP_SPACE: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];

static HEAP: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

pub struct Allocator;
impl Allocator {
    pub fn init_heap(heap_start: usize, heap_size: usize) {
        unsafe {
            HEAP.lock().init(heap_start, heap_size);
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP.dealloc(ptr, layout)
    }
}

pub fn init() {
    unsafe {
        Allocator::init_heap(HEAP_SPACE.as_ptr() as usize, KERNEL_HEAP_SIZE);
    }
}
