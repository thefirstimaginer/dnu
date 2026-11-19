# Especificação de Memória — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/mm/` (alocador de frames, paginação, gerenciador de memória)

---

## 1. Introdução

O subsistema de memória do kernel DNU gerencia frames físicos, page tables e o alocador de heap do kernel. O kernel opera em modo 64-bit com paginação de 4 níveis (PML4 → PDPT → PD → PT), páginas de 4 KiB.

O kernel não usa `std`; toda a alocação é feita por estruturas próprias em `no_std`.

---

## 2. FrameAllocator

Localizado em `src/mm/allocator.rs` e `src/mm/memory.rs`.

O `FrameAllocator` gerencia frames físicos disponíveis com base no mapa de memória fornecido pelo Limine (`BootInfo`). Principais operações:

| Função | Descrição |
|--------|-----------|
| `FrameAllocator::new()` | Cria um alocador vazio |
| `memory::init_with_allocator(...)` | Inicializa com entradas do mmap, offset HHDM e frames reservados |
| `memory::allocate_frame(...)` | Aloca um frame físico livre |
| `memory::deallocate_frame(...)` | Libera um frame físico |
| `memory::allocator_mut()` | Retorna referência mutável ao allocator global |

### Inicialização na Boot

Durante `kernel_entry`, o allocator é inicializado assim:

```rust
let mut allocator = memory::FrameAllocator::new();
unsafe {
    core::ptr::write(core::ptr::addr_of_mut!(crate::ALLOCATOR), MaybeUninit::new(allocator));
}
let mut allocator = memory::allocator_mut();
let frame_count = memory::init_with_allocator(
    allocator, entries, boot_info.hhdm_offset, &[]
);
```

O `ALLOCATOR` é um `static mut MaybeUninit<FrameAllocator>` colocado na seção `.data` para evitar page faults durante o boot inicial. Ele é escrito via `core::ptr::write` porque o allocator ainda não está totalmente inicializado.

---

## 3. HHDM Offset

O Limine mapeia a memória física em um offset alto (HHDM — Higher Half Direct Map). O kernel usa `boot_info.hhdm_offset` para converter endereços físicos em virtuais acessíveis:

```rust
// Exemplo: acessar um endereço físico via HHDM
let virt = physical_addr + boot_info.hhdm_offset;
```

**Importante:** `boot_info.dzimage` já é um endereço virtual acessível (mapeado pelo Limine na área alta do kernel em `boot.rs`). Não se soma `hhdm_offset` a ele, pois isso causaria double-mapping e endereço inválido.

---

## 4. Paginação

O kernel usa paginação de 4 níveis em x86_64:

```
PML4 → PDPT → PD → PT → Página de 4 KiB
```

### Componentes

| Arquivo | Descrição |
|---------|-----------|
| `src/mm/paging.rs` | Gerenciamento de page tables, fixação de entradas do kernel |
| `src/mm/allocator.rs` | Alocador de frames físicos |
| `src/mm/memory.rs` | Inicialização e API de alocação de memória |

A função `paging::fix_kernel_page_table_entries()` corrige entradas de page table do kernel após o boot inicial, garantindo que o kernel esteja corretamente mapeado no endereço alto.

---

## 5. Alocador de Heap

Após o `FrameAllocator` estar pronto, o alocador de heap global é inicializado via `allocator::allocator_init()`. O heap é usado para alocações dinâmicas do kernel durante a execução.

O subsistema `Allocator` é marcado como pronto no boot após a inicialização do heap.

---

## 6. Seções de Memória Especiais

| Seção | Propósito |
|-------|-----------|
| `.data.early` | Estado inicial do kernel (ex: bitmap de subsistemas prontos), sempre mapeado pelo Limine |
| `.data` | Dados globais do kernel (ex: `ALLOCATOR`) |
| `.note.pvh` | Nota PVH para boot em Xen |

---

## Referências

- [Especificação de Boot](./bootspec.md) — fluxo de boot e inicialização de memória
- [Arquitetura x86_64](./archspec.md) — contexto de modo longo e paginação
- [Subsistemas de Boot](./subsystems.md) — estado dos subsistemas de memória
