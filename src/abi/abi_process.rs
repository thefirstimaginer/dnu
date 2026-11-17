//! Syscalls de processo, IPC e credenciais.
//!
//! * `exec`/`execve` — o primeiro cria um processo novo com a imagem; o segundo
//!   substitui a imagem do processo atual (par `fork` + `exec` do POSIX);
//! * `spawn` — cria um filho com descritores padrão e ambiente escolhidos;
//! * `pipe` — cria as duas pontas e escreve os descritores no array do usuário;
//! * `process_list` — snapshot do escalonador;
//! * `authenticate` — valida credenciais e troca a identidade do processo.
//!
//! `exec`, `execve` e `spawn` compartilham a decodificação de `argv`/`envp`,
//! feita por `ExecArguments`.

use crate::abi::{SpawnIo, MAX_EXEC_ARGS, MAX_EXEC_ENV, MAX_EXEC_ITEM_SIZE};

use super::trap::{TrapFrame, E2BIG, EFAULT, EINVAL};
use super::user::{copy_struct_from_user, load_user_vector, vector_slices, Vector};
use crate::process;

/// Escreve diagnóstico na UART.
fn diag(s: &[u8]) {
    crate::console_write_bytes(s);
}

/// `exec`/`execve`: decodifica caminho, argumentos e ambiente.
pub fn exec_handler(frame: &mut TrapFrame) -> *mut TrapFrame {
    crate::console_write_bytes(b"exec_handler: entered\r\n");
    let replace = frame.rax == crate::abi::SYS_EXECVE;
    let mut decoded = ExecArguments::new();
    if let Err(error) = decoded.load(frame) {
        crate::console_write_bytes(b"exec_handler: failed to decode arguments\r\n");
        return frame.fail(error);
    }
    crate::console_write_bytes(b"exec_handler: path=");
    crate::console_write_bytes(decoded.path().as_bytes());
    crate::console_write_bytes(b"\r\n");
    if replace {
        let result = process::execve_current(decoded.path(), &decoded.arguments(), &decoded.environment(), frame);
        crate::console_write_bytes(b"exec_handler: execve returned\r\n");
        result
    } else {
        let result = process::spawn_current(decoded.path(), &decoded.arguments(), &decoded.environment(), frame);
        crate::console_write_bytes(b"exec_handler: spawn returned\r\n");
        result
    }
}

/// `spawn(path, len, argv, argc, &SpawnIo, _)`.
///
/// O sexto argumento (`r9`) não é usado: o ambiente viaja dentro de `SpawnIo`
/// porque os seis registradores já estão ocupados.
pub fn spawn_handler(frame: &mut TrapFrame) -> *mut TrapFrame {
    crate::console_write_bytes(b"spawn_handler CALLED!\r\n");
    crate::console_write_bytes(b"spawn_handler: path_ptr=0x");
    // caminho e tamanho são lidos abaixo
    let path_ptr = frame.rdi;
    crate::serial_write_hex(path_ptr);
    crate::console_write_bytes(b"\r\n");
    crate::serial_write(b"spawn_handler: rsi(path_len)=0x");
    crate::serial_write_hex(frame.rsi);
    crate::console_write_bytes(b"\r\n");
    let path_len = frame.rsi;
    // Escreve o ponteiro e tamanho em hex simples (debug)
    let mut ptr = path_ptr;
    let mut hex_buf = [0u8; 16];
    let mut h = 16;
    while h > 0 {
        h -= 1;
        hex_buf[h] = b"0123456789abcdef"[(ptr & 0xf) as usize];
        ptr >>= 4;
    }
    crate::console_write_bytes(&hex_buf[h..]);
    crate::console_write_bytes(b" len=");
    let mut len = path_len;
    let mut len_buf = [0u8; 20];
    let mut l = len_buf.len();
    if len == 0 {
        l -= 1;
        len_buf[l] = b'0';
    } else {
        while len > 0 {
            l -= 1;
            len_buf[l] = b'0' + (len % 10) as u8;
            len /= 10;
        }
    }
    crate::console_write_bytes(&len_buf[l..]);
    crate::console_write_bytes(b"\r\n");
    // O caminho nunca excede a ABI de exec, então um buffer de 128 bytes basta.
    let mut path_buffer = [0u8; MAX_EXEC_ITEM_SIZE];
    let path_length = frame.rsi;
    crate::serial_write(b"spawn_handler: path_length=");
    crate::serial_write_hex(path_length);
    crate::console_write_bytes(b"\r\n");
    let path = {
        let length = match usize::try_from(path_length) {
            Ok(length) => length,
            Err(_) => return frame.fail(EINVAL),
        };
        if length == 0 || length > path_buffer.len() {
            return frame.fail(EINVAL);
        }
        // Copia o caminho da memória do usuário para o buffer do kernel.
        crate::serial_write(b"spawn_handler: about to copy path from user\r\n");
        let copy_ok = process::copy_from_current_user(frame.rdi, &mut path_buffer[..length]);
        crate::console_write_bytes(b"spawn_handler: copy=");
        crate::serial_write(if copy_ok { b"1" } else { b"0" });
        crate::console_write_bytes(b"\r\n");
        if !copy_ok {
            crate::serial_write(b"spawn_handler: copy_from_user failed\r\n");
            return frame.fail(EFAULT);
        }
        match core::str::from_utf8(&path_buffer[..length]) {
            Ok(path) => {
                crate::console_write_bytes(b"spawn_handler: path=");
                crate::console_write_bytes(path.as_bytes());
                crate::console_write_bytes(b"\r\n");
                path
            }
            Err(_) => {
                crate::serial_write(b"spawn_handler: from_utf8 failed\r\n");
                return frame.fail(EINVAL);
            }
        }
    };

    crate::serial_write(b"spawn_handler: path_copy_ok\r\n");

    let mut io = SpawnIo::default();
    crate::serial_write(b"spawn_handler: r8=");
    crate::serial_write_hex(frame.r8);
    crate::console_write_bytes(b"\r\n");
    if frame.r8 != 0 {
        if !copy_struct_from_user(frame.r8, &mut io) {
            crate::serial_write(b"spawn_handler: copy_struct_from_user failed\r\n");
            return frame.fail(EFAULT);
        }
        crate::serial_write(b"spawn_handler: io_copied\r\n");
    }

    let argument_count = frame.r10 as usize;
    crate::serial_write(b"spawn_handler: argument_count=");
    crate::serial_write_hex(argument_count as u64);
    crate::console_write_bytes(b"\r\n");

    if argument_count > MAX_EXEC_ARGS {
        return frame.fail(E2BIG);
    }
    let mut arguments = [[0u8; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ARGS];
    let mut argument_lengths = [0usize; MAX_EXEC_ARGS];
    crate::serial_write(b"spawn_handler: about to load_user_vector(arguments)\r\n");
    if let Err(error) =
        load_user_vector(frame.rdx, argument_count, &mut arguments, &mut argument_lengths)
    {
        crate::serial_write(b"spawn_handler: load_user_vector(arguments) failed\r\n");
        return frame.fail(error);
    }
    crate::serial_write(b"spawn_handler: load_user_vector(arguments) ok\r\n");

    let environment_count = io.environment_count as usize;
    crate::serial_write(b"spawn_handler: environment_count=");
    crate::serial_write_hex(environment_count as u64);
    crate::console_write_bytes(b"\r\n");
    if environment_count > MAX_EXEC_ENV {
        return frame.fail(E2BIG);
    }
    let mut environment = [[0u8; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ENV];
    let mut environment_lengths = [0usize; MAX_EXEC_ENV];
    crate::serial_write(b"spawn_handler: about to load_user_vector(environment)\r\n");
    if let Err(error) = load_user_vector(
        io.environment,
        environment_count,
        &mut environment,
        &mut environment_lengths,
    ) {
        crate::serial_write(b"spawn_handler: load_user_vector(environment) failed\r\n");
        return frame.fail(error);
    }
    crate::serial_write(b"spawn_handler: load_user_vector(environment) ok\r\n");

    crate::serial_write(b"spawn_handler: calling spawn_current_with_io path=");
    crate::serial_write(path.as_bytes());
    crate::serial_write(b"\r\n");
    let result = process::spawn_current_with_io(
        path,
        &vector_slices(&arguments, &argument_lengths, argument_count),
        &vector_slices(&environment, &environment_lengths, environment_count),
        &io,
        frame,
    );
    crate::console_write_bytes(b"spawn_handler: spawn_current_with_io returned, rcx=0x");
    let mut rcx_val = unsafe { (*result).rcx };
    let mut buf = [0u8; 16];
    let mut i = 0;
    if rcx_val == 0 { buf[i] = b'0'; i += 1; }
    while rcx_val > 0 { buf[i] = b'0' + (rcx_val % 16) as u8; if buf[i] > b'9' { buf[i] += 7; } i += 1; rcx_val /= 16; }
    while i > 0 { i -= 1; crate::console_write_bytes(&buf[i..i+1]); }
    crate::console_write_bytes(b" rip=0x");
    let mut rip_val = unsafe { (*result).rip };
    let mut buf2 = [0u8; 16];
    let mut j = 0;
    if rip_val == 0 { buf2[j] = b'0'; j += 1; }
    while rip_val > 0 { buf2[j] = b'0' + (rip_val % 16) as u8; if buf2[j] > b'9' { buf2[j] += 7; } j += 1; rip_val /= 16; }
    while j > 0 { j -= 1; crate::console_write_bytes(&buf2[j..j+1]); }
    crate::console_write_bytes(b"\r\n");
    result
}

/// `pipe(int fds[2])`: devolve `[leitura, escrita]` no array do usuário.
pub fn create_pipe(frame: &mut TrapFrame) -> *mut TrapFrame {
    let id = match crate::ipc::create() {
        Ok(id) => id,
        Err(error) => return frame.fail(error.to_errno()),
    };
    let read_end = process::allocate_descriptor(process::Descriptor::pipe(
        process::DescriptorKind::PipeRead,
        id,
    ));
    let write_end = process::allocate_descriptor(process::Descriptor::pipe(
        process::DescriptorKind::PipeWrite,
        id,
    ));
    let (Some(read_end), Some(write_end)) = (read_end, write_end) else {
        // Desfaz o que foi alocado para o pipe não vazar.
        if let Some(fd) = read_end {
            process::close_descriptor(fd);
        }
        if let Some(fd) = write_end {
            process::close_descriptor(fd);
        }
        crate::ipc::close_reader(id);
        crate::ipc::close_writer(id);
        return frame.fail(-24); // EMFILE
    };
    let endpoints = [read_end as i32, write_end as i32];
    let bytes = unsafe {
        core::slice::from_raw_parts(
            endpoints.as_ptr().cast::<u8>(),
            core::mem::size_of_val(&endpoints),
        )
    };
    if !process::copy_to_current_user(frame.rdi, bytes) {
        process::close_descriptor(read_end);
        process::close_descriptor(write_end);
        return frame.fail(EFAULT);
    }
    frame.ok(0)
}

/// `process_list(&[ProcessInfo], capacity)`.
pub fn process_list(frame: &mut TrapFrame) -> *mut TrapFrame {
    let capacity = frame.rsi as usize;
    if capacity > crate::abi::MAX_PROCESSES {
        return frame.fail(EINVAL);
    }
    let (processes, count) = process::snapshot();
    let length = count.min(capacity);
    let bytes = unsafe {
        core::slice::from_raw_parts(
            processes.as_ptr().cast::<u8>(),
            length * core::mem::size_of::<crate::abi::ProcessInfo>(),
        )
    };
    if process::copy_to_current_user(frame.rdi, bytes) {
        frame.ok(length as u64)
    } else {
        frame.fail(EFAULT)
    }
}

/// `authenticate(user, user_len, pass, pass_len, target_uid)`.
pub fn authenticate(frame: &mut TrapFrame) -> *mut TrapFrame {
    let username_length = frame.rsi as usize;
    let password_length = frame.r10 as usize;
    let mut username = [0u8; crate::abi::ACCOUNT_NAME_SIZE];
    let mut password = [0u8; crate::abi::ACCOUNT_PASSWORD_SIZE];
    let valid = username_length != 0
        && username_length <= username.len()
        && password_length != 0
        && password_length <= password.len()
        && process::copy_from_current_user(frame.rdi, &mut username[..username_length])
        && process::copy_from_current_user(frame.rdx, &mut password[..password_length])
        && !username[..username_length].contains(&0)
        && !password[..password_length].contains(&0);
    if !valid {
        return frame.fail(EINVAL);
    }
    let result = process::authenticate_current(
        &username[..username_length],
        &password[..password_length],
        frame.r8 as u32,
    );
    password.fill(0);
    frame.ok(result as u64)
}

/// `argv`/`envp` comuns a `exec`, `execve` e `spawn`.
///
/// Layout no `TrapFrame` (igual ao de `execve(2)`): `rdi = path`,
/// `rsi = path_len`, `rdx = argv`, `r10 = argc`, `r8 = envp`, `r9 = envc`.
pub struct ExecArguments {
    path: [u8; MAX_EXEC_ITEM_SIZE],
    path_length: usize,
    arguments: [[u8; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ARGS],
    environment: [[u8; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ENV],
    argument_lengths: [usize; MAX_EXEC_ARGS],
    environment_lengths: [usize; MAX_EXEC_ENV],
    argument_count: usize,
    environment_count: usize,
}

impl ExecArguments {
    const fn new() -> Self {
        Self {
            path: [0; MAX_EXEC_ITEM_SIZE],
            path_length: 0,
            arguments: [[0; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ARGS],
            environment: [[0; MAX_EXEC_ITEM_SIZE]; MAX_EXEC_ENV],
            argument_lengths: [0; MAX_EXEC_ARGS],
            environment_lengths: [0; MAX_EXEC_ENV],
            argument_count: 0,
            environment_count: 0,
        }
    }

    fn load(&mut self, frame: &TrapFrame) -> Result<(), i64> {
        self.load_path(frame.rdi, frame.rsi)?;
        self.argument_count = load_user_vector(
            frame.rdx,
            frame.r10 as usize,
            &mut self.arguments,
            &mut self.argument_lengths,
        )?;
        self.environment_count = load_user_vector(
            frame.r8,
            frame.r9 as usize,
            &mut self.environment,
            &mut self.environment_lengths,
        )?;
        Ok(())
    }

    fn load_path(&mut self, pointer: u64, length: u64) -> Result<(), i64> {
        let length = usize::try_from(length).map_err(|_| EINVAL)?;
        if length == 0 || length > self.path.len() {
            return Err(EINVAL);
        }
        if !process::copy_from_current_user(pointer, &mut self.path[..length]) {
            return Err(EFAULT);
        }
        if self.path[..length].contains(&0) {
            return Err(EINVAL);
        }
        self.path_length = length;
        Ok(())
    }

    fn path(&self) -> &str {
        // `load_path` só grava caminhos que já decodificaram como UTF-8.
        core::str::from_utf8(&self.path[..self.path_length]).unwrap_or("")
    }

    fn arguments(&self) -> Vector<'_, MAX_EXEC_ARGS> {
        vector_slices(&self.arguments, &self.argument_lengths, self.argument_count)
    }

    fn environment(&self) -> Vector<'_, MAX_EXEC_ENV> {
        vector_slices(&self.environment, &self.environment_lengths, self.environment_count)
    }
}
