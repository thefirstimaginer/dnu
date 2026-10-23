// Conversão de ciclos do TSC em tempo — a matemática do relógio do kernel,
// isolada de portas de E/S para poder ser testada no host.
//
// Este arquivo é um módulo de `dnu_kernel/src/drivers/time.rs` (kernel) e é
// também o módulo `time` de `dnu_kernel/src/tests/trap.rs` (testes), de modo que
// a versão testada e a versão que roda são literalmente a mesma função.
// Converte ciclos de TSC em microssegundos desde o boot.
//
// Devolve `None` quando a frequência é desconhecida: sem ela não há conversão
// honesta, e o log degrada para `[time?]` em vez de imprimir um número errado.
//
// A conta é feita em duas parcelas (`ciclos / frequência` para os segundos
// inteiros e `resto * 1_000_000 / frequência` para a fração) para não estourar
// `u64`: `ciclos * 1_000_000` estouraria em poucos segundos a 3 GHz.
pub fn elapsed_microseconds(cycles: u64, frequency: u64) -> Option<u64> {
    if frequency == 0 {
        return None;
    }
    Some(cycles / frequency * 1_000_000 + cycles % frequency * 1_000_000 / frequency)
}

// Converte ciclos de TSC em milissegundos desde o boot (usado por `top`).
pub fn elapsed_milliseconds(cycles: u64, frequency: u64) -> Option<u64> {
    elapsed_microseconds(cycles, frequency).map(|microseconds| microseconds / 1000)
}
