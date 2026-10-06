//! Lock access shared across the crate.
//!
//! A poisoned lock means a body panicked while holding it. The data behind
//! the lock is still structurally valid, and the panic is reported through
//! the runtime's own failure flag, so every lock is recovered rather than
//! propagated. The policy lives here once so no caller reimplements it.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock a mutex, recovering the data if the lock was poisoned.
pub(crate) trait MutexExt<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Read or write a lock, recovering the data if the lock was poisoned.
pub(crate) trait RwLockExt<T> {
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T>;
    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T> {
        self.write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
