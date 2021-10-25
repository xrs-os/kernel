use crate::spinlock;

/// A mutual exclusion primitive useful for protecting shared data
/// This mutex will block threads waiting for the lock to become available.
#[allow(dead_code)]
pub type Mutex<T> = sleeplock::Mutex<spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type MutexLockFuture<'a, T> = sleeplock::MutexLockFuture<'a, spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type MutexLockGuard<'a, T> = sleeplock::MutexGuard<'a, spinlock::MutexIrq<()>, T>;

/// A reader-writer lock
/// This mutex will block threads waiting for the lock to become available.
#[allow(dead_code)]
pub type RwLock<T> = sleeplock::RwLock<spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type RwLockReadFuture<'a, T> = sleeplock::RwLockReadFuture<'a, spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type RwLockWriteFuture<'a, T> = sleeplock::RwLockWriteFuture<'a, spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type RwLockReadGuard<'a, T> = sleeplock::RwLockReadGuard<'a, spinlock::MutexIrq<()>, T>;

#[allow(dead_code)]
pub type RwLockWriteGuard<'a, T> = sleeplock::RwLockWriteGuard<'a, spinlock::MutexIrq<()>, T>;
