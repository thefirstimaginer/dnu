# Plano de Implementação — Roadmap Futuro do DNU

Este plano detalha os itens do roadmap futuro (seção 4 da análise original), quebrados em **peças pequenas e independentes**, cada uma com:
- Objetivo
- Por que fazer (problema resolvido)
- Locais de código
- Dependências (peças que devem vir antes)
- Critérios de aceitação (Definition of Done)

Ordem sugerida: de baixo risco / isolado para alto risco / sistêmico. Não há urgência de ordem estrita, exceto onde indicado.

---

## Área 1 — Segurança e Isolamento

### 1.1. Stack canaries em usuários
**Objetivo:** Detectar estouro de pilha (stack buffer overflow) em processos userspace.

**Locais:**
- `src/process/loader.rs` — preparação da pilha do usuário (após `prepare_user_stack`)
- `src/process/descriptors.rs` — frames de sinal / restaurador de pilha
- `abi/src/types.rs` — constante `STACK_CANARY_VALUE` (opcional)

**Dependências:** nenhuma (guard pages já implementados na peça 2 do roadmap anterior)

**Critérios de aceitação:**
- Um valor aleatório (canary) é gravado no topo da pilha do usuário antes de `iretq`
- O restaurador de sinal / retorno de syscall verifica o canary antes de restaurar
- Se o canary foi modificado, o processo recebe `SIGSEGV` (não silencioso)

---

### 1.2. Controle de acesso real em superfícies (tokens/capacidades)
**Objetivo:** Substituir identificação apenas por PID por tokens/capacidades, impedindo que qualquer processo conheça o id de uma superfície e a manipule.

**Status parcial:** `surface_send_event` já verifica dono (implementado). Faltam: `surface_destroy`, `surface_present`, `surface_flush`, `surface_move` para superfícies alheias via token, e geração de token no `surface_create`.

**Locais:**
- `src/console/surface.rs` — struct `Surface` ganha campo `token: u64`
- `src/abi/abi_display.rs` — `surface_create` gera token aleatório e retorna em `SurfaceInfo`; outras syscalls validam token
- `src/process/scheduler.rs` — `release_pid` limpa superfícies (já existe)

**Critérios de aceitação:**
- `surface_create` retorna um `token` ao cliente
- `surface_destroy`/`present`/`flush`/`move` rejeitam chamadas com token inválido ou ausente (`EINVAL`)
- O dono continua podendo operar sem token (compatibilidade) ou com token (novo modo)

---

### 1.3. Assinatura do dzImage (ed25519) em vez de apenas CRC-32
**Objetivo:** Impedir que um atacante modifique o dzImage e recompute o CRC-32.

**Locais:**
- `src/boot/dzimage.rs` — função `verify` adiciona verificação de assinatura ed25519
- `tools/dzimage_host.rs` — ferramenta de host assina o dzImage
- `abi/src/types.rs` — constante da chave pública embarcada no kernel

**Dependências:** nenhuma (CRC-32 já existe; assinatura é camada adicional)

**Critérios de aceitação:**
- `dzimage::verify` aceita opcionalmente uma assinatura ed25519
- Kernel embarca a chave pública para verificação
- Ferramenta de host pode assinar um dzImage (`dnu-pack --sign`)
- Imagem com assinatura inválida é rejeitada no boot

---

### 1.4. SMEP/SMAP (CR4)
**Objetivo:** Impedir que o kernel acesse diretamente memória de usuário (SMEP) e que usuário acesse memória de kernel (SMAP).

**Locais:**
- `src/arch/idt.rs` — `configure_syscall_msrs` configura CR4.SMEP e CR4.SMAP
- `src/mm/paging.rs` — funções de acesso a memória de usuário devem usar `stac`/`clac` quando necessário

**Dependências:** EFER.NXE já configurado; CR4 acessível em ring 0

**Critérios de aceitação:**
- CR4 tem bits SMEP (bit 20) e SMAP (bit 21) definidos após boot
- Acesso direto do kernel a endereços de usuário gera page fault (a menos que use `stac`/`clac`)

---

### 1.5. KPTI (tabelas de página separadas kernel/userspace)
**Objetivo:** Impedir que userspace veja endereços do kernel (mitiga Meltdown-like).

**Locais:**
- `src/mm/paging.rs` — `AddressSpace::new_user` não copia entradas do kernel acima de USER_MAX
- `src/process/scheduler.rs` — `schedule` faz `paging::switch` entre address spaces de usuário e kernel

**Dependências:** W^X estrito já implementado; paginação de 4 níveis existente

**Critérios de aceitação:**
- Address space de usuário não mapeia memória do kernel acima de `USER_MAX`
- Syscall entra no kernel com CR3 do kernel (switch de tabelas)

---

## Área 2 — Memória e Processos

### 2.1. Copy-on-write (COW) para fork
**Objetivo:** `fork` não copiar toda a área de usuários; páginas são compartilhadas até serem escritas.

**Locais:**
- `src/mm/paging.rs` — `clone_user` marca páginas como COW (somente leitura + bit COW personalizado)
- `src/process/loader.rs` — `clone_user` usado por `fork`
- Page fault handler (idt.rs) — intercepta escrita em página COW, aloca cópia

**Critérios de aceitação:**
- `fork` não duplica frames físicos imediatamente
- Primeira escrita em página compartilhada dispara page fault, aloca nova página e copia conteúdo
- Pai e filho têm páginas independentes após escrita

---

### 2.2. mmap com suporte a arquivo (não apenas anônimo)
**Objetivo:** Permitir `mmap` de arquivos (não só anônimo), com paginação sob demanda.

**Locais:**
- `src/abi/abi_user.rs` — handler `mmap` aceita `fd` válido (não apenas `MAP_ANONYMOUS`)
- `src/fs/vfs.rs` — leitura de páginas de arquivo sob demanda
- `src/process/space.rs` — VMA armazena referência a arquivo + offset

**Critérios de aceitação:**
- `mmap` de um arquivo existente mapeia páginas sob demanda (page cache)
- Writes em mapeamento compartilhado (`MAP_SHARED`) gravam no arquivo
- Writes em mapeamento privado (`MAP_PRIVATE`) usam COW

---

### 2.3. OOM killer / falha graciosa
**Objetivo:** Quando `allocate_frame` retorna `None`, o kernel não trava; mata um processo ou falha graciosamente.

**Locais:**
- `src/mm/memory.rs` — `allocate_frame` retorna `None` (já acontece)
- `src/process/scheduler.rs` — `schedule` ou chamador de `allocate_frame` escolhe vítima para OOM
- `src/main.rs` — boot frame sem alocação não faz `halt()`

**Critérios de aceitação:**
- Falha de alocação de frame não resulta em `kernel_halt()` / `hlt` infinito
- O kernel mata o processo de menor prioridade / maior consumidor de memória (OOM killer simples)
- Ou retorna `ENOMEM` para a syscall que solicitou

---

### 2.4. Pilhas de kernel dinâmicas
**Objetivo:** Substituir array fixo `[u8; KERNEL_STACK_SIZE]; MAX_PROCESSES` por alocação dinâmica.

**Locais:**
- `src/process/scheduler.rs` — `KERNEL_STACKS` deixa de ser array fixo
- `src/process/scheduler.rs` — `schedule` aloca pilha de kernel por demanda

**Critérios de aceitação:**
- Não há mais limite rígido de 32 pilhas de kernel reservadas estaticamente
- Pilhas de kernel são alocadas/liberadas com criação/destruição de processos

---

### 2.5. Maior número de processos e threads (já parcialmente feito: MAX_PROCESSES 16→32)
**Objetivo:** Escalar além de 32 processos, com suporte a múltiplas threads por processo.

**Locais:**
- `abi/src/types.rs` — `MAX_PROCESSES` e `MAX_THREADS`
- `src/process/scheduler.rs` — estrutura de threads dentro de processo

**Critérios de aceitação:**
- Suporta pelo menos 64 processos simultâneos
- Múltiplas threads por processo compartilham address space

---

## Área 3 — Sistema de Arquivos e Dispositivos

### 3.1. Symlinks e hardlinks no overlay RAMFS
**Objetivo:** O overlay RAMFS suporta links simbólicos e hardlinks.

**Locais:**
- `src/fs/ramfs.rs` — `NodeKind` ganha `Symlink` e `Hardlink`; `OverlayNode` armazena target path
- `src/fs/vfs.rs` — resolução de caminho segue symlinks

**Critérios de aceitação:**
- `symlink(target, linkpath)` cria um nó de tipo symlink
- `readlink` lê o target de um symlink
- Resolução de caminho segue symlinks (com limite de recursão para evitar loops)

---

### 3.2. Metadados (timestamps, proprietário) e quota no overlay RAMFS
**Objetivo:** Timestamps de acesso/modificação, uid/gid, e quota por usuário.

**Status parcial:** timestamps já implementados (peça 3 do roadmap anterior). Faltam quota e atime.

**Locais:**
- `src/fs/ramfs.rs` — `OverlayNode` ganha `accessed_at`; quota por `uid`
- `src/abi/abi_fs.rs` — syscalls de stat retornam metadados completos

**Critérios de aceitação:**
- `stat` retorna `st_atime`, `st_mtime`, `st_ctime`, `st_uid`, `st_gid`
- Quota por usuário impede que um usuário consuma todo o OVERLAY_DATA_CAPACITY

---

### 3.3. pivot_root e mount table no VFS
**Objetivo:** Suporte completo a `pivot_root` e tabela de montagens.

**Locais:**
- `src/fs/vfs.rs` — `Vfs` mantém tabela de montagens; `pivot_root` syscall
- `src/abi/abi_fs.rs` — handlers `mount`, `umount`, `pivot_root`

**Critérios de aceitação:**
- `mount` registra um sistema de arquivos em um ponto de montagem
- `pivot_root` troca a raiz do sistema de arquivos
- Caminhos resolvem através de pontos de montagem

---

### 3.4. Driver de disco: DMA/IDE, LBA48 (>128 GiB), NVMe, AHCI
**Objetivo:** Substituir ATA PIO polling por DMA e suportar discos >128 GiB.

**Locais:**
- `src/drivers/ata.rs` — modo DMA com IRQ (não polling)
- `src/drivers/` — novo driver NVMe ou AHCI (opcional, depende de hardware alvo)

**Critérios de aceitação:**
- Leitura/escrita de disco usa DMA com interrupção (não busy-wait de 1.000.000 iterações)
- LBA48 suporta discos >128 GiB
- Não há busy-wait longo bloqueando a CPU única

---

### 3.5. Sistemas de arquivos FAT/ext2
**Objetivo:** Ler/escrever em partições FAT ou ext2 além do DFS nativo.

**Locais:**
- `src/fs/` — novo módulo `fat.rs` ou `ext2.rs`
- `src/fs/vfs.rs` — registra novo backend no VFS

**Critérios de aceitação:**
- Kernel pode montar uma partição FAT ou ext2
- Arquivos podem ser lidos via VFS

---

### 3.6. /dev real com device nodes, TTYs e PTYs
**Objetivo:** Sistema de arquivos `/dev` com nós de dispositivo, terminais e pseudo-terminais.

**Locais:**
- `src/fs/ramfs.rs` ou novo `src/fs/devfs.rs` — device nodes
- `src/drivers/` — drivers de TTY/PTY

**Critérios de aceitação:**
- `/dev/tty`, `/dev/console`, `/dev/null` existem
- Processos podem abrir `/dev/tty` para entrada/saída

---

## Área 4 — Userspace e Ecossistema

### 4.1. Biblioteca padrão dnu-std
**Objetivo:** `std` do Rust sobre as syscalls do DNU (alloc, threads, fs, net, etc.).

**Locais:**
- Repositório separado ou `tools/dnu-std/` — crate `dnu-std` com `#[global_allocator]` sobre `sbrk`/`mmap` do kernel
- `src/abi/` — exposição de syscalls faltantes para `std`

**Critérios de aceitação:**
- Um programa Rust simples (`fn main()` com `println!`) compila com `dnu-std` e roda no DNU
- `std::thread::spawn`, `std::fs::File`, `std::net::TcpStream` funcionam

---

### 4.2. Shell e init system
**Objetivo:** Shell interativo e supervisão de serviços.

**Locais:**
- Userspace (fora do kernel): `shell/` e `init/` em repositório de userspace
- Kernel: syscalls já existentes são suficientes

**Critérios de aceitação:**
- `/sbin/init` (PID 1) inicia e supervisiona serviços
- Shell aceita comandos básicos (`ls`, `cat`, `echo`, `exit`)

---

### 4.3. Servidor de display / compositor (ws_server)
**Objetivo:** Servidor de display em userspace que compõe superfícies.

**Locais:**
- Userspace: `ws_server/` (fora do kernel)
- Kernel: syscalls de superfície já existem

**Critérios de aceitação:**
- `ws_server` roda como PID além do 1, consome eventos de superfície e desenha na tela
- Múltiplas janelas de diferentes clientes aparecem na tela

---

## Área 5 — Hardware e SMP

### 5.1. Spinlocks e per-CPU data
**Objetivo:** Suporte a múltiplos CPUs com estado protegido por travas.

**Locais:**
- `src/mm/memory.rs`, `src/process/scheduler.rs`, `src/fs/ramfs.rs` — estado global protegido por spinlocks em vez de `UnsafeCell` sem trava
- Novo `src/arch/smp.rs` — per-CPU data, ID de CPU atual

**Critérios de aceitação:**
- Estado global usa spinlocks (`spin::Mutex` ou equivalente)
- Cada CPU tem seu próprio stack e dados per-CPU

---

### 5.2. APIC/x2APIC e IPIs
**Objetivo:** Inter-processor interrupts para SMP.

**Locais:**
- `src/arch/smp.rs` — inicialização do APIC local e I/O APIC
- `src/arch/idt.rs` — vetores de IPI

**Critérios de aceitação:**
- CPU secundária pode ser iniciada (SIPI)
- IPI entre CPUs funciona (ex: TLB shootdown)

---

### 5.3. ACPI completo (sleep, shutdown, tabelas de configuração)
**Objetivo:** Suporte a ACPI para gerenciamento de energia e configuração de hardware.

**Locais:**
- `src/drivers/acpi.rs` — parse de tabelas ACPI (FADT, MADT, etc.)
- `src/main.rs` — shutdown via ACPI

**Critérios de aceitação:**
- Kernel lê tabelas ACPI do firmware
- Shutdown via ACPI funciona

---

### 5.4. PCI enumeration
**Objetivo:** Enumerar dispositivos PCI.

**Locais:**
- `src/drivers/pci.rs` — config space PCI, enumeração de barras/dispositivos

**Critérios de aceitação:**
- Kernel lista dispositivos PCI e seus barramentos

---

### 5.5. Rede: driver Ethernet + pilha TCP/IP
**Objetivo:** Conectividade de rede.

**Locais:**
- `src/drivers/net/` — driver (rtl8139, e1000, virtio-net)
- `src/net/` — pilha TCP/IP (IP, TCP, UDP)

**Critérios de aceitação:**
- Kernel envia/recebe pacotes Ethernet
- Ping ou TCP simples funciona

---

## Área 6 — Ferramentas e Qualidade

### 6.1. CI com build em QEMU
**Objetivo:** Todo commit é buildado e testado em QEMU automaticamente.

**Locais:**
- `.github/workflows/` ou equivalente — CI que roda `make check` e `make test` em QEMU

**Critérios de aceitação:**
- Push/PR dispara build do kernel para `x86_64-unknown-none`
- Kernel boot em QEMU e atinge `start_init`

---

### 6.2. Testes de integração que bootem o kernel (QEMU)
**Objetivo:** Testes não são apenas unitários no host; bootam o kernel em QEMU.

**Locais:**
- `tests/` — testes de integração que usam QEMU para bootar o kernel

**Critérios de aceitação:**
- `cargo test` (ou `make test`) roda testes que iniciam o kernel em QEMU
- Syscalls de borda são testadas end-to-end

---

### 6.3. Fuzzing de syscalls
**Objetivo:** Encontrar crashes/bugs em handlers de syscall com entradas aleatórias.

**Locais:**
- `tests/fuzz/` ou script separado — gera syscalls aleatórias com argumentos inválidos

**Critérios de aceitação:**
- Fuzzer roda por tempo definido sem kernel panic
- Entradas inválidas retornam erro apropriado, não travam o sistema

---

### 6.4. Miri para unsafe
**Objetivo:** Detectar comportamento indefinido em blocos `unsafe`.

**Locais:**
- CI — `cargo miri test` nos testes de unidade

**Critérios de aceitação:**
- Testes de unidade passam sob Miri sem UB detectado

---

### 6.5. Documentação interna (remover allow(missing_docs))
**Objetivo:** Todo item público tem documentação.

**Locais:**
- Todo `src/` — remover `#![allow(missing_docs)]` e adicionar documentação

**Critérios de aceitação:**
- `cargo doc` não gera warnings de documentação faltante

---

## Ordem sugerida de execução

### Fase 1 — Segurança rápida (baixo risco, alto impacto)
1.1 Stack canaries
1.2 Controle de acesso em superfícies (tokens)
1.4 SMEP/SMAP
6.5 Documentação

### Fase 2 — Memória e processos (médio risco)
2.1 COW para fork
2.2 mmap com arquivo
2.3 OOM killer / falha graciosa
2.4 Pilhas de kernel dinâmicas
2.5 Mais processos/threads

### Fase 3 — Sistema de arquivos (médio/alto risco)
3.1 Symlinks/hardlinks
3.2 Metadados + quota
3.3 pivot_root + mount table
3.4 Driver DMA/LBA48
3.5 FAT/ext2
3.6 /dev real

### Fase 4 — Userspace (depende do kernel estável)
4.1 dnu-std
4.2 Shell + init
4.3 ws_server

### Fase 5 — SMP e hardware (alto risco)
5.1 Spinlocks + per-CPU
5.2 APIC/x2APIC + IPIs
5.3 ACPI
5.4 PCI enumeration
5.5 Rede

### Fase 6 — Qualidade e CI (contínuo)
6.1 CI QEMU
6.2 Testes de integração QEMU
6.3 Fuzzing
6.4 Miri

---

## Como usar este plano

Cada peça acima pode ser implementada isoladamente. Recomendo começar por **1.1 (stack canaries)** ou **2.3 (OOM killer)**, que são isoladas e não dependem de infraestrutura complexa.

Quando estiver pronto para implementar uma peça específica, me avise e eu gero o prompt detalhado para aquela peça (no mesmo formato das peças críticas/altas/médias já implementadas).
