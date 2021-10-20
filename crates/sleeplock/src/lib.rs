#![no_std]

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crossbeam_queue::SegQueue;

pub struct Mutex<R, T: ?Sized> {
    wakers: SegQueue<Waker>,
    inner: lock_api::Mutex<R, T>,
}

unsafe impl<R: lock_api::RawMutex + Send, T: ?Sized + Send> Send for Mutex<R, T> {}
unsafe impl<R: lock_api::RawMutex + Sync, T: ?Sized + Send> Sync for Mutex<R, T> {}

impl<R: lock_api::RawMutex, T> Mutex<R, T> {
    pub fn new(value: T) -> Self {
        Self {
            wakers: SegQueue::new(),
            inner: lock_api::Mutex::new(value),
        }
    }

    pub fn lock(&self) -> MutexLockFuture<'_, R, T> {
        MutexLockFuture { lock: self }
    }
}

pub struct MutexGuard<'a, R: lock_api::RawMutex, T: ?Sized> {
    inner: lock_api::MutexGuard<'a, R, T>,
    wakers: &'a SegQueue<Waker>,
}

pub struct RwLock<R, T: ?Sized> {
    wakers: SegQueue<Waker>,
    inner: lock_api::RwLock<R, T>,
}

unsafe impl<R: lock_api::RawRwLock + Send, T: ?Sized + Send> Send for RwLock<R, T> {}
unsafe impl<R: lock_api::RawRwLock + Sync, T: ?Sized + Send + Sync> Sync for RwLock<R, T> {}

impl<R: lock_api::RawRwLock, T> RwLock<R, T> {
    pub fn new(value: T) -> Self {
        Self {
            wakers: SegQueue::new(),
            inner: lock_api::RwLock::new(value),
        }
    }

    pub fn read(&self) -> RwLockReadFuture<'_, R, T> {
        RwLockReadFuture { lock: self }
    }

    pub fn write(&self) -> RwLockWriteFuture<'_, R, T> {
        RwLockWriteFuture { lock: self }
    }
}

pub struct RwLockReadGuard<'a, R: lock_api::RawRwLock, T: ?Sized> {
    inner: lock_api::RwLockReadGuard<'a, R, T>,
    wakers: &'a SegQueue<Waker>,
}

pub struct RwLockWriteGuard<'a, R: lock_api::RawRwLock, T: ?Sized> {
    inner: lock_api::RwLockWriteGuard<'a, R, T>,
    wakers: &'a SegQueue<Waker>,
}

macro_rules! impl_drop_for_guard {
    ($name:ident where R: lock_api::$bound:tt) => {
        impl<'a, R: lock_api::$bound, T: ?Sized> Drop for $name<'a, R, T> {
            fn drop(&mut self) {
                // Wake up another thread that is waiting for this lock
                if let Some(waker) = self.wakers.pop() {
                    waker.wake_by_ref()
                }
                // drop(&mut self.inner)
            }
        }
    };
}

macro_rules! impl_deref_for_guard {
    ($name:ident where R: lock_api::$bound:tt) => {
        impl<'a, R: lock_api::$bound, T> core::ops::Deref for $name<'a, R, T> {
            type Target = T;
            fn deref(&self) -> &Self::Target {
                self.inner.deref()
            }
        }
    };
}

macro_rules! impl_deref_mut_for_guard {
    ($name:ident where R: lock_api::$bound:tt) => {
        impl<'a, R: lock_api::$bound, T> core::ops::DerefMut for $name<'a, R, T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                self.inner.deref_mut()
            }
        }
    };
}

impl_drop_for_guard!(MutexGuard where R: lock_api::RawMutex);
impl_drop_for_guard!(RwLockReadGuard where R: lock_api::RawRwLock);
impl_drop_for_guard!(RwLockWriteGuard where R: lock_api::RawRwLock);

impl_deref_for_guard!(MutexGuard where R: lock_api::RawMutex);
impl_deref_for_guard!(RwLockReadGuard where R: lock_api::RawRwLock);
impl_deref_for_guard!(RwLockWriteGuard where R: lock_api::RawRwLock);

impl_deref_mut_for_guard!(MutexGuard where R: lock_api::RawMutex);
impl_deref_mut_for_guard!(RwLockWriteGuard where R: lock_api::RawRwLock);

macro_rules! lock_future {
    ($future_name:ident where R: lock_api::$bound:tt, lock: $lock:ident, output: $output:ident, try_fn: $try_fn:ident) => {
        pub struct $future_name<'a, R, T> {
            lock: &'a $lock<R, T>,
        }

        impl<'a, R: lock_api::$bound, T> Future for $future_name<'a, R, T> {
            type Output = $output<'a, R, T>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                match self.lock.inner.$try_fn() {
                    None => {
                        // TODO: If there are multiple (unlikely) calls to poll that do not obtain a lock there may be repeated insertions of waker
                        // Consider using thread ids as hash key de-duplication
                        self.lock.wakers.push(cx.waker().clone());
                        Poll::Pending
                    }
                    Some(guard) => Poll::Ready($output {
                        inner: guard,
                        wakers: &self.lock.wakers,
                    }),
                }
            }
        }
    };
}

lock_future!(
    MutexLockFuture where R: lock_api::RawMutex,
    lock: Mutex,
    output: MutexGuard,
    try_fn: try_lock
);

lock_future!(
    RwLockReadFuture where R: lock_api::RawRwLock,
    lock: RwLock,
    output: RwLockReadGuard,
    try_fn: try_read
);

lock_future!(
    RwLockWriteFuture where R: lock_api::RawRwLock,
    lock: RwLock,
    output: RwLockWriteGuard,
    try_fn: try_write
);
