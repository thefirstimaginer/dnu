#![no_std]

pub mod def;
pub mod syscall;

pub use syscall::*;

pub const ABI_VERSION: u64 = 1; // Não há porque mudar a versão da ABI a cada compilação,
                                // somente se houver transição de versão estável 
                                // (v1.x -> ABI 1, v2.x ABI 2)

// Limites compartilhados entre kernel, libc C (`dreamcore.h`) e runtime Rust.
// O kernel não pode aceitar pedidos maiores que estes, e nenhum deles pode
// divergir de `common/userland/libraries/libc/include/dreamcore.h`.

/// Tamanho máximo de um `write`/`read` em descritor.
pub const MAX_IO_SIZE: usize = 4096;
/// Tamanho máximo de um pedido `SYS_WRITE` (console sem descritor).
pub const MAX_CONSOLE_WRITE: usize = 512;
/// Tamanho máximo de um caminho consultado por `*_file`/`isdir`/`isfile`.
pub const MAX_PATH_QUERY: usize = 128;
/// Tamanho máximo de um caminho mutável (`open`, `mkdir`, `pivot_root`...).
pub const MAX_MUTABLE_PATH: usize = 256;
/// Buffer máximo devolvido por `readdir`/`dmesg`/`read_file`.
pub const MAX_READ_BUFFER: usize = MAX_IO_SIZE;
/// Conteúdo máximo aceito por `write_file`.
pub const MAX_WRITE_FILE_SIZE: usize = MAX_IO_SIZE;

/// Retrato do sistema devolvido por `SYS_SYSTEM_INFO`.
///
/// Todos os campos são valores já calculados pelo kernel: o userspace não tem
/// como consultar o TSC, o alocador de frames ou o escalonador diretamente, e
/// derivar essas informações de syscalls avulsas daria números inconsistentes
/// entre si (uptime medido antes e RAM medida depois, por exemplo).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// Microssegundos desde o boot, do relógio de TSC.
    pub uptime_microseconds: u64,
    /// Frequência do TSC em Hz (0 quando não calibrada).
    pub tsc_frequency: u64,
    /// RAM utilizável total, em bytes.
    pub memory_total: u64,
    /// RAM ainda livre, em bytes.
    pub memory_free: u64,
    /// Largura e altura do framebuffer ativo (0 se não houver).
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    /// Processos ativos no momento da chamada.
    pub process_count: u32,
    /// Prioridade do processo que fez a chamada (ver `Priority` no kernel).
    pub priority: u32,
    /// PID do processo que fez a chamada.
    pub pid: u64,
    /// Versão da ABI do kernel.
    pub abi_version: u64,
}

/// Flags do `waitpid`: `WNOHANG` — não bloqueia; devolve 0 se nenhum filho
/// terminou. Necessário para supervisores de serviços (Horust) e shells.
pub const WNOHANG: i64 = 1;

/// Superfícies de clientes registráveis por processo; o painel usa 1, um
/// cliente tipo terminal usaria outra.
pub const MAX_SURFACES: usize = 8;
/// Tamanho máximo do buffer de uma superfície, em pixels (320x200 como as
/// janelas internas do compositor).
pub const SURFACE_MAX_WIDTH: u32 = 320;
pub const SURFACE_MAX_HEIGHT: u32 = 200;
pub const SURFACE_MAX_PIXELS: usize =
    (SURFACE_MAX_WIDTH as usize) * (SURFACE_MAX_HEIGHT as usize);

/// Descrição de uma superfície de cliente, preenchida pelo chamador e
/// completada pelo kernel (`address`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceInfo {
    /// Id atribuído pelo kernel (0 se o pedido falhar).
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Pixels por linha no buffer mapeado (pode exceder `width`).
    pub pitch: u32,
    /// Endereço virtual do buffer no processo chamador.
    pub address: u64,
}

/// Eventos dirigidos ao dono de uma superfície.
pub const SURFACE_EVENT_KEY: u32 = 1;
pub const SURFACE_EVENT_MOUSE_MOVE: u32 = 2;
pub const SURFACE_EVENT_MOUSE_BUTTON: u32 = 3;
/// O compositor moveu a janela (`x`/`y` são a nova origem na tela).
pub const SURFACE_EVENT_MOVED: u32 = 4;
/// O compositor escondeu a janela (mínima ou fechada pelo WM).
pub const SURFACE_EVENT_HIDDEN: u32 = 5;

/// Snapshot de uma superfície visível, preenchido pelo kernel em
/// `surface_snapshot`. `address` já vem mapeado no espaço do servidor.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceSnapshot {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    /// Endereço virtual do buffer no espaço do servidor de display.
    pub address: u64,
    /// Origem da superfície na tela, definida pelo dono (`surface_move`).
    pub x: i32,
    pub y: i32,
    /// Ordem de empilhamento (maior = à frente).
    pub z: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceEvent {
    /// Superfície alvo.
    pub surface: u32,
    /// Um dos `SURFACE_EVENT_*`.
    pub kind: u32,
    pub x: i32,
    pub y: i32,
    pub button: u32,
    pub pressed: u32,
    /// Caractere (para `SURFACE_EVENT_KEY`).
    pub key: u32,
}

pub const SIGNAL_HUP: u64 = 1;
pub const SIGNAL_INT: u64 = 2;
pub const SIGNAL_KILL: u64 = 9;
pub const SIGNAL_SEGV: u64 = 11;
pub const SIGNAL_TERM: u64 = 15;
pub const SIGNAL_CONT: u64 = 18;
pub const SIGNAL_STOP: u64 = 19;
pub const SIGNAL_DEFAULT: u64 = 0;
pub const SIGNAL_IGNORE: u64 = 1;

pub const PROCESS_RUNNING: u32 = 1;
pub const PROCESS_STOPPED: u32 = 2;
/// Bloqueado em `waitpid` esperando um filho.
pub const PROCESS_WAITING: u32 = 3;
/// Terminou, mas o pai ainda não coletou o status.
pub const PROCESS_ZOMBIE: u32 = 4;
// Large enough for a shell plus a handful of pipeline stages. Each slot costs a
// 64 KiB kernel stack (see `KERNEL_STACKS`), so this is not free memory.
pub const MAX_PROCESSES: usize = 16;
pub const PROCESS_NAME_SIZE: usize = 64;
pub const ACCOUNT_NAME_SIZE: usize = 32;
pub const ACCOUNT_PASSWORD_SIZE: usize = 64;
pub const HOSTNAME_SIZE: usize = 64;

pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;
pub const MAX_OPEN_FDS: usize = 24;

pub const OPEN_READ: u64 = 1 << 0;
pub const OPEN_WRITE: u64 = 1 << 1;
pub const OPEN_CREATE: u64 = 1 << 2;
pub const OPEN_TRUNCATE: u64 = 1 << 3;
pub const OPEN_APPEND: u64 = 1 << 4;
pub const OPEN_EXCLUSIVE: u64 = 1 << 5;

pub const MAX_EXEC_ARGS: usize = 12;
pub const MAX_EXEC_ENV: usize = 12;
pub const MAX_EXEC_ITEM_SIZE: usize = 128;

/// Number of pipes available system-wide (each has a read end and a write end).
pub const MAX_PIPES: usize = 8;
/// Capacity of each pipe ring buffer, in bytes.
pub const PIPE_CAPACITY: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserArg {
    pub address: u64,
    pub length: u64,
}

// ------------------------------------------------------------------------------
// Tipos de suporte à biblioteca padrão (dnu-std)
// ------------------------------------------------------------------------------
/// Descriptor de uma thread criada por `SYS_THREAD_CREATE`.
///
/// A thread nova nasce no mesmo espaço de endereço do processo: memória,
/// descritores e credenciais são compartilhados. O que é próprio de cada thread
/// é a pilha (de kernel e de usuário) e o estado de FPU/SSE.
///
/// `entry` é o endereço da função que a thread começa a executar (recebe
/// `argument` em `rdi`); ao retornar, o kernel chama `SYS_THREAD_EXIT(0)`
/// implicitamente. `stack_top` é o topo da pilha de usuário já alocada pelo
/// chamador (a `std` aloca com `mmap`); `0` significa "o kernel escolhe".
/// `tls` é o valor gravado em `fs_base` para o TLS da thread (0 = nenhum).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadCreate {
    /// Função de entrada da thread (`fn(argument)`).
    pub entry: u64,
    /// Valor entregue em `rdi` à função de entrada.
    pub argument: u64,
    /// Topo da pilha de usuário da thread (0 = o kernel escolhe).
    pub stack_top: u64,
    /// Base do TLS da thread, gravada em `fs_base` (0 = não mexe).
    pub tls: u64,
}

/// `futex(&word, op, value, &timeout, ...)`: operações suportadas.
///
/// O subconjunto abaixo cobre `Mutex` (WAIT/WAKE), `Condvar`/`park` (WAKE com
/// contagem) e timeouts (`*_BITSET` com um `Timespec`). A semântica é a mesma do
/// futex do Linux: a espera é **revalidada** sob o bloqueio do escalonador para
/// não perder um `wake` que chegue entre a leitura de `word` e o bloqueio.
pub const FUTEX_WAIT: u64 = 0;
/// `futex(&word, FUTEX_WAKE, count, ...)`: acorda até `count` threads.
pub const FUTEX_WAKE: u64 = 1;
/// Como `FUTEX_WAIT`, mas a espera expira no `Timespec` informado.
pub const FUTEX_WAIT_BITSET: u64 = 9;
/// Como `FUTEX_WAKE`, restrito ao conjunto de bits indicado em `r8`.
pub const FUTEX_WAKE_BITSET: u64 = 10;
/// Máscara que seleciona todas as threads (`FUTEX_BITSET_MATCH_ANY`).
pub const FUTEX_BITSET_MATCH_ANY: u64 = 0xffff_ffff;

/// Relógios de `SYS_CLOCK_GETTIME`.
/// Tempo real (desde a epoch), ajustável — base de `SystemTime`.
pub const CLOCK_REALTIME: u64 = 0;
/// Relógio monótono (nunca retrocede) — base de `Instant` e de `nanosleep`.
pub const CLOCK_MONOTONIC: u64 = 1;

/// Intervalo de tempo em segundos + nanossegundos.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timespec {
    pub seconds: i64,
    pub nanoseconds: i64,
}

/// Flags de `SYS_MMAP`.
/// Mapeamento anônimo (sem arquivo por trás): só páginas zeroadas.
/// A `std` usa isto para o heap e as pilhas de thread.
pub const MAP_ANONYMOUS: u64 = 1 << 0;
/// Mapeamento privado (copy-on-write num `fork` futuro).
pub const MAP_PRIVATE: u64 = 1 << 1;
/// Exige um endereço exato em `address` (falha se já estiver ocupado).
pub const MAP_FIXED: u64 = 1 << 2;

/// Proteções de `SYS_MMAP` (bit a bit).
pub const PROT_READ: u64 = 1 << 0;
pub const PROT_WRITE: u64 = 1 << 1;
pub const PROT_EXEC: u64 = 1 << 2;

/// Tamanho de uma página de usuário, compartilhado por `mmap`/`munmap`.
pub const PAGE_SIZE: u64 = 4096;

/// Descrição do bloco TLS montado pelo kernel (`SYS_TLS_INFO`).
///
/// O kernel monta o bloco a partir do segmento `PT_TLS` do DEF e grava
/// `fs_base` (modelo *variant II*: as variáveis ficam em offsets negativos).
/// `size` é o `p_memsz` do segmento; `0` quando o binário não tem TLS.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlsInfo {
    pub fs_base: u64,
    pub size: u64,
    pub alignment: u64,
}

/// Maior `length` aceito por uma única `mmap`.
///
/// Sem este limite uma chamada com `length = u64::MAX` derrubaria o kernel ao
/// arredondar a faixa; 64 MiB é muito acima do que a `alloc` da `std` pede de
/// uma vez e continua cabendo com folga no espaço reservado a `mmap`.
pub const MMAP_MAX_LENGTH: u64 = 64 * 1024 * 1024;

/// Base da região de `mmap` anônimo — logo acima da heap de `sbrk`.
///
/// A heap cresce em `USER_HEAP_START` até `USER_HEAP_LIMIT` (64 MiB), então a
/// região anônima começa depois dela. Fica longe das superfícies
/// (`SURFACE_REGION_BASE`), da pilha (`0x7fff_ffff_0000`) e do framebuffer
/// (`0x4000_0000_0000`).
///
/// O kernel distribui as regiões em ordem crescente a partir daqui; o
/// userspace nunca depende de endereços fixos, só do que `mmap` devolve.
pub const MMAP_REGION_BASE: u64 = 0x0000_0002_0000_0000;

/// Teto da região de `mmap` anônimo (exclusivo).
///
/// 256 MiB de espaço virtual: com `MMAP_MAX_LENGTH` = 64 MiB isso comporta
/// várias alocações simultâneas. Como só as páginas tocadas consomem RAM
/// física (o mapeamento é sob demanda), o teto é generoso sem custar memória.
pub const MMAP_REGION_END: u64 = 0x0000_0003_0000_0000;

/// Maior número de threads (além da principal) por processo.
///
/// Cada thread consome um slot do escalonador e uma pilha de kernel de 128 KiB
/// (`KERNEL_STACKS`), então o limite é baixo de propósito. O total de threads
/// do sistema não passa de `MAX_PROCESSES`.
pub const MAX_THREADS_PER_PROCESS: usize = 8;

/// Pilha de usuário que o kernel aloca quando `ThreadCreate.stack_top == 0`.
pub const THREAD_STACK_SIZE: u64 = 128 * 1024;

/// Describes how a child created by `SYS_SPAWN` should see its standard
/// descriptors. Each field is a descriptor number owned by the *caller*: the
/// kernel duplicates it into the child, so `0`, `1`, `2` keep meaning
/// stdin/stdout/stderr. `-1` means "leave it as the default".
///
/// This is how a shell sets up a pipeline (`child 1 | child 2`) without a real
/// `fork`: give the left child `stdout = pipe_write`, the right child
/// `stdin = pipe_read`, and close the unused ends in the parent afterwards.
/// Describes how a child created by `SYS_SPAWN` should see its standard
/// descriptors, and carries the environment for the child.
///
/// The six syscall registers are all used by `SYS_SPAWN`, so the environment
/// (pointer and count) travels inside this structure instead of taking extra
/// registers. Both fields are `u64` so the layout is identical for the kernel
/// and for 64-bit userspace.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnIo {
    pub stdin_fd: i64,
    pub stdout_fd: i64,
    pub stderr_fd: i64,
    /// Ponteiro para um array de [`UserArg`] com o ambiente do filho.
    pub environment: u64,
    /// Quantidade de entradas em `environment`.
    pub environment_count: u64,
}

impl SpawnIo {
    /// Sentinel meaning "keep the default descriptor".
    pub const INHERIT: i64 = -1;
}

/// `Default` inherits every descriptor and passes no environment.
///
/// A caller that only wants to override one descriptor writes just that field:
///
/// ```ignore
/// SpawnIo { stdout_fd: write_end as i64, ..SpawnIo::default() }
/// ```
impl Default for SpawnIo {
    fn default() -> Self {
        Self {
            stdin_fd: Self::INHERIT,
            stdout_fd: Self::INHERIT,
            stderr_fd: Self::INHERIT,
            environment: 0,
            environment_count: 0,
        }
    }
}

/// Descrição do framebuffer para o servidor de display.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramebufferInfo {
    /// Endereço **virtual** onde o framebuffer aparece no processo chamador,
    /// preenchido por `fb_map`.
    pub address: u64,
    /// Endereço físico da memória de vídeo (informativo).
    pub physical_address: u64,
    pub width: u32,
    pub height: u32,
    /// Bytes por linha (pode ser maior que `width * 4`).
    pub pitch: u32,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
    pub reserved: u8,
}

/// Tipos de evento de entrada entregues ao servidor de display.
pub const INPUT_KEY: u32 = 1;
pub const INPUT_MOUSE_MOVE: u32 = 2;
pub const INPUT_MOUSE_BUTTON: u32 = 3;

/// A base do espaço virtual onde os buffers de superfície são mapeados. Fica
/// bem longe da heap (`USER_HEAP_LIMIT` = 64 MiB sobre `0x1_0000_0000`; a base
/// aqui é +256 MiB) e do framebuffer (`0x4000_0000_0000`). Cada superfície
/// recebe uma fatia alinhada de 1 MiB, então o endereço do cliente nunca
/// colide.
pub const SURFACE_REGION_BASE: u64 = 0x0000_0002_4000_0000;
pub const SURFACE_REGION_STRIDE: u64 = 0x10_0000; // 1 MiB

/// Um evento de entrada em formato fixo (sem ponteiros).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputEvent {
    /// Um dos `INPUT_*`.
    pub kind: u32,
    /// Tecla (para `INPUT_KEY`): o caractere ou um código de tecla especial.
    pub key: u32,
    /// Posição do cursor (para eventos de mouse).
    pub x: i32,
    pub y: i32,
    /// Botão (1 = esquerdo, 2 = direito, 3 = meio).
    pub button: u32,
    /// 1 = pressionado, 0 = liberado.
    pub pressed: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserStat {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u64,
    /// Um dos `PROCESS_*`.
    pub state: u32,
    pub pending_signals: u32,
    pub name: [u8; PROCESS_NAME_SIZE],
    /// Nível de prioridade (0 = mais baixo, ver `Priority` no kernel).
    pub priority: u32,
    /// 1 quando este é o processo em execução no momento da chamada.
    pub running: u32,
    /// Ticks de CPU que a fatia deste processo concede por vez.
    pub quantum_ticks: u32,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallConfig {
    pub username_length: u8,
    pub password_length: u8,
    pub hostname_length: u8,
    pub add_user: u8,
    pub administrator: u8,
    pub reserved: [u8; 3],
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub password: [u8; ACCOUNT_PASSWORD_SIZE],
    pub hostname: [u8; HOSTNAME_SIZE],
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            username_length: 0,
            password_length: 0,
            hostname_length: 0,
            add_user: 0,
            administrator: 0,
            reserved: [0; 3],
            username: [0; ACCOUNT_NAME_SIZE],
            password: [0; ACCOUNT_PASSWORD_SIZE],
            hostname: [0; HOSTNAME_SIZE],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserIdentity {
    pub uid: u32,
    pub gid: u32,
    pub is_admin: u32,
    pub username_length: u32,
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub hostname_length: u32,
    pub hostname: [u8; HOSTNAME_SIZE],
}

impl Default for UserIdentity {
    fn default() -> Self {
        Self {
            uid: 0,
            gid: 0,
            is_admin: 0,
            username_length: 0,
            username: [0; ACCOUNT_NAME_SIZE],
            hostname_length: 0,
            hostname: [0; HOSTNAME_SIZE],
        }
    }
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self {
            pid: 0,
            state: 0,
            pending_signals: 0,
            name: [0; PROCESS_NAME_SIZE],
            priority: 0,
            running: 0,
            quantum_ticks: 0,
            _reserved: 0,
        }
    }
}

impl UserArg {
    pub const EMPTY: Self = Self {
        address: 0,
        length: 0,
    };
}
