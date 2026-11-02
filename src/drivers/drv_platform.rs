//! Drivers de plataforma: ACPI no registro genérico.
//!
//! O ACPI é o driver "sem dispositivo": gerencia energia, tabelas e
//! desligamento. Vive em `acpi.rs`; aqui apenas o publicamos no registro
//! para que `poweroff`/`reboot` e ferramentas de diagnóstico encontrem o
//! serviço de plataforma pelo registro central.

use crate::drivers::{Device, DeviceClass};

struct Acpi;

impl Device for Acpi {
    fn name(&self) -> &'static str {
        "acpi0"
    }

    fn class(&self) -> DeviceClass {
        DeviceClass::Platform
    }

    fn describe(&self) -> &'static str {
        "ACPI tables (RSDP) — power management"
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

static ACPI0: Acpi = Acpi;

/// Publica o ACPI no registro (após `acpi::init`).
pub fn register() {
    crate::drivers::registry::register(&ACPI0);
}
