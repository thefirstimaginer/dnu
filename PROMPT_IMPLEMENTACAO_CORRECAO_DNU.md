# Prompt: Correção de Problemas Críticos, Altos e Médios — DNU Kernel

**Contexto:** Kernel monolítico Rust (nightly, `x86_64-unknown-none`), ~23 mil LOC, ~500 blocos `unsafe`. Boot via Limine e caminho `direct_start` (dnuboot). Carrega imagens DEF/dzImage.

**Restrições gerais:** não quebrar ABI pública (`abi/`), manter suporte a Limine e `direct_start`, não remover `unsafe` onde for estritamente necessário para hardware, manter boot em QEMU e carga de `/sbin/init` do initramfs. Todo bloco `unsafe` alterado deve ter comentário explicando invariantes assumidos.

---

## Peça 1 — Crítico A: ALLOCATOR_PTR em armazenamento estático

**Locais:** `src/main.rs` (linhas 14, 163-166, 203-205), `src/mm/paging.rs` (funções `map_hhdm_range`, `fix_kernel_page_table_entries`, `table_pointer` leem `ALLOCATOR_PTR` via `ptr::read`).

**Problema:** `FrameAllocator` é alocado na pilha de `kernel_entry` e seu endereço é gravado em `static mut ALLOCATOR_PTR`. Ponteiro pendurado latente.

**Solução:**
- Substituir `static mut ALLOCATOR_PTR: *mut FrameAllocator` por `static mut ALLOCATOR: MaybeUninit<FrameAllocator>` (ou `Option<FrameAllocator>`) em `.data`.
- Inicializar antes de qualquer uso de paginação.
- Todas as funções que leram `ALLOCATOR_PTR` passam a ler do `ALLOCATOR` estático.

**Critério de aceitação:** Nenhum endereço de pilha é armazenado como localizador do alocador. Kernel boota e aloca frames normalmente.

---

## Peça 2 — Crítico B: Remover pulo de instrução em ring 3

**Local:** `src/arch/idt.rs`, função `dreamcore_exception_handler` (linhas ~472-502).

**Problema:** Page fault de instrução com `cr2 < 0x400000` e GP fault (vetor 13) em ring 3 fazem `rip += 2` e retomam com `iretq`. Permite fault injection.

**Solução:** Remover os blocos que modificam `rip` e retomam. Exceções em ring 3 devem terminar o processo (entregar `SIGSEGV`/`SIGILL`) via `terminate_faulting_process()`, nunca modificar `rip`.

**Critério de aceitação:** Processo userspace com instrução inválida ou acesso proibido é terminado; `rip` nunca é alterado pelo handler para retomar execução.

---

## Peça 3 — Crítico C: W^X estrito em `map_user_page_with`

**Local:** `src/mm/paging.rs`, função `map_user_page_with` (linhas ~121-133).

**Problema:** Ao remapear página já executável, o código remove `ENTRY_NX` ao adicionar escrita:
```rust
if *entry & ENTRY_NX == 0 { new_bits &= !ENTRY_NX; }
```
Resulta em páginas RWX.

**Solução:** Nunca remover `ENTRY_NX` ao adicionar escrita. Se a página já é executável, remapear com escrita deve falhar ou manter `NX`. Rejeitar explicitamente combinações `RW` + `X` em userspace.

**Critério de aceitação:** Nenhuma página de usuário pode ser simultaneamente gravável e executável.

---

## Peça 4 — Crítico D: dzImage verify no `direct_start` + memory map válido

**Local:** `src/boot/boot.rs`, função `direct_start` (linhas ~205-258).

**Problemas:** Não chama `dzimage::verify`. Passa `memory_entries = null` e `count = 0`, então o alocador não encontra regiões e o kernel trava no primeiro `allocate_frame`.

**Solução:** Chamar `dzimage::verify` (magic, versão, ABI, CRC-32) antes de pular para `kernel_entry`. Preencher memory map válido (ou falhar com mensagem clara) em vez de passar null/0.

**Critério de aceitação:** Boot via `dnuboot` verifica integridade do dzImage e não trava por falta de memory map.

---

## Peça 5 — Alta E: ALLOCATED_BYTES contabilizado corretamente

**Local:** `src/mm/memory.rs`.

**Problema:** `ALLOCATED_BYTES` inicializado em 0 e nunca incrementado. `SystemInfo.memory_total` reporta apenas memória livre.

**Solução:** Incrementar `ALLOCATED_BYTES` em alocações e decrementar em liberações, ou remover a métrica de `memory_total`.

**Critério de aceitação:** `SystemInfo.memory_total` reflete memória total física gerenciada.

---

## Peça 6 — Alta F: Limites do alocador aumentados

**Local:** `src/mm/memory.rs` (`MAX_REGIONS = 4`, `MAX_FREE_FRAMES = 4`).

**Problema:** `free_frame` descarta silenciosamente quando o cache de 4 está cheio. Apenas 4 regiões do memory map são aproveitadas.

**Solução:** Aumentar `MAX_FREE_FRAMES` e `MAX_REGIONS` para valores razoáveis baseados no memory map, ou usar estrutura dinâmica escalável.

**Critério de aceitação:** Nenhum frame vazado por cheio de cache; todas as regiões usáveis do memory map são registradas.

---

## Peça 7 — Alta G: Scheduler sem `kernel_halt` eterno

**Local:** `src/process/scheduler.rs` (linha ~1291, `schedule()`).

**Problema:** `schedule()` chama `crate::kernel_halt()` quando não há candidato. PID 1 saindo trava o sistema.

**Solução:** `schedule()` escolhe outro candidato ou, como último recurso, faz pânico controlado com dump, não `hlt` infinito. Revisar `any_waiting()` para não bloquear preempção de timer indevidamente.

**Critério de aceitação:** Sistema não entra em `hlt` infinito sem candidato; PID 1 saindo resulta em pânico controlado ou reboot limpo.

---

## Peça 8 — Alta H: TSC/monotônico sem travamento

**Local:** `src/time/time.rs` e chamadores de `nanosleep`/`futex` com timeout.

**Problema:** Se frequência do TSC não calibrada, `monotonic_microseconds()` fica em 0 → `idle_until_next_deadline()` faz loop `hlt` eterno.

**Solução:** Se frequência não calibrada, `nanosleep`/`futex` com timeout retornam erro (ou usam PIT como fallback) em vez de travar em `hlt`.

**Critério de aceitação:** `nanosleep`/`futex` com timeout não travam o sistema quando TSC não calibrado.

---

## Peça 9 — Alta I: Timer/PIC validado após `mask_legacy_pic`

**Local:** `src/boot/boot.rs` (`mask_legacy_pic`), `src/time/time.rs` (`configure_timer_interrupt`).

**Problema:** `mask_legacy_pic()` desativa todas as linhas do 8259. Se o timer PIT não for reprogramado, interrupções de timer nunca disparam.

**Solução:** Garantir que ticks de timer disparam após `mask_legacy_pic()`. Adicionar diagnóstico de falha de timer no boot.

**Critério de aceitação:** Ticks de timer funcionam após boot; diagnóstico avisa se timer não estiver disparando.

---

## Peça 10 — Média J: Auditoria de `unsafe`

**Local:** Todo o crate raiz (`src/main.rs`, `src/lib.rs`).

**Solução:** Ativar `#![deny(unsafe_op_in_unsafe_fn)]`. Remover `allow(unsafe_op_in_unsafe_fn)` e `allow(missing_docs)` de `main.rs`. Documentar cada bloco `unsafe`. Reduzir `unsafe impl Sync` onde atomicos bastem.

**Critério de aceitação:** Compila com `#![deny(unsafe_op_in_unsafe_fn)]` sem erros; cada `unsafe` tem documentação.

---

## Peça 11 — Média K: `rust-toolchain.toml`

**Local:** Raiz do repositório.

**Solução:** Adicionar `rust-toolchain.toml` pinando a versão nightly usada.

**Critério de aceitação:** `rust-toolchain.toml` presente e build reproduzível com a toolchain fixada.

---

## Peça 12 — Média L: Testes integrados ao `cargo test` + testes de fronteira

**Local:** `tests/` e `src/`.

**Solução:** Integrar testes autônomos (`rustc --test`) ao harness do Cargo. Adicionar testes de syscall de fronteira: endereços inválidos, tamanhos excessivos, tentativas de mapeamento RWX, page fault em ring 3.

**Critério de aceitação:** `cargo test` executa a suíte; novos testes de fronteira cobrem W^X, endereços inválidos e comportamento de exceções em userspace.

---

## Ordem de execução sugerida

1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12

Não implementar funcionalidades do roadmap futuro (ASLR, SMP, KPTI, assinatura criptográfica, etc.). Apenas estabilizar e tornar seguro o que já existe.
