//! Syscalls de thread, relógio, espera e memória mapeada.
//!
//! Estas são as primitivas que a biblioteca padrão de Rust (e qualquer runtime
//! estilo `std`) precisa para existir — sem elas não existe `std::thread`,
//! `std::sync`, `Instant`, `sleep` nem um `GlobalAlloc` que devolva memória:
//!
//! * `thread_create`/`thread_exit`/`gettid` — threads que compartilham memória
//!   e descritores com o processo, com pilha e TLS próprios;
//! * `futex` — espera/notificação usada por `Mutex`, `Condvar` e `park`;
//! * `nanosleep` — suspensão temporizada;
//! * `clock_gettime` — relógio monótono e de tempo real;
//! * `mmap`/`munmap` — memória anônima mapeada sob demanda.
//!
//! ## Modelo de thread
//!
//! O escalonador já é de *threads*: cada slot tem `pid` (tid), `tgid`, pilha de
//! kernel e estado FPU. Até agora, porém, `fork`/`spawn` sempre criavam um
//! `tgid` novo. `thread_create` reaproveita essa estrutura: cria um slot com o
//! **mesmo `tgid`** do chamador (e o mesmo espaço de endereço, sem copiar
//! páginas) e um `TrapFrame` que entra em `entry` em ring 3.
//!
//! Não há `fork` de threads: `execve` recusa um slot cujo `tgid` pertence a um
//! grupo com mais de uma thread viva (não implementamos o `exec` multithread).
//!
//! ## Modelo de futex
//!
//! O kernel guarda por thread o endereço da palavra (`futex_address`) e o
//! conjunto de bits (`futex_bitset`). O escalonador só considera elegível uma
//! thread com `futex_address == 0`, e `FUTEX_WAKE` limpa esse campo. A espera é
//! **revalidada** no kernel: `FUTEX_WAIT` relê a palavra (atômica `cmpxchg` no
//! endereço do usuário) e só bloqueia se o valor ainda for o esperado; se já
//! mudou, devolve `EAGAIN` como o Linux. Isso fecha a corrida entre a leitura
//! no userspace e o bloqueio.
//!
//! ## Modelo de `mmap`
//!
//! Cada processo tem uma tabela fixa (`MAX_MAPPINGS`) de faixas anônimas. O
//! alocador distribui faixas crescentes a partir de `MMAP_REGION_BASE`, sem
//! reutilizar buracos: `GlobalAlloc` da `std` sempre devolve o bloco inteiro
//! para o sistema antes de pedir outro, e um alocador *first-fit* com lista de
//! buracos seria mais código do que o necessário para a primeira versão. As
//! páginas são mapeadas sob demanda (touch → `#[page_fault]`), então mapear
//! 1 MiB não consome 1 MiB de RAM até ser escrito.

use core::sync::atomic::{AtomicU64, Ordering};

use imagineos_abi::{
    CLOCK_MONOTONIC, CLOCK_REALTIME, FUTEX_WAIT, FUTEX_WAIT_BITSET, FUTEX_WAKE, FUTEX_WAKE_BITSET,
    MAP_ANONYMOUS, MAP_FIXED, MMAP_MAX_LENGTH, MMAP_REGION_BASE, MMAP_REGION_END, PAGE_SIZE,
    PROT_EXEC, PROT_READ, PROT_WRITE, ThreadCreate, Timespec,
};

use super::trap::{EAGAIN, EFAULT, EINVAL, ENOMEM, ENOSYS, TrapFrame};
use super::user::{copy_struct_from_user, copy_struct_to_user};
use crate::exec::process::{self, Mapping};

// ---------------------------------------------------------------------------
// Utilitários de tempo
// ---------------------------------------------------------------------------

/// Microssegundos desde o boot (relógio monótono), ou `None` se o TSC não foi
/// calibrado. `nanosleep` e os timeouts de futex dependem dele.
fn monotonic_microseconds() -> Option<u64> {
    crate::time::elapsed_microseconds()
}

/// Convert (`seconds`, `nanoseconds`) into microseconds, saturating at `u64::MAX`.
///
/// `seconds` may be negative for `CLOCK_REALTIME`/`FUTEX_WAIT_BITSET` absolute
/// deadlines on a system whose wall clock starts near the epoch; those are
/// clamped to 0 (already expired) instead of wrapping around.
fn to_microseconds(seconds: i64, nanoseconds: i64) -> u64 {
    if seconds < 0 || nanoseconds < 0 {
        return 0;
    }
    let whole_seconds = (seconds as u64).saturating_mul(1_000_000);
    // `1_500_500 ns` → `1_500 µs` (truncamento para baixo, como a conversão
    // `Timespec` → µs do POSIX). A conta em `u128` evita estouro no produto.
    let fraction = ((nanoseconds as u128) / 1_000) as u64;
    whole_seconds.saturating_add(fraction)
}

/// Sorteia um deadline para a thread atual e devolve quantos µs faltam.
///
/// Grava `wake_deadline` no processo atual; o escalonador acorda a thread quando
/// o TSC alcança o prazo. Devolve `Ok(micros)` (0 = prazo já vencido) ou
/// `Err` com o errno da ABI.
fn set_wake_deadline(duration_micros: u64) -> Result<u64, i64> {
    let now = monotonic_microseconds().ok_or(-5 /* EIO */)?;
    let deadline = now.saturating_add(duration_micros);
    process::set_current_wake_deadline(Some(deadline));
    Ok(duration_micros)
}

// ---------------------------------------------------------------------------
// Syscalls de relógio e espera
// ---------------------------------------------------------------------------

/// `clock_gettime(clockid, &Timespec) -> 0`.
pub fn clock_gettime(frame: &mut TrapFrame) -> *mut TrapFrame {
    let clock = frame.rdi;
    if clock != CLOCK_MONOTONIC && clock != CLOCK_REALTIME {
        return frame.fail(EINVAL);
    }
    let Some(microseconds) = monotonic_microseconds() else {
        return frame.fail(-5 /* EIO */);
    };
    let value = Timespec {
        seconds: (microseconds / 1_000_000) as i64,
        nanoseconds: ((microseconds % 1_000_000) * 1_000) as i64,
    };
    if copy_struct_to_user(frame.rsi, &value) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `nanosleep(&Timespec, &Timespec) -> 0`.
///
/// A semântica é a **primeira tentativa**: o kernel bloqueia a thread até o
/// prazo (o escalonador a acorda), devolve `0` quando o prazo vence e `EINTR`
/// se um sinal chegar antes. O segundo argumento (tempo restante) não é
/// preenchido: o chamador que precisa dele (loop de `std::thread::sleep`) já
/// repete a chamada quando recebe `EINTR`, e a `std` descarta o resto.
///
/// A espera não é implementada como laço no kernel (como o `waitpid` faz): o
/// `TrapFrame` é devolvido ao userspace com `RAX` já definido, e o laço entre
/// `nanosleep` e `yield` fica no runtime. Isso mantém o kernel simples — a
/// pilha de kernel nunca fica suspensa no meio de uma syscall.
pub fn nanosleep(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut request = Timespec::default();
    if !copy_struct_from_user(frame.rdi, &mut request) {
        return frame.fail(EFAULT);
    }
    if request.seconds < 0 || request.nanoseconds < 0 || request.nanoseconds >= 1_000_000_000 {
        return frame.fail(EINVAL);
    }
    let duration = to_microseconds(request.seconds, request.nanoseconds);
    if duration == 0 {
        // `nanosleep(0)` ainda é um ponto de cessão: o POSIX permite que um
        // `yield` ocorra, e um sleep de 0 ns sem `yield` vira um busy-loop.
        return frame.ok(0);
    }
    if monotonic_microseconds().is_none() {
        return frame.fail(ENOSYS);
    }
    match set_wake_deadline(duration) {
        Ok(_) => {
            // `block_current` devolve o frame da próxima thread. Se a própria
            // thread voltar (ninguém mais elegível), o resultado já é `0`
            // porque o slot não é `None` — ela reexecuta o laço no runtime.
            let (next, _switched) = process::block_current(frame, true);
            next
        }
        Err(error) => frame.fail(error),
    }
}

// ---------------------------------------------------------------------------
// futex
// ---------------------------------------------------------------------------

/// Lê a palavra do usuário e devolve `Some(current)` só se ela ainda vale
/// `expected`. A leitura é atômica com a intenção de bloquear: nenhuma outra
/// thread pode intercalar um `store` entre a checagem e o bloqueio, porque o
/// kernel roda com as interrupções desligadas no caminho da syscall.
fn compare_and_stay(address: u64, expected: u32) -> Option<u32> {
    if address == 0 || address & 3 != 0 {
        return None;
    }
    let mut current = expected;
    if !process::copy_from_current_user(address, unsafe {
        core::slice::from_raw_parts_mut((&mut current as *mut u32).cast::<u8>(), 4)
    }) {
        return None;
    }
    if current != expected {
        return None;
    }
    Some(current)
}

/// `futex(&word, op, value, &timeout, ...) -> 0 | -EAGAIN | -ETIMEDOUT`.
///
/// `op` sem `FUTEX_WAIT`/`FUTEX_WAKE`: os bits `FUTEX_*_BITSET` e o bit
/// `FUTEX_CLOCK_REALTIME` (0x100) são aceitos, mas o relógio usado é sempre o
/// monótono — é o único que o kernel mantém.
pub fn futex(frame: &mut TrapFrame) -> *mut TrapFrame {
    let address = frame.rdi;
    let operation = frame.rsi & 0x7f;
    let value = frame.rdx as u32;

    match operation {
        FUTEX_WAIT | FUTEX_WAIT_BITSET => {
            if address & 3 != 0 || address == 0 {
                return frame.fail(EINVAL);
            }
            // Timeout: `&Timespec` em `r10`. Sem timeout = espera indefinida.
            let timeout_pointer = frame.r10;
            let timeout = if timeout_pointer == 0 {
                None
            } else {
                let mut spec = Timespec::default();
                if !copy_struct_from_user(timeout_pointer, &mut spec) {
                    return frame.fail(EFAULT);
                }
                if spec.seconds < 0 || spec.nanoseconds < 0 || spec.nanoseconds >= 1_000_000_000 {
                    return frame.fail(EINVAL);
                }
                // `WAIT_BITSET` usa `r8` como máscara de bits; para o kernel só
                // importa que a palavra seja a mesma, mas guardar a máscara
                // permite um `FUTEX_WAKE_BITSET` direcionado no futuro.
                let _ = frame.r8;
                Some(to_microseconds(spec.seconds, spec.nanoseconds))
            };
            // Revalidação: se a palavra deixou de valer `expected`, alguém já
            // mudou a condição e avisou (ou vai avisar); não perdemos o wake.
            if compare_and_stay(address, value).is_none() {
                return frame.fail(EAGAIN);
            }
            process::set_current_futex(address, frame.r8 as u64);
            match timeout {
                None => {
                    let (next, switched) = process::block_current(frame, false);
                    if !switched {
                        // Nenhuma outra thread elegível: não há como dormir sem
                        // perder o evento, então a espera termina como
                        // interrompida e o runtime revalida a palavra.
                        process::set_current_futex(0, 0);
                        unsafe {
                            (*next).rax = EAGAIN as u64;
                        }
                    }
                    next
                }
                Some(micros) => match set_wake_deadline(micros) {
                    Ok(_) => {
                        let (next, _) = process::block_current(frame, true);
                        next
                    }
                    Err(error) => frame.fail(error),
                },
            }
        }
        FUTEX_WAKE | FUTEX_WAKE_BITSET => {
            if address & 3 != 0 || address == 0 {
                return frame.fail(EINVAL);
            }
            let bitset = if operation == FUTEX_WAKE_BITSET {
                frame.r8 as u64
            } else {
                imagineos_abi::FUTEX_BITSET_MATCH_ANY
            };
            // No Linux, `FUTEX_WAKE` com `val == 0` acorda **todas** as threads
            // da palavra; `usize::MAX` traduz isso para o escalonador.
            let count = match value as usize {
                0 => usize::MAX,
                count => count,
            };
            let woken = process::wake_futex(address, count, bitset);
            frame.ok(woken as u64)
        }
        _ => frame.fail(ENOSYS),
    }
}

// ---------------------------------------------------------------------------
// Threads
// ---------------------------------------------------------------------------

/// `gettid() -> tid`: identificador da **thread** atual.
///
/// `getpid` devolve o `tgid` (identidade do processo); `gettid` devolve o `pid`
/// do slot, que é único por thread.
pub fn gettid(frame: &mut TrapFrame) -> *mut TrapFrame {
    frame.ok(process::current_tid() as u64)
}

/// `thread_create(&ThreadCreate) -> tid`.
///
/// Valida o pedido (pilha dentro do espaço do processo, entrada executável) e
/// entrega a criação ao escalonador. A thread nova não roda até a atual ceder a
/// CPU (`yield`, bloqueio ou preempção), o que dá ao userspace o tempo de
/// completar a inicialização do TLS antes que a thread nova comece.
pub fn thread_create(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut request = ThreadCreate::default();
    if !copy_struct_from_user(frame.rdi, &mut request) {
        return frame.fail(EFAULT);
    }
    if request.entry == 0 {
        return frame.fail(EINVAL);
    }
    // A entrada precisa estar em página executável. O `AddressSpace` não guarda
    // o bit de execução da PTE, então a verificação honesta que existe hoje é
    // "está mapeada e é de usuário" — o loader ainda não distingue RX de RW.
    let entry_mapped =
        process::with_current_space(|space| space.translate(request.entry).is_some());
    if !entry_mapped {
        return frame.fail(EFAULT);
    }
    match process::create_thread(request, frame) {
        Ok(tid) => frame.ok(tid as u64),
        Err(error) => frame.fail(error),
    }
}

/// `set_tls(base) -> 0`: configura a base do TLS (`IA32_FS_BASE`) da thread
/// atual.
///
/// É o equivalente do `arch_prctl(ARCH_SET_FS)` do Linux — a chamada que a
/// musl faz em `__set_thread_area` antes de qualquer `pthread_self()` ou
/// `errno`. O valor vai para o `fs_base` da thread e é reprogramado a cada
/// troca de contexto (`process::restore_fpu_for`/`schedule`).
///
/// `base` é usada como ponteiro (`fs:0` precisa ser legível pelo userspace),
/// mas não validamos o mapeamento: o primeiro acesso do runtime a `%fs:0`
/// causa o `#PF` normal se o endereço for inválido.
pub fn set_tls(frame: &mut TrapFrame) -> *mut TrapFrame {
    process::set_current_tls(frame.rdi);
    frame.ok(0)
}

/// `tls_info(&TlsInfo) -> 0`: descreve o bloco TLS montado pelo kernel.
///
/// A musl usa `size` para calcular quanto espaço cada thread precisa ao ser
/// criada (`__copy_tls`); `fs_base` é informativo. Binários sem TLS devolvem
/// `size == 0` e a musl funciona sem `pthread_create` de TLS.
pub fn tls_info(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (fs_base, size, alignment) = process::current_tls_info();
    let info = imagineos_abi::TlsInfo {
        fs_base,
        size,
        alignment,
    };
    if copy_struct_to_user(frame.rdi, &info) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `thread_exit(status) -> !`.
///
/// Encerra apenas a thread atual. Se ela era a última do processo, todo o
/// processo termina com `status` (o `exit` normal). Caso contrário o slot é
/// liberado imediatamente e as demais threads continuam.
pub fn thread_exit(frame: &mut TrapFrame) -> *mut TrapFrame {
    process::exit_thread(frame, frame.rdi as i32)
}

// ---------------------------------------------------------------------------
// memória mapeada
// ---------------------------------------------------------------------------

static NEXT_MMAP_ADDRESS: AtomicU64 = AtomicU64::new(MMAP_REGION_BASE);

/// Arredonda `length` para cima em páginas, recusando zero e valores absurdos.
fn page_rounded(length: u64) -> Option<u64> {
    if length == 0 || length > MMAP_MAX_LENGTH {
        return None;
    }
    let pages = length.checked_add(PAGE_SIZE - 1)? / PAGE_SIZE;
    pages.checked_mul(PAGE_SIZE)
}

/// `mmap(address, length, protection, flags) -> address`.
///
/// Só memória anônima (`MAP_ANONYMOUS`) e privada (`MAP_PRIVATE`) é suportada:
/// mapear arquivo exigiria um VFS com `mmap` por backend, que ainda não existe
/// (e a `std` não precisa para heap nem para pilhas de thread). Com `MAP_FIXED`
/// o endereço pedido é honrado ou a chamada falha com `ENOMEM`.
pub fn mmap(frame: &mut TrapFrame) -> *mut TrapFrame {
    let requested = frame.rdi;
    let length = frame.rsi;
    let protection = frame.rdx;
    let flags = frame.r10;

    // A ABI não tem bit `MAP_SHARED`: mapeamento compartilhado (SysV) ainda não
    // é implementado. Recusar explicitamente é melhor que entregar memória
    // privada com semântica falsa.
    if flags & MAP_ANONYMOUS == 0 {
        return frame.fail(ENOSYS);
    }
    if protection & !(PROT_READ | PROT_WRITE | PROT_EXEC) != 0
        || protection & (PROT_READ | PROT_WRITE | PROT_EXEC) == 0
    {
        // `PROT_NONE` e bits desconhecidos não são suportados pelo `AddressSpace`
        // (não há como gravar uma PTE "não presente mas válida").
        return frame.fail(EINVAL);
    }
    // Página só de execução não existe neste hardware (NX não é configurado):
    // `PROT_EXEC` sem leitura continua legível. Fora isso, a única distinção que
    // importa é gravável ou não.
    let _ = PROT_EXEC;
    let Some(size) = page_rounded(length) else {
        return frame.fail(EINVAL);
    };

    // `MAP_FIXED` sem `MAP_PRIVATE` é válido (o Linux aceita as duas combinações
    // para anônimo); o bit só decide se o endereço é honrado.
    let fixed = flags & MAP_FIXED != 0;
    let address = if fixed {
        if requested & (PAGE_SIZE - 1) != 0
            || !(MMAP_REGION_BASE..MMAP_REGION_END).contains(&requested)
            || requested
                .checked_add(size)
                .is_none_or(|end| end > MMAP_REGION_END)
        {
            return frame.fail(EINVAL);
        }
        requested
    } else {
        match allocate_address(size) {
            Some(address) => address,
            None => return frame.fail(ENOMEM),
        }
    };

    // `MAP_FIXED` substitui tudo o que existir na faixa. A remoção vem antes
    // do registro da nova VMA: ao contrário, `add_mapping` recusaria a faixa
    // por ela ainda constar na tabela. As páginas já populadas também caem, o
    // que é a semântica de `MAP_FIXED` (a faixa é descartada e recriada).
    if fixed {
        process::remove_mapping_range(address, size);
    }

    if !process::add_mapping(Mapping {
        start: address,
        length: size,
        writable: protection & PROT_WRITE != 0,
    }) {
        return frame.fail(ENOMEM);
    }
    frame.ok(address)
}

/// `munmap(address, length) -> 0`.
pub fn munmap(frame: &mut TrapFrame) -> *mut TrapFrame {
    let address = frame.rdi;
    let length = frame.rsi;
    if address & (PAGE_SIZE - 1) != 0 || !(MMAP_REGION_BASE..MMAP_REGION_END).contains(&address) {
        return frame.fail(EINVAL);
    }
    // `munmap(…, 0)` é `EINVAL` no POSIX, mas um no-op é mais tolerante com um
    // `Drop` que zera o tamanho antes de liberar.
    if length == 0 {
        return frame.ok(0);
    }
    let Some(size) = page_rounded(length) else {
        return frame.fail(EINVAL);
    };
    let Some(end) = address.checked_add(size) else {
        return frame.fail(EINVAL);
    };
    if end > MMAP_REGION_END {
        return frame.fail(EINVAL);
    }
    process::remove_mapping_range(address, size);
    frame.ok(0)
}

/// Próximo endereço livre na região de `mmap`, respeitando as faixas vivas do
/// processo atual.
///
/// O contador é global e só avança; uma faixa liberada é reutilizada apenas se
/// o `MAP_FIXED` do chamador pedir o endereço. Isso mantém o alocador O(1) e é
/// suficiente para o uso real (`GlobalAlloc` de grow-only com `dealloc`).
fn allocate_address(size: u64) -> Option<u64> {
    let mut candidate = NEXT_MMAP_ADDRESS.load(Ordering::Relaxed);
    loop {
        if candidate.checked_add(size)? > MMAP_REGION_END {
            return None;
        }
        let next = process::next_free_mapping_address(candidate, size)?;
        if next == candidate {
            let end = candidate.checked_add(size)?;
            // Só publica o novo cursor se ninguém o tiver movido no meio; o
            // kernel é de uma CPU, então isto é um CAS de consistência.
            if NEXT_MMAP_ADDRESS
                .compare_exchange(candidate, end, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Some(candidate);
            }
            candidate = NEXT_MMAP_ADDRESS.load(Ordering::Relaxed);
        } else {
            candidate = next;
        }
    }
}

/// Usado pelos testes de host: reinicia o cursor global entre casos.
#[cfg(test)]
pub fn reset_mmap_allocator_for_tests() {
    NEXT_MMAP_ADDRESS.store(MMAP_REGION_BASE, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_microseconds_clamps_negative_times() {
        assert_eq!(to_microseconds(-1, 0), 0);
        assert_eq!(to_microseconds(0, -1), 0);
        // 1_500_500 ns = 1,5 ms; `500 ns` sozinho truncaria para 0 µs.
        assert_eq!(to_microseconds(0, 1_500_500), 1_500);
        assert_eq!(to_microseconds(2, 750_000_000), 2_750_000);
    }

    #[test]
    fn page_rounding_rejects_zero_and_oversize() {
        assert_eq!(page_rounded(0), None);
        assert_eq!(page_rounded(MMAP_MAX_LENGTH + 1), None);
        assert_eq!(page_rounded(1), Some(PAGE_SIZE));
        assert_eq!(page_rounded(PAGE_SIZE), Some(PAGE_SIZE));
        assert_eq!(page_rounded(PAGE_SIZE + 1), Some(PAGE_SIZE * 2));
    }

    #[test]
    fn map_error_is_reported_like_enomem() {
        // Documenta o contrato que `mmap` usa: falha de página => `ENOMEM`.
        let _ = MapError::OutOfMemory;
        assert_eq!(ENOMEM, -12);
    }

    #[test]
    fn futex_constants_are_the_linux_ones() {
        assert_eq!(FUTEX_WAIT, 0);
        assert_eq!(FUTEX_WAKE, 1);
        assert_eq!(FUTEX_WAIT_BITSET, 9);
        assert_eq!(FUTEX_WAKE_BITSET, 10);
        assert_eq!(EINTR, -4);
        assert_eq!(ETIMEDOUT, -110);
    }
}
