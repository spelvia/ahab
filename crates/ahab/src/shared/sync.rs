//! Locks that keep working after a thread panicked while holding one: the
//! data they guard stays consistent between statements, so a poisoned lock
//! is used as it is rather than taking the process down.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, even when poisoned.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
