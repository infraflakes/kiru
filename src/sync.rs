//! The crate's one synchronization policy: a poisoned lock means a body
//! panicked while holding it; the data behind the lock is still structurally
//! valid, and the panic travels through the runtime's own failure flag, so
//! every lock is recovered rather than propagated.

mod lock_recovery;

pub(crate) use lock_recovery::{MutexExt, RwLockExt};
