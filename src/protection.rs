//! Proteção do processo 1 (init) no lado do kernel.
//!
//! O PID 1 é o supervisor do sistema: se ele morre, não sobra ninguém para
//! coletar zumbis, reiniciar serviços ou desligar a máquina. Este módulo
//! concentra as regras que garantem isso, em vez de espalhar comparações
//! `pid == 1` pela camada de ABI:
//!
//! * [`PID_INIT`] é o pid reservado para o primeiro processo;
//! * [`is_init`] identifica o supervisor (por pid ou por slot, durante o boot);
//! * [`can_signal`] recusa sinais fatais ao init;
//! * [`can_exit`] recusa o encerramento do init por `exit(2)`;
//! * [`can_be_reaped`] recusa que um `waitpid` libere o slot do init.
//!
//! O primeiro processo **sempre** ocupa o slot 0 e recebe [`PID_INIT`]; o
//! escalonador o registra antes de qualquer outro, então ele é,
//! por construção, o primeiro a executar em ring 3 (`process::start`).

use crate::abi::trap::{EPERM, TrapFrame};

/// PID reservado ao supervisor do sistema.
pub const PID_INIT: usize = 1;

/// Verdadeiro quando `pid` é o supervisor.
pub const fn is_init_pid(pid: usize) -> bool {
    pid == PID_INIT
}

/// Verdadeiro quando o processo no `slot` é o supervisor.
///
/// Vale também antes de o pid estar visível ao userspace (o `init` roda com o
/// slot 0 reservado desde `process::init`).
pub const fn is_init_slot(slot: usize) -> bool {
    slot == 0
}

/// Recusa sinais que encerrariam o init.
///
/// `SIGKILL` e os demais sinais terminantes são bloqueados; sinais que apenas
/// interrompem (`SIGINT`, `SIGSTOP`) continuam permitidos, porque um supervisor
/// precisa poder ser parado para depuração sem morrer.
pub fn can_signal(pid: usize, signal: u64) -> Result<(), i64> {
    if !is_init_pid(pid) {
        return Ok(());
    }
    if crate::process::is_terminating_signal(signal) {
        return Err(EPERM);
    }
    Ok(())
}

/// Recusa `exit(2)` no init.
///
/// Um supervisor que termina deixa o sistema sem PID 1. O processo continua
/// rodando e o chamador recebe `EPERM`, o que o obriga a tratar o caso em vez de
/// simplesmente desaparecer.
pub fn can_exit(pid: usize) -> Result<(), i64> {
    if is_init_pid(pid) {
        return Err(EPERM);
    }
    Ok(())
}

/// Recusa a coleta do init por `waitpid`.
///
/// Mesmo que o init chegue ao estado de zumbi (por uma via que este módulo não
/// previu), liberar o slot apagaria o supervisor do escalonador.
pub fn can_be_reaped(pid: usize) -> bool {
    !is_init_pid(pid)
}

/// Marca o frame como recusado com `EPERM`.
#[allow(dead_code)]
pub fn deny(frame: &mut TrapFrame) -> *mut TrapFrame {
    frame.fail(EPERM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_pid_one_in_slot_zero() {
        assert!(is_init_pid(PID_INIT));
        assert!(!is_init_pid(0));
        assert!(!is_init_pid(2));
        assert!(is_init_slot(0));
        assert!(!is_init_slot(1));
    }

    #[test]
    fn terminating_signals_are_refused_for_init_only() {
        for signal in [
            imagineos_abi::SIGNAL_KILL,
            imagineos_abi::SIGNAL_TERM,
            imagineos_abi::SIGNAL_SEGV,
        ] {
            assert_eq!(can_signal(PID_INIT, signal), Err(EPERM));
            assert_eq!(can_signal(2, signal), Ok(()));
        }
        // Parar o supervisor continua permitido: é preciso poder depurá-lo.
        assert_eq!(can_signal(PID_INIT, imagineos_abi::SIGNAL_STOP), Ok(()));
        assert_eq!(can_signal(PID_INIT, imagineos_abi::SIGNAL_INT), Ok(()));
    }

    #[test]
    fn init_cannot_exit_nor_be_reaped() {
        assert_eq!(can_exit(PID_INIT), Err(EPERM));
        assert_eq!(can_exit(2), Ok(()));
        assert!(!can_be_reaped(PID_INIT));
        assert!(can_be_reaped(2));
    }
}
