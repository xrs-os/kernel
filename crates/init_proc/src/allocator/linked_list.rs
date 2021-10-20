use core::alloc::{GlobalAlloc, Layout};

pub use linked_list_allocator::LockedHeap;

static HEAP: LockedHeap = LockedHeap::empty();

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
