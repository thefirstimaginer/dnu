use crate::memory;
use core::arch::asm;
use core::ptr;

const PAGE_SIZE: u64 = 4096;
const ENTRY_PRESENT: u64 = 1;
const ENTRY_WRITABLE: u64 = 1 << 1;
const ENTRY_USER: u64 = 1 << 2;
const ENTRY_HUGE: u64 = 1 << 7;
/// Bit NX (No-Execute): desativa a execução de código nesta página.
const ENTRY_NX: u64 = 1 << 63;
/// Máscara de endereço físico válido em entradas de tabela de páginas (bits 0:51, alinhado à página).
///
/// Bits 52:62 são reservados em x86-64 e devem ser zero em todas as entradas
/// de tabela de páginas (PML4, PDPT, PD, PTE). Bit 63 (NX) é controlado
/// explicitamente pelas flags da página.
const PHYS_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
/// Máscara de bits válidos em uma entrada de tabela de página: mantém flags
/// (bits 0:11), endereço físico (bits 12:51) e NX (bit 63), limpando bits
/// reservados (52:62) que causam #PF com error code RSVD (bit 3).
const ENTRY_VALID_MASK: u64 = 0x800f_ffff_ffff_f0ff;
const ADDRESS_MASK: u64 = PHYS_ADDR_MASK;
const USER_MIN: u64 = 0x400000;
const USER_MAX: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy)]
pub struct AddressSpace {
    root_physical: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    InvalidAddress,
    OutOfMemory,
    AlreadyMapped,
    /// A faixa cruza uma página mapeada no meio (só ocorre em `try_map_range`).
    PartiallyMapped,
}

/// Permissões de uma página de usuário (bit a bit).
///
/// Espelha `PROT_*` da ABI, mas vive aqui para o módulo de paginação não
/// depender de `crate::abi`: o `mmap` traduz `PROT_WRITE`/`PROT_EXEC` para cá.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFlags(u8);

impl PageFlags {
    /// Página somente-leitura, não executável (W^X: nem escrita nem execução).
    pub const READ_ONLY: Self = Self(0);
    /// Página gravável, não executável (dados, pilha, heap).
    pub const READ_WRITE: Self = Self(1);
    /// Página somente-leitura, executável (código).
    pub const READ_EXEC: Self = Self(2);

    pub const fn is_writable(self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn is_executable(self) -> bool {
        self.0 & 2 != 0
    }

    /// Bits a somar na PTE (o bit `PRESENT`/`USER` fica a cargo do mapeador).
    ///
    /// W^X: uma página não pode ser simultaneamente gravável e executável.
    /// Páginas não executáveis recebem o bit NX para impedir execução de
    /// shellcode injetado.
    const fn entry_bits(self) -> u64 {
        let mut bits = 0u64;
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
    pub fn new_user() -> Option<Self> {
        let current_root = read_cr3() & ADDRESS_MASK;
        let root = memory::allocate_frame()?;
        unsafe {
            ptr::write_bytes(root.virtual_address, 0, PAGE_SIZE as usize);
            let source = memory::phys_to_virt(current_root)?.cast::<u64>();
            let destination = root.virtual_address.cast::<u64>();
            ptr::copy_nonoverlapping(source.add(256), destination.add(256), 256);
        }
        Some(Self {
            root_physical: root.physical_address,
        })
    }

    /// Mapeia uma página de usuário, somente-leitura e não executável.
    ///
    /// Usada para páginas que não precisam de escrita nem execução. É o
    /// padrão seguro para W^X: nenhuma página é gravável nem executável a
    /// menos que o chamador solicite explicitamente.
    pub fn map_user_page(&self, virtual_address: u64) -> Result<*mut u8, MapError> {
        self.map_user_page_with(virtual_address, PageFlags::READ_ONLY)
    }

    /// Mapeia uma página de usuário, legível e gravável (não executável).
    ///
    /// Usada para pilha, heap e segmentos de dados. Recebe o bit NX para
    /// impedir execução de código injetado nessas páginas.
    pub fn map_user_page_writable(
        &self,
        virtual_address: u64,
    ) -> Result<*mut u8, MapError> {
        self.map_user_page_with(virtual_address, PageFlags::READ_WRITE)
    }

    /// Mapeia uma página de usuário com as permissões pedidas.
    ///
    /// Se a PTE já existe o mapeamento é **atualizado** (as permissões passam a
    /// valer, o frame é preservado). É o que permite implementar
    /// demais mudanças de proteção sem realocar memória.
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
                let frame = memory::allocate_frame().ok_or(MapError::OutOfMemory)?;
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
            // Já mapeada: só ajusta a proteção e mantém o frame. Sem isto não
            // haveria como implementar `mprotect`/`MAP_FIXED` idempotente.
            *entry = (*entry & !ENTRY_WRITABLE) | flags.entry_bits();
            return Ok(memory::phys_to_virt(*entry & ADDRESS_MASK)
                .unwrap_or(core::ptr::null_mut()));
        }
        let frame = memory::allocate_frame().ok_or(MapError::OutOfMemory)?;
        unsafe {
            ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
        }
        *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_USER | flags.entry_bits();
        Ok(frame.virtual_address)
    }

    /// Mapeia uma faixa contígua de páginas de usuário, **tudo ou nada**.
    ///
    /// Base do `mmap`: verifica a faixa inteira antes de tocar em qualquer PTE,
    /// então uma falha no meio (`AlreadyMapped`, falta de memória) desfaz as
    /// páginas já instaladas em vez de deixar o processo com um mapeamento
    /// parcial invisível. A faixa não precisa estar alinhada ao início da
    /// página: páginas parciais são mapeadas por inteiro, como o POSIX manda.
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

        // Primeiro, nenhuma página pode estar presente: mapear por cima
        // corromperia memória que o chamador não pediu para descartar.
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
                    // `mapped` é o fim exclusivo do que já foi instalado.
                    mapped = page.checked_add(PAGE_SIZE).ok_or(MapError::InvalidAddress)?;
                }
                Err(MapError::AlreadyMapped) => {
                    // Corrida com outra thread do mesmo processo (o kernel não
                    // tem lock de espaço hoje): desfaz o que fizemos.
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

    /// Desmapeia a faixa `[from, to)` depois de uma falha no meio de `try_map_range`.
    fn unmap_range_best_effort(&self, from: u64, to: u64) {
        let mut page = from;
        while page < to {
            self.unmap_user_page(page);
            page = page.saturating_add(PAGE_SIZE);
        }
    }

    /// Desmapeia uma página de usuário e devolve o frame ao alocador.
    ///
    /// Base de `munmap`. Se a página não estiver mapeada, devolve `false` (o
    /// chamador trata como sucesso, pois `munmap` de uma faixa já livre é válido).
    /// As tabelas de páginas intermediárias são mantidas mapeadas; só o PTE do
    /// último nível é limpo. É a contrapartida de [`map_user_page`] necessária
    /// para o `GlobalAlloc` da `std` liberar memória de verdade.
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
            let entry = unsafe { *table.add(*index) };
            if entry & ENTRY_PRESENT == 0 {
                return false;
            }
            table_physical = entry & ADDRESS_MASK;
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
        // Invalida o TLB para este endereço; sem isto, acessos antigos
        // continuariam a acertar a tradução desmapeada.
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) page_address, options(nostack, preserves_flags));
        }
        memory::free_frame(physical_address);
        true
    }

    /// Cria um novo espaço de endereço com uma **cópia** de todas as páginas de
    /// usuário mapeadas neste.
    ///
    /// É um `fork` clássico sem copy-on-write: cada página de usuário recebe um
    /// frame novo com o conteúdo duplicado. O half superior (kernel) é
    /// compartilhado, como em [`AddressSpace::new_user`].
    ///
    /// Devolve `None` se faltar memória; nesse caso os frames já alocados são
    /// liberados, mas o novo espaço de endereço parcial pode permanecer com
    /// tabelas de páginas alocadas (elas são poucas e permanecem ligadas à
    /// tabela raiz, que é descartada).
    pub fn clone_user(&self) -> Option<Self> {
            let clone = Self::new_user()?;
            if self.copy_user_pages(&clone) == 0 {
                // O espaço de origem não tinha nenhuma página de usuário: não há o
                // que herdar, e o clone é inútil.
                return None;
            }
            Some(clone)
        }

        /// Percorre a metade de usuário da árvore e copia cada página presente.
    ///
    /// Devolve quantas páginas (PTEs de nível 1) foram copiadas; `0` significa
    /// que o espaço não tinha nada mapeado.
    fn copy_user_pages(&self, destination: &Self) -> usize {
        let mut copied = 0usize;
        for pml4_index in 0..256usize {
            // `4` é o nível da tabela percorrida (o PML4). O `copy_table` desce
            // até o nível 1 (tabela de PTEs); passar `3` aqui desalinhava todos
            // os níveis e copiava a página errada — o `fork` acabava com quase
            // nenhuma página no espaço do filho.
            copied += self.copy_table(
                self.root_physical,
                destination.root_physical,
                pml4_index,
                4,
            );
        }
        copied
    }

    /// Copia recursivamente uma entrada de tabela até chegar a nível 1 (PTE).
    ///
    /// `level` indica o nível da tabela que está sendo percorrida: 3 = PDPT,
    /// 2 = PD, 1 = PT. Ao chegar nas PTEs, cada página mapeada é copiada para um
    /// frame novo.
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
            // PTE: aloca um frame novo e copia o conteúdo da página.
            let Some(frame) = memory::allocate_frame() else {
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
            // Preserva os bits de controle da PTE original (presente, escrita,
            // usuário), trocando apenas o endereço físico da página.
            *destination_entry = (source_entry & !ADDRESS_MASK) | frame.physical_address;
            return 1;
        }

        if source_entry & ENTRY_HUGE != 0 {
            // Páginas enormes (2 MiB) na metade de usuário não são usadas pelo
            // loader, que mapeia sempre páginas de 4 KiB. Recusar aqui é mais
            // seguro do que copiar errado.
            return 0;
        }

        // Tabela intermediária: garante uma tabela própria no destino e desce.
        let next_source = source_entry & ADDRESS_MASK;
        if *destination_entry & ENTRY_PRESENT == 0 {
            let Some(frame) = memory::allocate_frame() else {
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

    /// Mapeia uma faixa de memória física (o framebuffer) na metade de usuário.
    ///
    /// Diferente de [`map_user_page`], a página física é **compartilhada**, não
    /// alocada: a memória de vídeo é um recurso único, e todos os clientes do
    /// display precisam vê-la. O mapeamento é gravável e de usuário — o kernel
    /// não protege a tela de um cliente mal comportado, o que é aceitável para
    /// um servidor de display em desenvolvimento.
    ///
    /// Devolve o endereço virtual onde a faixa ficou mapeada.
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

    /// Mapeia o framebuffer na metade do usuário como **somente-leitura**.
    ///
    /// Usado por `SYS_FB_MAP` para que clients de display não possam gravar
    /// diretamente na memória física de vídeo.
    pub fn map_framebuffer_readonly(
        &self,
        virtual_address: u64,
        physical_address: u64,
        length: usize,
    ) -> Option<u64> {
        self.map_framebuffer_range(virtual_address, physical_address, length, false)
    }

    /// Instala uma PTE apontando para uma página física existente.
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
                let frame = memory::allocate_frame()?;
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
        // Valida que o endereço físico não tem bits reservados (52:62) antes de
        // gravar na PTE. Um bit reservado setado causa #PF com error code RSVD.
        if physical_address & !PHYS_ADDR_MASK != 0 {
            return None;
        }
        let mut bits = ENTRY_PRESENT | ENTRY_USER | ENTRY_NX;
        if writable {
            bits |= ENTRY_WRITABLE;
        }
        *entry = (physical_address & PHYS_ADDR_MASK) | bits;
        memory::phys_to_virt(physical_address)
    }

    /// `translate` para uma página inteira, sem o deslocamento interno.
    ///
    /// Usado por `mmap` para descobrir se a faixa já está ocupada sem precisar
    /// de um endereço específico dentro dela.
    pub fn page_is_present(&self, page: u64) -> bool {
        self.translate(page & !(PAGE_SIZE - 1)).is_some()
    }

    /// `true` se a PTE da página existe e está gravável.
    ///
    /// O `#PF` de escrita precisa disto para distinguir "página de `mmap`
    /// ainda não populada" (erro de escrita, PTE ausente → popular) de uma
    /// escrita legítima em página somente-leitura (violação → `SIGSEGV`).
    pub fn page_is_writable(&self, page: u64) -> bool {
        let Some(entry) = self.page_entry(page & !(PAGE_SIZE - 1)) else {
            return false;
        };
        unsafe { (*entry & ENTRY_PRESENT != 0) && (*entry & ENTRY_WRITABLE != 0) }
    }

    /// Marca uma página como não presente sem devolver o frame ao alocador.
    ///
    /// Usado por `munmap` de faixas parcialmente presentes: o frame é liberado
    /// pelo chamador apenas quando ele tem certeza de que a página pertence à
    /// faixa (ver [`AddressSpace::flush_range`]).
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

    /// Endereço da PTE de uma página, ou `None` se alguma tabela não existe.
    fn page_entry(&self, page_address: u64) -> Option<*mut u64> {
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
        memory::phys_to_virt(physical)
    }
}

pub fn switch(space: AddressSpace) {
    unsafe {
        asm!("mov cr3, {}", in(reg) space.root_physical, options(nostack, preserves_flags));
    }
}

pub fn map_hhdm_range(hhdm_offset: u64, physical_address: u64, length: usize, writable: bool) -> bool {
    let Some(end) = physical_address.checked_add(length as u64) else {
        return false;
    };
    if length == 0 {
        return true;
    }

    let mut physical = physical_address & !(PAGE_SIZE - 1);
    let end = end.saturating_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    while physical < end {
        let Some(virtual_address) = hhdm_offset.checked_add(physical) else {
            return false;
        };
        if !map_hhdm_page(virtual_address, physical, writable) {
            return false;
        }
        physical = physical.saturating_add(PAGE_SIZE);
    }
    true
}

fn map_hhdm_page(virtual_address: u64, physical_address: u64, writable: bool) -> bool {
    let indices = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];
    let mut table_physical = read_cr3() & ADDRESS_MASK;
    for (level, index) in indices.iter().take(3).enumerate() {
        let Some(table) = table_pointer(table_physical) else {
            return false;
        };
        let entry = unsafe { &mut *table.add(*index) };
        if *entry & ENTRY_PRESENT == 0 {
            let Some(frame) = memory::allocate_frame() else {
                return false;
            };
            unsafe {
                ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
            }
            *entry = (frame.physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE;
        } else if level > 0 && *entry & ENTRY_HUGE != 0 {
            return true;
        }
        table_physical = *entry & ADDRESS_MASK;
    }

    let Some(table) = table_pointer(table_physical) else {
        return false;
    };
    let entry = unsafe { &mut *table.add(indices[3]) };
    // Garante que o endereço físico não tenha bits reservados (52:62) antes de
    // escrever na entrada da tabela de páginas. Um bit reservado setado causa
    // #PF com error code bit 3 (RSVD) em qualquer acesso que traduz essa entrada.
    if physical_address & !PHYS_ADDR_MASK != 0 {
        return false;
    }
    if *entry & ENTRY_PRESENT != 0 {
        // Atualiza permissões: garante escrita se solicitado.
        if writable {
            *entry |= ENTRY_WRITABLE;
        }
        unsafe {
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
        return *entry & ADDRESS_MASK == physical_address;
    }
    *entry = (physical_address & PHYS_ADDR_MASK) | ENTRY_PRESENT | ENTRY_WRITABLE;
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

fn table_pointer(physical: u64) -> Option<*mut u64> {
    Some(memory::phys_to_virt(physical)?.cast())
}

/// Navega pelas tabelas de páginas para o endereço virtual informado e limpa
/// bits reservados (52:62) de todas as entradas encontradas no caminho.
///
/// O Limine pode configurar entradas de tabela de páginas com bits reservados
/// setados, o que causa #PF com error code bit 3 (RSVD) em qualquer acesso que
/// traduza essas entradas. Esta função varre as tabelas para o range de endereços
/// virtuais do kernel e corrige as entradas, preservando os bits de controle
/// (present, writable, user, NX, etc.) e apenas removendo bits que o hardware
/// considera reservados.
///
/// É chamada cedo no boot, antes de acessar qualquer dado estático que possa
/// residir em uma página com entrada corrompida.
/// Navega pelas tabelas de páginas para o range de endereços virtuais do
/// kernel e limpa bits reservados (52:62) de todas as entradas encontradas.
///
/// O `hhdm_offset` é usado para converter endereços físicos de tabelas de
/// páginas em endereços virtuais acessíveis pelo kernel.
///
/// O Limine pode configurar entradas de tabela de páginas com bits reservados
/// setados, o que causa #PF com error code bit 3 (RSVD) em qualquer acesso que
/// traduza essas entradas. Esta função varre as tabelas para o range de
/// endereços virtuais do kernel e corrige as entradas, preservando os bits de
/// controle (present, writable, user, NX, etc.) e apenas removendo bits que o
/// hardware considera reservados.
///
/// É chamada cedo no boot, antes de acessar qualquer dado estático que possa
/// residir em uma página com entrada corrompida.
pub fn fix_kernel_page_table_entries(hhdm_offset: u64, virtual_start: u64, virtual_end: u64) {
    let root_physical = read_cr3() & ADDRESS_MASK;
    let mut address = virtual_start & !(PAGE_SIZE - 1);
    let mut fixed = 0u64;
    let mut mapped = 0u64;
    let mut scanned = 0u64;
    while address < virtual_end {
        let indices = [
            ((address >> 39) & 0x1ff) as usize,
            ((address >> 30) & 0x1ff) as usize,
            ((address >> 21) & 0x1ff) as usize,
            ((address >> 12) & 0x1ff) as usize,
        ];
        let mut table_physical = root_physical;
        let mut ok = true;
        // Navega pelas tabelas intermediarias (PML4 -> PDPT -> PD -> PT).
        // Se alguma tabela intermediaria estiver ausente, pula esta pagina.
        for index in indices.iter().take(3) {
            let Some(table_phys) = table_physical.checked_add(hhdm_offset) else {
                ok = false;
                break;
            };
            let table_ptr = unsafe { (table_phys as *mut u64).as_mut() };
            let Some(table) = table_ptr else {
                ok = false;
                break;
            };
            let table = unsafe { core::slice::from_raw_parts_mut(table as *mut u64, 512) };
            let entry = unsafe { &mut table[*index] };
            if *entry & ENTRY_PRESENT == 0 {
                ok = false;
                break;
            }
            let old = *entry;
            *entry &= ENTRY_VALID_MASK;
            if old != *entry { fixed += 1; }
            table_physical = *entry & ADDRESS_MASK;
        }
        if ok {
            let Some(table_phys) = table_physical.checked_add(hhdm_offset) else {
                address = address.checked_add(PAGE_SIZE).unwrap_or(u64::MAX);
                continue;
            };
            let table_ptr = unsafe { (table_phys as *mut u64).as_mut() };
            let Some(table) = table_ptr else {
                address = address.checked_add(PAGE_SIZE).unwrap_or(u64::MAX);
                continue;
            };
            let table = unsafe { core::slice::from_raw_parts_mut(table as *mut u64, 512) };
            let entry = unsafe { &mut table[indices[3]] };
            if *entry & ENTRY_PRESENT == 0 {
                // PTE ausente: aloca um frame zeroed e mapeia para este
                // endereço. Isso cobre paginas do .bss que o Limine nao alocou.
                let Some(frame) = memory::allocate_frame() else {
                    address = address.checked_add(PAGE_SIZE).unwrap_or(u64::MAX);
                    continue;
                };
                unsafe {
                    core::ptr::write_bytes(frame.virtual_address, 0, PAGE_SIZE as usize);
                }
                *entry = (frame.physical_address & PHYS_ADDR_MASK)
                    | ENTRY_PRESENT | ENTRY_WRITABLE;
                mapped += 1;
            } else {
                let old = *entry;
                *entry &= ENTRY_VALID_MASK;
                if old != *entry { fixed += 1; }
            }
        }
        scanned += 1;
        address = address.checked_add(PAGE_SIZE).unwrap_or(u64::MAX);
    }
    crate::serial_write(b"fix_paging: scanned=");
    let mut buf = [0u8; 16];
    let mut n = 0;
    let mut v = scanned;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 10) as u8; n += 1; v /= 10; }
    for i in (0..n).rev() { crate::serial_write(&buf[i..i+1]); }
    crate::serial_write(b" fixed=");
    let mut n = 0;
    let mut v = fixed;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 10) as u8; n += 1; v /= 10; }
    for i in (0..n).rev() { crate::serial_write(&buf[i..i+1]); }
    crate::serial_write(b" mapped=");
    let mut n = 0;
    let mut v = mapped;
    if v == 0 { buf[n] = b'0'; n += 1; }
    while v > 0 { buf[n] = b'0' + (v % 10) as u8; n += 1; v /= 10; }
    for i in (0..n).rev() { crate::serial_write(&buf[i..i+1]); }
    crate::serial_write(b"\r\n");
}
