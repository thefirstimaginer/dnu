//! Números das syscalls do contrato `int 0x80` do Dreamcore.
//!
//! Formato dos argumentos (64 bits): `rax` = número, `rdi`, `rsi`, `rdx`,
//! `r10`, `r8`, `r9` = argumentos 1..6. O `rcx` **não** carrega argumento: a
//! instrução `int` grava nele o endereço de retorno em ring 3, por isso o
//! quarto parâmetro viaja em `r10`.

pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const SYS_EXIT: u64 = 4;
pub const SYS_GETPID: u64 = 5;
pub const SYS_CLEAR: u64 = 6;
pub const SYS_EXEC: u64 = 7;
pub const SYS_ISDIR: u64 = 8;
pub const SYS_ISFILE: u64 = 9;
pub const SYS_READ_FILE: u64 = 10;
pub const SYS_READDIR: u64 = 11;
pub const SYS_MKDIR: u64 = 12;
pub const SYS_TOUCH: u64 = 13;
pub const SYS_REMOVE: u64 = 14;
pub const SYS_WRITE_FILE: u64 = 15;
pub const SYS_ABI_VERSION: u64 = 16;
pub const SYS_OPEN: u64 = 17;
pub const SYS_READ_FD: u64 = 18;
pub const SYS_WRITE_FD: u64 = 19;
pub const SYS_CLOSE: u64 = 20;
pub const SYS_DISK_COUNT: u64 = 21;
pub const SYS_DISK_SECTORS: u64 = 22;
pub const SYS_INSTALL_DISK: u64 = 23;
pub const SYS_STAT: u64 = 24;
pub const SYS_DMESG: u64 = 25;
pub const SYS_SHUTDOWN_REQUEST: u64 = 26;
pub const SYS_SHUTDOWN_POLL: u64 = 27;
pub const SYS_POWER_OFF: u64 = 28;
pub const SYS_KILL: u64 = 29;
pub const SYS_SIGACTION: u64 = 30;
pub const SYS_SIGRETURN: u64 = 31;
pub const SYS_PROCESS_LIST: u64 = 32;
pub const SYS_AUTHENTICATE: u64 = 33;
pub const SYS_GETIDENTITY: u64 = 34;
pub const SYS_INSTALL_DISK_CONFIG: u64 = 35;
pub const SYS_SBRK: u64 = 36;
pub const SYS_LSEEK: u64 = 37;
pub const SYS_FSTAT: u64 = 38;
pub const SYS_GETTIMEOFDAY: u64 = 39;
// Primitivas de processo e IPC para shells e pipelines no estilo POSIX.
pub const SYS_DUP: u64 = 40;
pub const SYS_DUP2: u64 = 41;
pub const SYS_PIPE: u64 = 42;
pub const SYS_SPAWN: u64 = 43;
pub const SYS_WAIT: u64 = 44;
/// `fork(2)`: duplica o processo atual. Devolve 0 no filho, o pid no pai.
pub const SYS_FORK: u64 = 45;
/// `waitpid(pid, &status, flags)`: espera um filho e coleta o status.
pub const SYS_WAITPID: u64 = 46;
/// `execve(path, argv, envp)`: substitui a imagem do processo atual.
pub const SYS_EXECVE: u64 = 47;
// Primitivas de display: um servidor de janelas em userspace precisa enxergar
// o framebuffer e receber os eventos de entrada.
/// `fb_info(&FramebufferInfo)`: descreve o framebuffer ativo.
pub const SYS_FB_INFO: u64 = 48;
/// `fb_map(&FramebufferInfo)`: mapeia o framebuffer no processo atual.
pub const SYS_FB_MAP: u64 = 49;
/// `input_poll(&InputEvent)`: retira um evento de entrada (0 se não houver).
pub const SYS_INPUT_POLL: u64 = 50;
/// `console_release()`: o servidor de display assume a tela e o console do
/// kernel deixa de desenhar nela.
pub const SYS_CONSOLE_RELEASE: u64 = 51;
/// `pointer_position()`: devolve `(x << 32) | y` do ponteiro, para o servidor
/// de display desenhar o cursor sem recontar cada movimento.
pub const SYS_POINTER_POSITION: u64 = 52;
// Protocolo de superfícies (janelas cliente/servidor):
// um processo qualquer pode registrar uma superfície de pixels e o servidor
// de display compõe na tela e encaminha eventos. O dono é identificado pelo
// PID, então não há tokens nem permissões — como o X11 antes da autenticação.
/// `surface_create(&SurfaceInfo)`: registra uma superfície no kernel e mapeia
/// o buffer compartilhado no processo atual. `SurfaceInfo.address` volta com o
/// endereço virtual.
pub const SYS_SURFACE_CREATE: u64 = 53;
/// `surface_present(id, flags)`: marca a superfície como visível (1) ou
/// oculta (0) e pede recomposição. Devolve 0 em caso de sucesso.
pub const SYS_SURFACE_PRESENT: u64 = 54;
/// `surface_flush(id)`: publica um frame escrito no buffer; o compositor
/// passa a copiar da região mapeada. Sem esta chamada o cliente desenha
/// "no escuro" (o kernel só copia quando o frame está consistente).
pub const SYS_SURFACE_FLUSH: u64 = 55;
/// `surface_destroy(id)`: remove a superfície, desmapeia e libera o buffer.
pub const SYS_SURFACE_DESTROY: u64 = 56;
/// `surface_poll(&SurfaceEvent)`: retira um evento dirigido a uma superfície
/// deste processo (0 se não houver). O kernel roteia com base no dono.
pub const SYS_SURFACE_POLL: u64 = 57;
/// `surface_snapshot(&[SurfaceSnapshot]) -> count`: lista as superfícies
/// visíveis e mapeia os buffers no processo chamador (o servidor).
pub const SYS_SURFACE_SNAPSHOT: u64 = 58;
/// `surface_send_event(&SurfaceEvent)`: entrega um evento ao dono da
/// superfície indicada (o servidor encaminha teclas e cliques).
pub const SYS_SURFACE_SEND_EVENT: u64 = 59;
/// `pivot_root(new_root, put_old)`: troca a raiz do sistema (initramfs → raiz
/// real no disco). POSIX/Linux: o initramfs chama após montar a nova raiz.
pub const SYS_PIVOT_ROOT: u64 = 60;
/// `surface_move(id, x, y)`: o dono reposiciona a própria superfície na tela.
/// O display server compõe no novo endereço a partir do próximo quadro.
pub const SYS_SURFACE_MOVE: u64 = 61;
/// `system_info(&SystemInfo)`: retrato do sistema (uptime, RAM, resolução,
/// contagem de processos, prioridade do chamador). Usado por `systemfetch` e
/// `top`.
pub const SYS_SYSTEM_INFO: u64 = 62;

// ------------------------------------------------------------------------------
// Base para a biblioteca padrão (dnu-std) do userspace.
//
// Sem estas primitivas não há como construir uma `std` de Rust: `std::thread`
// precisa de `clone`/TLS, `std::sync` precisa de `futex`, `std::time::Instant`
// e `std::thread::sleep` precisam de um relógio monótono e de `nanosleep`, e o
// `alloc` global precisa de `mmap`/`munmap` para crescer e devolver memória.
// ------------------------------------------------------------------------------
/// `thread_create(&ThreadCreate) -> tid`: cria uma thread no espaço de endereço
/// do processo atual. A thread nova compartilha memória, descritores e
/// credenciais com o processo; cada thread tem sua própria pilha de kernel e de
/// usuário. Devolve o `tid` (igual ao `pid` para a thread principal) ou um
/// errno negativo.
///
/// A thread nova começa executando `ThreadCreate.entry`, com `ThreadCreate.argument`
/// em `rdi`. Ao retornar, chama `thread_exit(0)` implicitamente.
pub const SYS_THREAD_CREATE: u64 = 63;
/// `thread_exit(status) -> !`: encerra a thread atual. Se for a última thread do
/// processo, o processo inteiro termina com `status`.
pub const SYS_THREAD_EXIT: u64 = 64;
/// `gettid() -> tid`: identifica a thread atual (não confundir com `getpid`).
pub const SYS_GETTID: u64 = 65;
/// `futex(&word, op, value, &timeout) -> result`: primitiva de espera/notificação.
/// Base de `Mutex`, `Condvar` e dos `park`/`unpark` da `std`. Ver `FUTEX_*`.
pub const SYS_FUTEX: u64 = 66;
/// `nanosleep(&Timespec, &Timespec) -> 0`: suspende a thread até o intervalo
/// esgotar. Interrompido por sinal devolve `-EINTR`.
pub const SYS_NANOSLEEP: u64 = 67;
/// `clock_gettime(clockid, &Timespec) -> 0`: relógio monótono (`CLOCK_MONOTONIC`)
/// ou de tempo real (`CLOCK_REALTIME`). Base de `Instant`/`SystemTime`.
pub const SYS_CLOCK_GETTIME: u64 = 68;
/// `mmap(address, length, protection, flags) -> address`: mapeia páginas de
/// usuário sob demanda. Devolve o endereço base ou um errno negativo.
pub const SYS_MMAP: u64 = 69;
/// `munmap(address, length) -> 0`: desmapeia páginas e devolve os frames ao
/// alocador. O par `mmap`/`munmap` é o que permite ao `GlobalAlloc` da `std`
/// crescer **e** liberar memória (o `sbrk` atual só cresce).
pub const SYS_MUNMAP: u64 = 70;
/// `set_tls(base) -> 0`: define a base do TLS da thread atual
/// (`IA32_FS_BASE`). É o equivalente do `arch_prctl(ARCH_SET_FS)` do Linux e
/// o que a musl chama em `__set_thread_area`; sem ele a thread principal não
/// tem `errno` por thread nem `pthread_self()`.
pub const SYS_SET_TLS: u64 = 71;
/// `tls_info(&TlsInfo) -> 0`: descreve o bloco TLS que o kernel montou para o
/// processo (`fs_base` e `memory_size`). A musl precisa disto para calcular
/// quanta memória cada thread nova consome; um `no_std` simples pode ignorar.
pub const SYS_TLS_INFO: u64 = 72;
