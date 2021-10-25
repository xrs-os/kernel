#![no_std]

use core::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crossbeam_queue::SegQueue;

pub struct Mutex<R, T: ?Sized> {
    locked: lock_api::Mutex<R, bool>,
    wakers: SegQueue<Waker>,
    value: UnsafeCell<T>,
}

unsafe impl<R: lock_api::RawMutex + Send, T: ?Sized + Send> Send for Mutex<R, T> {}
unsafe impl<R: lock_api::RawMutex + Sync, T: ?Sized + Send> Sync for Mutex<R, T> {}

impl<R: lock_api::RawMutex, T> Mutex<R, T> {
    pub fn new(value: T) -> Self {
        Self {
            locked: lock_api::Mutex::new(false),
            wakers: SegQueue::new(),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> MutexLockFuture<'_, R, T> {
        MutexLockFuture { mutex: self }
    }
}

pub struct MutexGuard<'a, R: lock_api::RawMutex, T: ?Sized> {
    mutex: &'a Mutex<R, T>,
}

impl<'a, R: lock_api::RawMutex, T: ?Sized> Drop for MutexGuard<'a, R, T> {
    fn drop(&mut self) {
        *self.mutex.locked.lock() = false;
        // Wake up another thread that is waiting for this lock
        if let Some(waker) = self.mutex.wakers.pop() {
            waker.wake_by_ref()
        }
    }
}

impl<'a, R: lock_api::RawMutex, T> core::ops::Deref for MutexGuard<'a, R, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'a, R: lock_api::RawMutex, T> core::ops::DerefMut for MutexGuard<'a, R, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

pub struct MutexLockFuture<'a, R, T> {
    mutex: &'a Mutex<R, T>,
}

impl<'a, R: lock_api::RawMutex, T> Future for MutexLockFuture<'a, R, T> {
    type Output = MutexGuard<'a, R, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut locked = self.mutex.locked.lock();
        if *locked {
            // TODO: If there are multiple (unlikely) calls to poll method that do not obtain a lock there may be repeated insertions of waker
            // Consider using thread ids as hash key de-duplication
            self.mutex.wakers.push(cx.waker().clone());
            Poll::Pending
        } else {
            *locked = true;
            Poll::Ready(MutexGuard { mutex: self.mutex })
        }
    }
}

// TODO: RwLock is temporarily replaced by Mutex.
pub type RwLockReadFuture<'a, R, T> = MutexLockFuture<'a, R, T>;
pub type RwLockWriteFuture<'a, R, T> = MutexLockFuture<'a, R, T>;

pub type RwLockReadGuard<'a, R, T> = MutexGuard<'a, R, T>;
pub type RwLockWriteGuard<'a, R, T> = MutexGuard<'a, R, T>;

pub struct RwLock<R, T: ?Sized> {
    mutex: Mutex<R, T>,
}

impl<R: lock_api::RawMutex, T> RwLock<R, T> {
    pub fn new(value: T) -> Self {
        Self {
            mutex: Mutex::new(value),
        }
    }

    pub fn read(&self) -> RwLockReadFuture<'_, R, T> {
        self.mutex.lock()
    }

    pub fn write(&self) -> RwLockWriteFuture<'_, R, T> {
        self.mutex.lock()
    }
}
