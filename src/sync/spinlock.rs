//! Spinlock simples para proteção de estado global em sistemas single-core
//! com preparação para SMP.
//!
//! Em single-core, o spinlock é um no-op (interrupções desligadas bastam).
//! Em SMP, usaria `atomic::spin_loop_hint` e uma palavra de trava AtomicBool.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use core::ops::{Deref, DerefMut};

/// Spinlock que protege dados compartilhados entre CPUs.
///
/// # Safety
/// O tipo `T` protegido deve ser `Send` para ser compartilhado entre threads
/// de diferentes CPUs.
pub struct SpinLock<T: ?Sized> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    /// Cria um novo spinlock desbloqueado.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Adquire o travamento, esperando (spin) até ficar disponível.
    ///
    /// Em single-core com interrupções desligadas, isso retorna imediatamente
    /// se o lock não estiver segurado por outro contexto de interrupção.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }

    /// Tenta adquirir o travamento sem esperar. Retorna `None` se já estiver trancado.
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }

    /// Libera o travamento.
    unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

impl<T> SpinLock<T>
where
    T: Default,
{
    /// Cria um novo spinlock com o valor padrão de `T`.
    pub fn new_default() -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(T::default()),
        }
    }
}

/// Guard RAII que libera o spinlock ao ser descartado.
pub struct SpinLockGuard<'a, T: ?Sized> {
    lock: &'a SpinLock<T>,
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            self.lock.locked.store(false, Ordering::Release);
        }
    }
}

unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}
unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}
