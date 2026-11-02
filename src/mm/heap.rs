use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

const HEAP_SIZE: usize = 1024 * 1024;

#[repr(align(4096))]
struct HeapSpace(UnsafeCell<[u8; HEAP_SIZE]>);
unsafe impl Sync for HeapSpace {}

struct BumpAllocator {
    offset: AtomicUsize,
}

unsafe impl Sync for BumpAllocator {}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = HEAP.0.get().cast::<u8>() as usize;
        let mut current = self.offset.load(Ordering::Relaxed);
        loop {
            let aligned = match (base + current).checked_add(layout.align() - 1) {
                Some(value) => value & !(layout.align() - 1),
                None => return core::ptr::null_mut(),
            };
            let next = match aligned.checked_add(layout.size()) {
                Some(value) => value - base,
                None => return core::ptr::null_mut(),
            };
            if next > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match self.offset.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(observed) => current = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

static HEAP: HeapSpace = HeapSpace(UnsafeCell::new([0; HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator {
    offset: AtomicUsize::new(0),
};
