//! Alocador de memória baseado em lista vinculada (linked list allocator) para
//! o kernel do DNU.
//!
//! Este allocador suporta alocação e liberação de memória (`malloc`/`free`),
//! permitindo o uso de `Box`, `Vec`, `String` e outros tipos que alocam
//! dinamicamente no `no_std`.
//!
//! O algoritmo:
//! - Mantém uma lista ordenada de blocos livres por endereço
//! - Em `alloc`: busca o primeiro bloco que caiba (first-fit), divide-o se
//!   sobrar espaço suficiente para um novo bloco livre
//! - Em `dealloc`: recoloca o bloco na lista livre e coalesce (une) com
//!   blocos adjacentes para evitar fragmentação externa

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::{null_mut, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// Estruturas internas do allocador
// ============================================================================

/// Cabeçalho de um bloco (livre ou alocado), armazenado no início do bloco.
#[repr(C)]
#[derive(Clone, Copy)]
struct BlockHeader {
    size: usize,
    next: Option<NonNull<BlockHeader>>,
}

impl BlockHeader {
    const fn free(size: usize, next: Option<NonNull<BlockHeader>>) -> Self {
        Self { size, next }
    }

    fn data_ptr(&self) -> *mut u8 {
        unsafe { (self as *const BlockHeader as *mut u8).add(size_of::<BlockHeader>()) }
    }

    unsafe fn from_data_ptr(ptr: *mut u8) -> *mut BlockHeader {
        (ptr as *mut u8).sub(size_of::<BlockHeader>()) as *mut BlockHeader
    }

    fn data_size(&self) -> usize {
        self.size - size_of::<BlockHeader>()
    }

    fn align_size(size: usize) -> usize {
        let header_size = size_of::<BlockHeader>();
        let total = header_size + size;
        let align = core::mem::align_of::<BlockHeader>();
        (total + align - 1) & !(align - 1)
    }
}

// ============================================================================
// O allocador
// ============================================================================

pub struct LinkedListAllocator {
    head: UnsafeCell<Option<NonNull<BlockHeader>>>,
    total_size: UnsafeCell<usize>,
    allocated_bytes: AtomicUsize,
}

unsafe impl Sync for LinkedListAllocator {}

impl LinkedListAllocator {
    const fn new() -> Self {
        Self {
            head: UnsafeCell::new(None),
            total_size: UnsafeCell::new(0),
            allocated_bytes: AtomicUsize::new(0),
        }
    }

    pub unsafe fn init(&mut self, start: *mut u8, size: usize) {
        let start = start as usize;
        let end = start + size;

        let align = core::mem::align_of::<BlockHeader>();
        let start = (start + align - 1) & !(align - 1);
        let end = end & !(align - 1);

        if end <= start + size_of::<BlockHeader>() {
            return;
        }

        let usable_size = end - start;
        let header = BlockHeader::free(usable_size, None);
        let header_ptr = start as *mut BlockHeader;
        unsafe {
            core::ptr::write(header_ptr, header);
        }

        *self.head.get() = Some(NonNull::new(header_ptr).unwrap());
        *self.total_size.get() = usable_size;
    }

    pub unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = BlockHeader::align_size(layout.size());

        let mut prev: Option<NonNull<BlockHeader>> = None;
        let mut curr = unsafe { *self.head.get() };

        while let Some(node) = curr {
            let node_ref = unsafe { node.as_ref() };

            if node_ref.size >= size {
                let remaining = node_ref.size - size;
                if remaining >= size_of::<BlockHeader>() {
                    let new_node_ptr = node.as_ptr().add(size) as *mut BlockHeader;
                    unsafe {
                        core::ptr::write(
                            new_node_ptr,
                            BlockHeader::free(remaining, node_ref.next),
                        );
                    }

                    let mut header = node_ref.clone();
                    header.size = size;
                    header.next = None;

                    unsafe {
                        core::ptr::write(node.as_ptr(), header);
                    }

                    if let Some(mut prev_node) = prev {
                        unsafe {
                            prev_node.as_mut().next = Some(NonNull::new(new_node_ptr).unwrap());
                        }
                    } else {
                        unsafe {
                            *self.head.get() = Some(NonNull::new(new_node_ptr).unwrap());
                        }
                    }

                    self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
                    return node_ref.data_ptr();
                } else {
                    if let Some(mut prev_node) = prev {
                        unsafe {
                            prev_node.as_mut().next = node_ref.next;
                        }
                    } else {
                        unsafe {
                            *self.head.get() = node_ref.next;
                        }
                    }

                    self.allocated_bytes.fetch_add(node_ref.size, Ordering::Relaxed);
                    return node_ref.data_ptr();
                }
            }

            prev = Some(node);
            curr = node_ref.next;
        }

        null_mut()
    }

    pub unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let size = BlockHeader::align_size(layout.size());
        let header_ptr = BlockHeader::from_data_ptr(ptr);

        unsafe {
            core::ptr::write(header_ptr, BlockHeader::free(size, None));
        }

        let mut prev: Option<NonNull<BlockHeader>> = None;
        let mut curr = unsafe { *self.head.get() };
        let new_ptr = NonNull::new(header_ptr).unwrap();

        while let Some(node) = curr {
            if node.as_ptr() > header_ptr {
                break;
            }
            prev = Some(node);
            curr = unsafe { node.as_ref().next };
        }

        if let Some(mut prev_node) = prev {
            let prev_end = unsafe {
                prev_node.as_ptr().add(prev_node.as_ref().size) as *mut u8
            };
            if prev_end == header_ptr as *mut u8 {
                unsafe {
                    prev_node.as_mut().size += size;
                    prev_node.as_mut().next = (*header_ptr).next;
                }
                self.allocated_bytes.fetch_sub(size, Ordering::Relaxed);
                return;
            }
        }

        if let Some(next_node) = curr {
            let new_end = header_ptr as *mut u8 as usize + size;
            let next_start = next_node.as_ptr() as *mut u8;
            if new_end == next_start as usize {
                unsafe {
                    (*header_ptr).size += next_node.as_ref().size;
                    (*header_ptr).next = next_node.as_ref().next;
                }
                self.allocated_bytes.fetch_sub(size, Ordering::Relaxed);
                return;
            }
        }

        unsafe {
            (*header_ptr).next = curr;
        }
        if let Some(mut prev_node) = prev {
            unsafe {
                prev_node.as_mut().next = Some(new_ptr);
            }
        } else {
            unsafe {
                *self.head.get() = Some(new_ptr);
            }
        }

        self.allocated_bytes.fetch_sub(size, Ordering::Relaxed);
    }

    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes.load(Ordering::Relaxed)
    }

    pub fn total_size(&self) -> usize {
        unsafe { *self.total_size.get() }
    }
}

// ============================================================================
// Instância global do allocador
// ============================================================================

const HEAP_SIZE: usize = 1024 * 1024;

#[repr(align(4096))]
struct HeapSpace(UnsafeCell<[u8; HEAP_SIZE]>);

unsafe impl Sync for HeapSpace {}

static HEAP_SPACE: HeapSpace = HeapSpace(UnsafeCell::new([0; HEAP_SIZE]));

// Única instância global do allocador. Usada como #[global_allocator] e
// inicializada via allocator_init() (usando cast de ponteiro para obter
// acesso mutável durante a inicialização de boot).
#[global_allocator]
static GLOBAL_ALLOC: LinkedListAllocator = LinkedListAllocator {
    head: UnsafeCell::new(None),
    total_size: UnsafeCell::new(0),
    allocated_bytes: AtomicUsize::new(0),
};

// ============================================================================
// Inicialização
// ============================================================================

/// Inicializa o alocador global com o heap estático.
///
/// Deve ser chamado exatamente uma vez durante o boot do kernel, antes de
/// qualquer alocação dinâmica.
pub unsafe fn allocator_init() {
    unsafe {
        // Obtém acesso mutável ao #[global_allocator] via cast de ponteiro.
        // Isto é seguro porque allocator_init() é chamado exatamente uma vez,
        // single-thread, antes de qualquer alocação.
        let alloc = &mut *(&GLOBAL_ALLOC as *const LinkedListAllocator as *mut LinkedListAllocator);
        alloc.init(HEAP_SPACE.0.get().cast::<u8>(), HEAP_SIZE);
    }
}

// ============================================================================
// Implementação da trait GlobalAlloc
// ============================================================================

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            core::ptr::write_bytes(ptr, 0, layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(layout);
        }
        if new_size == 0 {
            self.dealloc(ptr, layout);
            return null_mut();
        }

        let header_ptr = BlockHeader::from_data_ptr(ptr);
        let old_header = unsafe { core::ptr::read(header_ptr) };
        let old_size = old_header.data_size();

        if new_size <= old_size {
            return ptr;
        }

        let new_layout = Layout::from_size_align(new_size, layout.align())
            .expect("layout inválido");
        let new_ptr = self.alloc(new_layout);
        if new_ptr.is_null() {
            return null_mut();
        }

        unsafe {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, old_size);
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}

// ============================================================================
// Funções auxiliares públicas
// ============================================================================

pub fn allocated_bytes() -> usize {
    unsafe {
        let alloc = &*(&GLOBAL_ALLOC as *const LinkedListAllocator);
        alloc.allocated_bytes()
    }
}

pub fn total_heap_size() -> usize {
    unsafe {
        let alloc = &*(&GLOBAL_ALLOC as *const LinkedListAllocator);
        alloc.total_size()
    }
}

pub fn free_bytes() -> usize {
    unsafe {
        let alloc = &*(&GLOBAL_ALLOC as *const LinkedListAllocator);
        alloc.total_size().saturating_sub(alloc.allocated_bytes())
    }
}
