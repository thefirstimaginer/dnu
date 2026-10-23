//! Regras do processo 1 (init) no kernel.
//!
//! Ver [`protection`] para as invariantes. Este módulo existe para que a
//! proteção do supervisor tenha um dono claro: a camada de ABI consulta estas
//! funções em vez de comparar `pid == 1` em cada handler.

pub mod protection;
