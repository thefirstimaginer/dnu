use crate::memory;
use core::arch::asm;
use core::ptr;

const PAGE_SIZE: u64 = 4096;
const ENTRY_PRESENT: u64 = 1;
const ENTRY_WRITABLE: u64 = 1 << 1;
const ENTRY_USER: u64 = 1 << 2;
const ENTRY_HUGE: u64 = 1 << 7;
const ENTRY_NX: u64 = 1 << 63;
const PHYS_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const ENTRY_VALID_MASK: u64 = 0x800f_ffff_ffff_f0ff;
const ADDRESS_MASK: u64 = PHYS_ADDR_MASK;
/// Bits reservados (52-62) em entradas de tabela de páginas — devem ser zero.
const RSVD_MASK: u64 = 0x7ff0_0000_0000_0000;
const USER_MIN: u64 = 0x400000;
const USER_MAX: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy)]
pub struct AddressSpace {
    pub root_physical: u64,
    pub allocator: *mut memory::FrameAllocator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    InvalidAddress,
    OutOfMemory,
    AlreadyMapped,
    PartiallyMapped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFlags(u8);

impl PageFlags {
    pub const READ_ONLY: Self = Self(0);
    pub const READ_WRITE: Self = Self(1);
    pub const READ_EXEC: Self = Self(2);

    pub const fn is_writable(self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn is_executable(self) -> bool {
        self.0 & 2 != 0
    }

    const fn entry_bits(self) -> u64 {
        let mut bits = ENTRY_PRESENT;
        if self.is_writable() {
            bits |= ENTRY_WRITABLE;
        }
        if !self.is_executable() {
            bits |= ENTRY_NX;
        }
        bits
    }
}

impl AddressSpace {
    pub fn new_user(allocator: &mut memory::FrameAllocator) -> Option<Self> {
        let current_root = read_cr3() & ADDRESS_MASK;
        let root = memory::allocate_frame(allocator)?;
        unsafe {
            ptr::write_bytes(root.virtual_address, 0, PAGE_SIZE as usize);
            let source = memory::phys_to_virt(allocator, current_root)?.cast::<u64>();
            let destination = root.virtual_address.cast::<u64>();
            ptr::copy_nonoverlapping(source.add(256), destination.add(256), 256);
        }
        Some(Self {
            root_physical: root.physical_address,
            allocator,
        })
    }

    pub fn map_user_page(&self, virtual_address: u64) -> Result<*mut u8, MapError> {
        self.map_user_page_with(virtual_address, PageFlags::READ_ONLY)
    }

    pub fn map_user_page_writable(
        &self,
        virtual_address: u64,
    ) -> Result<*mut u8, MapError> {
        self.map_user_page_with(virtual_address, PageFlags::READ_WRITE)
    }

    pub fn map_user_page_with(
        &self,
        virtual_address: u64,
        flags: PageFlags,
    ) -> Result<*mut u8, MapError> {
        if !(USER_MIN..USER_MAX).contains(&virtual_address) {
            return Err(MapError::InvalidAddress);
        }
        let page_address = virtual_address & !(PAGE_SIZE - 1);
        let indices = [
            ((page_address >> 39) & 0x1ff) as usize,
            ((page_address >> 30) & 0x1ff) as usize,
            ((page_address >> 21) & 0x1ff) as usize,
            ((page_address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = self.root_physical;
        for index in indices.iter().take(3) {
            let table = table_pointer(table_physical).ok_or(MapError::OutOfMemory)?;
            let entry = unsafe { &mut *table.add(*index) };
            if *entry & ENTRY_PRESENT == 0 {
                let frame = unsafe { memory::allocate_frame(&mut *self.allocator) }.ok_or(MapError::OutOfMemory)?;
                unsafe {
                    ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
                }
                *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE | ENTRY_USER;
            } else {
                *entry |= ENTRY_WRITABLE | ENTRY_USER;
            }
            table_physical = *entry & ADDRESS_MASK;
        }

        let table = table_pointer(table_physical).ok_or(MapError::OutOfMemory)?;
        let entry = unsafe { &mut *table.add(indices[3]) };
        if *entry & ENTRY_PRESENT != 0 {
            // Merge permissions: preserve EXECUTE if already set, add NX only
            // if the page is not already executable.
            let mut new_bits = *entry & !ENTRY_WRITABLE;
            new_bits |= flags.entry_bits();
            // If page was already executable, don't add NX (shared page with code).
            if *entry & ENTRY_NX == 0 {
                new_bits &= !ENTRY_NX;
            }
            *entry = new_bits;
            return Ok(memory::phys_to_virt(unsafe { &*self.allocator }, *entry & ADDRESS_MASK)
                .unwrap_or(core::ptr::null_mut()));
        }
        let frame = unsafe { memory::allocate_frame(&mut *self.allocator) }.ok_or(MapError::OutOfMemory)?;
        unsafe {
            ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
        }
        *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_USER | flags.entry_bits();
        Ok(frame.virtual_address)
    }

    pub fn try_map_range(
        &self,
        start: u64,
        length: u64,
        flags: PageFlags,
    ) -> Result<(), MapError> {
        if length == 0 {
            return Err(MapError::InvalidAddress);
        }
        let Some(end) = start.checked_add(length) else {
            return Err(MapError::InvalidAddress);
        };
        let first = start & !(PAGE_SIZE - 1);
        let last = (end - 1) & !(PAGE_SIZE - 1);
        if first < USER_MIN || last >= USER_MAX {
            return Err(MapError::InvalidAddress);
        }

        let mut page = first;
        while page <= last {
            if self.translate(page).is_some() {
                return Err(MapError::AlreadyMapped);
            }
            page = page.checked_add(PAGE_SIZE).ok_or(MapError::InvalidAddress)?;
        }

        let mut mapped = first;
        page = first;
        while page <= last {
            match self.map_user_page_with(page, flags) {
                Ok(_) => {
                    mapped = page.checked_add(PAGE_SIZE).ok_or(MapError::InvalidAddress)?;
                }
                Err(MapError::AlreadyMapped) => {
                    self.unmap_range_best_effort(first, mapped);
                    return Err(MapError::AlreadyMapped);
                }
                Err(error) => {
                    self.unmap_range_best_effort(first, mapped);
                    return Err(error);
                }
            }
            page = page.checked_add(PAGE_SIZE).ok_or(MapError::InvalidAddress)?;
        }
        Ok(())
    }

    fn unmap_range_best_effort(&self, from: u64, to: u64) {
        let mut page = from;
        while page < to {
            self.unmap_user_page(page);
            page = page.saturating_add(PAGE_SIZE);
        }
    }

    pub fn unmap_user_page(&self, virtual_address: u64) -> bool {
        if !(USER_MIN..USER_MAX).contains(&virtual_address) {
            return false;
        }
        let page_address = virtual_address & !(PAGE_SIZE - 1);
        let indices = [
            ((page_address >> 39) & 0x1ff) as usize,
            ((page_address >> 30) & 0x1ff) as usize,
            ((page_address >> 21) & 0x1ff) as usize,
            ((page_address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = self.root_physical;
        for index in indices.iter().take(3) {
            let Some(table) = table_pointer(table_physical) else {
                return false;
            };
            let entry = unsafe { &mut *table.add(*index) };
            if *entry & ENTRY_PRESENT == 0 {
                return false;
            }
            table_physical = *entry & ADDRESS_MASK;
        }
        let Some(table) = table_pointer(table_physical) else {
            return false;
        };
        let entry = unsafe { &mut *table.add(indices[3]) };
        if *entry & ENTRY_PRESENT == 0 {
            return false;
        }
        let physical_address = *entry & ADDRESS_MASK;
        *entry = 0;
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) page_address, options(nostack, preserves_flags));
        }
        unsafe { memory::free_frame(&mut *self.allocator, physical_address) };
        true
    }

    pub fn clone_user(&self) -> Option<Self> {
        let clone = Self::new_user(unsafe { &mut *self.allocator })?;
        if self.copy_user_pages(&clone) == 0 {
            return None;
        }
        Some(clone)
    }

    fn copy_user_pages(&self, destination: &Self) -> usize {
        let mut copied = 0usize;
        for pml4_index in 0..256usize {
            copied += self.copy_table(
                self.root_physical,
                destination.root_physical,
                pml4_index,
                4,
            );
        }
        copied
    }

    fn copy_table(
        &self,
        source_physical: u64,
        destination_physical: u64,
        index: usize,
        level: u8,
    ) -> usize {
        let Some(source_table) = table_pointer(source_physical) else {
            return 0;
        };
        let source_entry = unsafe { *source_table.add(index) };
        if source_entry & ENTRY_PRESENT == 0 {
            return 0;
        }

        let Some(destination_table) = table_pointer(destination_physical) else {
            return 0;
        };
        let destination_entry = unsafe { &mut *destination_table.add(index) };

        if level == 1 {
            let Some(frame) = (unsafe { memory::allocate_frame(&mut *self.allocator) }) else {
                return 0;
            };
            let Some(source_page) = table_pointer(source_entry & ADDRESS_MASK) else {
                return 0;
            };
            unsafe {
                ptr::copy_nonoverlapping(
                    source_page.cast::<u8>(),
                    frame.virtual_address,
                    PAGE_SIZE as usize,
                );
            }
            *destination_entry = (source_entry & !ADDRESS_MASK) | frame.physical_address;
            return 1;
        }

        if source_entry & ENTRY_HUGE != 0 {
            return 0;
        }

        let next_source = source_entry & ADDRESS_MASK;
        if *destination_entry & ENTRY_PRESENT == 0 {
            let Some(frame) = (unsafe { memory::allocate_frame(&mut *self.allocator) }) else {
                return 0;
            };
            unsafe {
                ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
            }
            *destination_entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE | ENTRY_USER;
        }
        let next_destination = *destination_entry & ADDRESS_MASK;
        let mut copied = 0usize;
        for child in 0..512usize {
            copied += self.copy_table(next_source, next_destination, child, level - 1);
        }
        copied
    }

    pub fn map_framebuffer_range(
        &self,
        virtual_address: u64,
        physical_address: u64,
        length: usize,
        writable: bool,
    ) -> Option<u64> {
        if length == 0 {
            return None;
        }
        let page_offset = physical_address & (PAGE_SIZE - 1);
        let start_physical = physical_address & !(PAGE_SIZE - 1);
        let start_virtual = (virtual_address & !(PAGE_SIZE - 1))
            .checked_add(page_offset)
            .unwrap_or(virtual_address);
        let pages = (page_offset as usize)
            .checked_add(length)?
            .div_ceil(PAGE_SIZE as usize);
        for index in 0..pages {
            let virtual_page = (virtual_address & !(PAGE_SIZE - 1))
                .checked_add((index as u64) * PAGE_SIZE)?;
            let physical_page = start_physical.checked_add((index as u64) * PAGE_SIZE)?;
            let _ = self.map_physical_page(virtual_page, physical_page, writable)?;
        }
        Some(start_virtual)
    }

    pub fn map_framebuffer_readonly(
        &self,
        virtual_address: u64,
        physical_address: u64,
        length: usize,
    ) -> Option<u64> {
        self.map_framebuffer_range(virtual_address, physical_address, length, false)
    }

    fn map_physical_page(
        &self,
        virtual_address: u64,
        physical_address: u64,
        writable: bool,
    ) -> Option<*mut u8> {
        if !(USER_MIN..USER_MAX).contains(&virtual_address) {
            return None;
        }
        let page_address = virtual_address & !(PAGE_SIZE - 1);
        let indices = [
            ((page_address >> 39) & 0x1ff) as usize,
            ((page_address >> 30) & 0x1ff) as usize,
            ((page_address >> 21) & 0x1ff) as usize,
            ((page_address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = self.root_physical;
        for index in indices.iter().take(3) {
            let table = table_pointer(table_physical)?;
            let entry = unsafe { &mut *table.add(*index) };
            if *entry & ENTRY_PRESENT == 0 {
                let frame = unsafe { memory::allocate_frame(&mut *self.allocator) }?;
                unsafe {
                    ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
                }
                *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_USER;
            } else {
                *entry |= ENTRY_USER;
            }
            table_physical = *entry & ADDRESS_MASK;
        }
        let table = table_pointer(table_physical)?;
        let entry = unsafe { &mut *table.add(indices[3]) };
        if physical_address & !PHYS_ADDR_MASK != 0 {
            return None;
        }
        let mut bits = ENTRY_PRESENT | ENTRY_USER | ENTRY_NX;
        if writable {
            bits |= ENTRY_WRITABLE;
        }
        *entry = (physical_address & PHYS_ADDR_MASK) | bits;
        memory::phys_to_virt(unsafe { &*self.allocator }, physical_address)
    }

    pub fn page_is_present(&self, page: u64) -> bool {
        self.translate(page & !(PAGE_SIZE - 1)).is_some()
    }

    pub fn page_is_writable(&self, page: u64) -> bool {
        let Some(entry) = self.page_entry(page & !(PAGE_SIZE - 1)) else {
            return false;
        };
        unsafe { (*entry & ENTRY_PRESENT != 0) && (*entry & ENTRY_WRITABLE != 0) }
    }

    pub fn unmap_page_quietly(&self, page: u64) -> Option<u64> {
        let entry = self.page_entry(page & !(PAGE_SIZE - 1))?;
        let physical = unsafe { *entry & ADDRESS_MASK };
        unsafe {
            if *entry & ENTRY_PRESENT == 0 {
                return None;
            }
            *entry = 0;
            let address = page & !(PAGE_SIZE - 1);
            core::arch::asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags));
        }
        Some(physical)
    }

    pub fn page_entry(&self, page_address: u64) -> Option<*mut u64> {
        if !(USER_MIN..USER_MAX).contains(&page_address) {
            return None;
        }
        let indices = [
            ((page_address >> 39) & 0x1ff) as usize,
            ((page_address >> 30) & 0x1ff) as usize,
            ((page_address >> 21) & 0x1ff) as usize,
            ((page_address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = self.root_physical;
        for index in indices.iter().take(3) {
            let table = table_pointer(table_physical)?;
            let entry = unsafe { *table.add(*index) };
            if entry & ENTRY_PRESENT == 0 {
                return None;
            }
            table_physical = entry & ADDRESS_MASK;
        }
        let table = table_pointer(table_physical)?;
        Some(unsafe { table.add(indices[3]) })
    }

    pub fn translate(&self, virtual_address: u64) -> Option<*mut u8> {
        let indices = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = self.root_physical;
        for index in indices.iter().take(3) {
            let table = table_pointer(table_physical)?;
            let entry = unsafe { *table.add(*index) };
            if entry & ENTRY_PRESENT == 0 || entry & ENTRY_USER == 0 {
                return None;
            }
            table_physical = entry & ADDRESS_MASK;
        }
        let table = table_pointer(table_physical)?;
        let entry = unsafe { *table.add(indices[3]) };
        if entry & ENTRY_PRESENT == 0 || entry & ENTRY_USER == 0 {
            return None;
        }
        let physical = (entry & ADDRESS_MASK) | (virtual_address & (PAGE_SIZE - 1));
        memory::phys_to_virt(unsafe { &*self.allocator }, physical)
    }
}

pub fn switch(space: AddressSpace) {
    unsafe {
        asm!("mov cr3, {}", in(reg) space.root_physical, options(nomem, nostack, preserves_flags));
    }
}

pub fn map_hhdm_range(hhdm_offset: u64, physical_address: u64, length: usize, writable: bool) -> bool {
    let Some(end) = physical_address.checked_add(length as u64) else {
        return false;
    };
    if length == 0 {
        return true;
    }

    let allocator = unsafe {
        let ptr_val = core::ptr::read(core::ptr::addr_of_mut!(crate::ALLOCATOR_PTR));
        if ptr_val.is_null() {
            return false;
        }
        &mut *ptr_val
    };

    const HUGE_PAGE_SIZE: u64 = 0x200000; // 2MB
    const HUGE_PAGE_MASK: u64 = !(HUGE_PAGE_SIZE - 1);

    let mut physical = physical_address & !(HUGE_PAGE_SIZE - 1);
    let end = end.saturating_add(HUGE_PAGE_SIZE - 1) & !(HUGE_PAGE_SIZE - 1);
    while physical < end {
        let Some(virtual_address) = hhdm_offset.checked_add(physical) else {
            return false;
        };
        if !map_hhdm_huge_page(allocator, virtual_address, physical, writable) {
            return false;
        }
        physical += HUGE_PAGE_SIZE;
    }
    true
}

/// Mapeia uma página huge de 2MB usando uma única entrada de PTE com bit HUGE.
/// Isso evita alocar múltiplas páginas de tabela PTE para regiões grandes.
fn map_hhdm_huge_page(allocator: &mut memory::FrameAllocator, virtual_address: u64, physical_address: u64, writable: bool) -> bool {
    let indices = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
    ];
    let mut table_physical = read_cr3() & ADDRESS_MASK;
    for (level, index) in indices.iter().take(2).enumerate() {
        let table = match table_pointer(table_physical) {
            Some(t) => t,
            None => return false,
        };
        let entry = unsafe { &mut *table.add(*index) };
        if *entry & ENTRY_PRESENT == 0 {
            let Some(frame) = memory::allocate_frame(allocator) else {
                return false;
            };
            unsafe {
                ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
            }
            *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE;
        } else if *entry & ENTRY_HUGE != 0 {
            // Entrada já é huge page, não faz nada
            return true;
        }
        table_physical = (*entry & ADDRESS_MASK) & !RSVD_MASK;
    }

    let Some(table) = table_pointer(table_physical) else {
        return false;
    };
    let entry = unsafe { &mut *table.add(indices[2]) };
    if physical_address & !PHYS_ADDR_MASK != 0 {
        return false;
    }
    if *entry & ENTRY_PRESENT != 0 {
        if writable {
            *entry |= ENTRY_WRITABLE;
        }
        *entry &= !RSVD_MASK;
        unsafe {
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
        return true;
    }
    *entry = (physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE | ENTRY_HUGE | ENTRY_USER;
    unsafe {
        asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
    }
    true
}

fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn digits_hex(digit: u64, buf: &mut [u8], n: &mut usize) {
    let c = if digit < 10 { b'0' + digit as u8 } else { b'a' + digit as u8 - 10 };
    buf[*n] = c;
    *n += 1;
}

/// Corrige bits reservados (52-62) nas entradas de tabela de páginas do
/// Limine para o range do kernel (.text, .rodata, .data, .bss) e mapeia
/// páginas que estejam ausentes.
///
/// O Limine pode deixar bits reservados definidos ou páginas não mapeadas
/// nas entradas de tabela de páginas, causando page fault quando o kernel
/// tenta acessar essas páginas. Esta função caminha pelas tabelas de páginas
/// para o range do kernel, corrige bits reservados e aloca frames para
/// páginas que não estão presentes.
pub fn fix_kernel_page_table_entries() {
    crate::serial_write(b"fix_kernel_page_table: start\r\n");
    const KERNEL_START: u64 = 0xffffffff80000000;
    const KERNEL_END: u64 = 0xffffffff80800000; // 8 MB of kernel VMA
    const RSVD_MASK: u64 = 0x7ff0_0000_0000_0000; // bits 52-62
    const NX_MASK: u64 = 1 << 63; // bit 63: NX (reserved unless EFER.NXE=1)

    let allocator = unsafe {
        let ptr_val = core::ptr::read(core::ptr::addr_of_mut!(crate::ALLOCATOR_PTR));
        if ptr_val.is_null() {
            crate::serial_write(b"fix_kernel_page_table: allocator not ready\r\n");
            return;
        }
        &mut *ptr_val
    };

    let hhdm_offset = allocator.hhdm_offset;
    let root_physical = read_cr3() & ADDRESS_MASK;
    crate::serial_write(b"fix_kernel_page_table: CR3=0x");
    crate::serial_write_hex(root_physical);
    crate::serial_write(b" hhdm=0x");
    crate::serial_write_hex(hhdm_offset);
    crate::serial_write(b"\r\n");

    // Dump page table for fault address 0xffffffff8004c000
    {
        let fault_page = 0xffffffff8004c000u64;
        let fault_indices = [
            ((fault_page >> 39) & 0x1ff) as usize,
            ((fault_page >> 30) & 0x1ff) as usize,
            ((fault_page >> 21) & 0x1ff) as usize,
            ((fault_page >> 12) & 0x1ff) as usize,
        ];
        let mut tp = root_physical;
        crate::serial_write(b"fix_kernel_page_table: walking fault page 0x4c000\r\n");
        for (level, index) in fault_indices.iter().enumerate() {
            let table_virt = match tp.checked_add(hhdm_offset) {
                Some(v) => v as *mut u64,
                None => {
                    crate::serial_write(b"  level ");
                    crate::serial_write_hex(level as u64);
                    crate::serial_write(b": overflow\r\n");
                    break;
                }
            };
            let entry = unsafe { *table_virt.add(*index) };
            crate::serial_write(b"  level ");
            crate::serial_write_hex(level as u64);
            crate::serial_write(b": index=");
            crate::serial_write_hex(*index as u64);
            crate::serial_write(b" entry=0x");
            crate::serial_write_hex(entry);
            crate::serial_write(b" P=");
            crate::serial_write(if entry & 1 != 0 { b"1" } else { b"0" });
            crate::serial_write(b" W=");
            crate::serial_write(if entry & 2 != 0 { b"1" } else { b"0" });
            crate::serial_write(b" U=");
            crate::serial_write(if entry & 4 != 0 { b"1" } else { b"0" });
            crate::serial_write(b"\r\n");
            if entry & 1 == 0 {
                break;
            }
            if level < 3 {
                tp = entry & ADDRESS_MASK;
            }
        }
    }

    let mut fixed_count = 0u64;
    let mut mapped_count = 0u64;

    // First pass: map any missing pages in the kernel range
    let mut addr = KERNEL_START;
    while addr < KERNEL_END {
        let page = addr & !(PAGE_SIZE - 1);
        let indices = [
            ((page >> 39) & 0x1ff) as usize,
            ((page >> 30) & 0x1ff) as usize,
            ((page >> 21) & 0x1ff) as usize,
            ((page >> 12) & 0x1ff) as usize,
        ];

        // Walk PML4 → PDPT → PD → PTE using direct HHDM offset conversion
        let mut table_physical = root_physical;
        let mut huge_page = false;
        let mut walk_ok = true;

        for (level, index) in indices.iter().take(3).enumerate() {
            let table_virt = match table_physical.checked_add(hhdm_offset) {
                Some(v) => v as *mut u64,
                None => { walk_ok = false; break; }
            };
            let table = unsafe { &mut *table_virt.add(*index) };

            if *table & ENTRY_PRESENT == 0 {
                let Some(frame) = memory::allocate_frame(allocator) else {
                    walk_ok = false;
                    break;
                };
                unsafe {
                    ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
                }
                *table = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE;
                mapped_count += 1;
            }

            if *index == indices[2] && *table & ENTRY_HUGE != 0 {
                huge_page = true;
                let fixed = *table & !(RSVD_MASK | NX_MASK);
                if fixed != *table {
                    *table = fixed;
                    fixed_count += 1;
                    unsafe {
                        asm!("invlpg [{}]", in(reg) page, options(nostack, preserves_flags));
                    }
                }
                break;
            }

            table_physical = *table & ADDRESS_MASK;
        }

        if walk_ok && !huge_page {
            let pte_virt = match table_physical.checked_add(hhdm_offset) {
                Some(v) => v as *mut u64,
                None => {
                    addr = addr.saturating_add(PAGE_SIZE);
                    continue;
                }
            };
            let entry_ptr = unsafe { pte_virt.add(indices[3]) };
            let entry = unsafe { *entry_ptr };
            if entry & ENTRY_PRESENT == 0 {
                let Some(frame) = memory::allocate_frame(allocator) else {
                    addr = addr.saturating_add(PAGE_SIZE);
                    continue;
                };
                unsafe {
                    ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
                }
                unsafe {
                    *entry_ptr = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE;
                }
                mapped_count += 1;
            } else {
                let fixed = entry & !(RSVD_MASK | NX_MASK);
                if fixed != entry {
                    unsafe {
                        *entry_ptr = fixed;
                        fixed_count += 1;
                        asm!("invlpg [{}]", in(reg) page, options(nostack, preserves_flags));
                    }
                }
            }
        }

        addr = addr.saturating_add(PAGE_SIZE);
    }

    crate::serial_write(b"fix_kernel_page_table: done mapped=");
    let mut digits = [0u8; 16];
    let mut cursor = digits.len();
    let mut v = mapped_count;
    if v == 0 { digits[cursor - 1] = b'0'; cursor -= 1; }
    while v > 0 { cursor -= 1; digits[cursor] = b'0' + (v % 10) as u8; v /= 10; }
    crate::serial_write(&digits[cursor..]);
    crate::serial_write(b" fixed=");
    let mut cursor = digits.len();
    let mut v = fixed_count;
    if v == 0 { digits[cursor - 1] = b'0'; cursor -= 1; }
    while v > 0 { cursor -= 1; digits[cursor] = b'0' + (v % 10) as u8; v /= 10; }
    crate::serial_write(&digits[cursor..]);
    crate::serial_write(b"\r\n");
}

fn table_pointer(physical: u64) -> Option<*mut u64> {
    let allocator = unsafe {
        let ptr_val = core::ptr::read(core::ptr::addr_of_mut!(crate::ALLOCATOR_PTR));
        if ptr_val.is_null() {
            return None;
        }
        &*ptr_val
    };
    Some(memory::phys_to_virt(allocator, physical)?.cast())
}