use crate::spinlock;

/// A mutual exclusion primitive useful for protecting shared data
/// This mutex will block threads waiting for the lock to become available.
pub type Mutex<T> = sleeplock::Mutex<spinlock::MutexIrq<()>, T>;

/// A reader-writer lock
/// This mutex will block threads waiting for the lock to become available.
pub type RwLock<T> = sleeplock::RwLock<spinlock::RwLockIrq<()>, T>;
