use core::mem::{self, MaybeUninit};

use crate::cpu;

/// A spin-based lock providing mutually exclusive access to data.
/// And the `MutexIrq` will turn off interrupt when enters the critical section
/// and resumes interrupt on exit from the critical section.
pub struct MutexIrq<T: ?Sized>(spin::Mutex<T>);

unsafe impl<T: ?Sized + Send> Sync for MutexIrq<T> {}
unsafe impl<T: ?Sized + Send> Send for MutexIrq<T> {}

impl<T> MutexIrq<T> {
    pub const fn new(value: T) -> Self {
        Self(spin::Mutex::new(value))
    }

    /// Acquires a mutex
    pub fn lock(&self) -> MutexIrqGuard<'_, T> {
        // Call `cpu::push_off()` to turn off interrupt when locking
        cpu::push_off();
        MutexIrqGuard(Some(self.0.lock()))
    }

    pub fn try_lock(&self) -> Option<MutexIrqGuard<'_, T>> {
        // Call `cpu::push_off()` to turn off interrupt when locking
        cpu::push_off();
        match self.0.try_lock() {
            Some(guard) => Some(MutexIrqGuard(Some(guard))),
            None => {
                // Lock not acquired, resume interrupt state
                cpu::pop_off();
                None
            }
        }
    }
}

unsafe impl lock_api::RawMutex for MutexIrq<()> {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self::new(());
    type GuardMarker = lock_api::GuardSend;

    #[inline(always)]
    fn lock(&self) {
        // Call `cpu::push_off()` to turn off interrupt when locking
        cpu::push_off();
        <spin::Mutex<()> as lock_api::RawMutex>::lock(&self.0);
    }

    #[inline(always)]
    fn try_lock(&self) -> bool {
        // Call `cpu::push_off()` to turn off interrupt when locking
        cpu::push_off();
        if !<spin::Mutex<()> as lock_api::RawMutex>::try_lock(&self.0) {
            // Lock not acquired, resume interrupt state
            cpu::pop_off();
            false
        } else {
            true
        }
    }

    #[inline(always)]
    unsafe fn unlock(&self) {
        <spin::Mutex<()> as lock_api::RawMutex>::unlock(&self.0);
        // Call `cpu::pop_off()` to resume interrupt
        cpu::pop_off();
    }

    #[inline(always)]
    fn is_locked(&self) -> bool {
        <spin::Mutex<()> as lock_api::RawMutex>::is_locked(&self.0)
    }
}

/// A lock that provides data access to either one writer or many readers.
/// And the `RwLockIrq` will turn off interrupt when enters the critical section
/// and resumes interrupt on exit from the critical section.
pub struct RwLockIrq<T: ?Sized>(spin::RwLock<T>);

unsafe impl<T: ?Sized + Send> Send for RwLockIrq<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockIrq<T> {}

impl<T> RwLockIrq<T> {
    pub const fn new(value: T) -> Self {
        Self(spin::RwLock::new(value))
    }

    pub fn read(&self) -> RwLockReadIrqGuard<T> {
        cpu::push_off();
        RwLockReadIrqGuard(Some(self.0.read()))
    }

    pub fn try_read(&self) -> Option<RwLockReadIrqGuard<T>> {
        cpu::push_off();
        match self.0.try_read() {
            Some(guard) => Some(RwLockReadIrqGuard(Some(guard))),
            None => {
                // Lock not acquired, resume interrupt state
                cpu::pop_off();
                None
            }
        }
    }

    pub fn write(&self) -> RwLockWriteIrqGuard<T> {
        cpu::push_off();
        RwLockWriteIrqGuard(MaybeUninit::new(self.0.write()))
    }

    pub fn try_write(&self) -> Option<RwLockWriteIrqGuard<T>> {
        cpu::push_off();

        match self.0.try_write() {
            Some(guard) => Some(RwLockWriteIrqGuard(MaybeUninit::new(guard))),
            None => {
                // Lock not acquired, resume interrupt state
                cpu::pop_off();
                None
            }
        }
    }

    pub fn upgradeable_read(&self) -> RwLockUpgradableIrqGuard<T> {
        cpu::push_off();
        RwLockUpgradableIrqGuard(MaybeUninit::new(self.0.upgradeable_read()))
    }

    pub fn try_upgradeable_read(&self) -> Option<RwLockUpgradableIrqGuard<T>> {
        cpu::push_off();
        match self.0.try_upgradeable_read() {
            Some(guard) => Some(RwLockUpgradableIrqGuard(MaybeUninit::new(guard))),
            None => {
                // Lock not acquired, resume interrupt state
                cpu::pop_off();
                None
            }
        }
    }
}

unsafe impl lock_api::RawRwLock for RwLockIrq<()> {
    type GuardMarker = lock_api::GuardSend;

    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self::new(());

    #[inline(always)]
    fn lock_shared(&self) {
        cpu::push_off();
        <spin::RwLock<()> as lock_api::RawRwLock>::lock_shared(&self.0)
    }

    #[inline(always)]
    fn try_lock_shared(&self) -> bool {
        cpu::push_off();
        if !<spin::RwLock<()> as lock_api::RawRwLock>::try_lock_shared(&self.0) {
            // Lock not acquired, resume interrupt state
            cpu::pop_off();
            false
        } else {
            true
        }
    }

    #[inline(always)]
    unsafe fn unlock_shared(&self) {
        <spin::RwLock<()> as lock_api::RawRwLock>::unlock_shared(&self.0);
        // resume interrupt state
        cpu::pop_off();
    }

    #[inline(always)]
    fn lock_exclusive(&self) {
        cpu::push_off();
        <spin::RwLock<()> as lock_api::RawRwLock>::lock_exclusive(&self.0);
    }

    #[inline(always)]
    fn try_lock_exclusive(&self) -> bool {
        cpu::push_off();
        if !<spin::RwLock<()> as lock_api::RawRwLock>::try_lock_exclusive(&self.0) {
            // resume interrupt state
            cpu::pop_off();
            false
        } else {
            true
        }
    }

    #[inline(always)]
    unsafe fn unlock_exclusive(&self) {
        <spin::RwLock<()> as lock_api::RawRwLock>::unlock_exclusive(&self.0);
        // resume interrupt state
        cpu::pop_off();
    }

    #[inline(always)]
    fn is_locked(&self) -> bool {
        <spin::RwLock<()> as lock_api::RawRwLock>::is_locked(&self.0)
    }
}

pub struct MutexIrqGuard<'a, T>(Option<spin::MutexGuard<'a, T>>);
pub struct RwLockWriteIrqGuard<'a, T>(MaybeUninit<spin::RwLockWriteGuard<'a, T>>);
pub struct RwLockReadIrqGuard<'a, T>(Option<spin::RwLockReadGuard<'a, T>>);
pub struct RwLockUpgradableIrqGuard<'a, T>(MaybeUninit<spin::RwLockUpgradableGuard<'a, T>>);

impl<'a, T> RwLockWriteIrqGuard<'a, T> {
    pub fn downgrade(mut self) -> RwLockReadIrqGuard<'a, T> {
        let inner = mem::replace(&mut self.0, MaybeUninit::uninit());
        // Disbale drop
        mem::forget(self);
        RwLockReadIrqGuard(Some(unsafe { inner.assume_init() }.downgrade()))
    }

    pub fn downgrade_to_upgradeable(mut self) -> RwLockUpgradableIrqGuard<'a, T> {
        let inner = mem::replace(&mut self.0, MaybeUninit::uninit());
        // Disable drop
        mem::forget(self);
        RwLockUpgradableIrqGuard(MaybeUninit::new(
            unsafe { inner.assume_init() }.downgrade_to_upgradeable(),
        ))
    }
}

impl<'a, T> core::ops::Deref for RwLockWriteIrqGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.assume_init_ref() }.deref()
    }
}

impl<'a, T> core::ops::DerefMut for RwLockWriteIrqGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.assume_init_mut() }.deref_mut()
    }
}

impl<'a, T> RwLockUpgradableIrqGuard<'a, T> {
    pub fn upgrade(mut self) -> RwLockWriteIrqGuard<'a, T> {
        let inner = mem::replace(&mut self.0, MaybeUninit::uninit());
        // Disbale drop
        mem::forget(self);
        RwLockWriteIrqGuard(MaybeUninit::new(unsafe { inner.assume_init() }.upgrade()))
    }

    pub fn try_upgrade(mut self) -> core::result::Result<RwLockWriteIrqGuard<'a, T>, Self> {
        let inner = mem::replace(&mut self.0, MaybeUninit::uninit());
        // Disbale drop
        mem::forget(self);

        match unsafe { inner.assume_init() }.try_upgrade() {
            Ok(write_guard) => Ok(RwLockWriteIrqGuard(MaybeUninit::new(write_guard))),
            Err(upgradeable_read_guard) => Err(RwLockUpgradableIrqGuard(MaybeUninit::new(
                upgradeable_read_guard,
            ))),
        }
    }
}

impl<'a, T> core::ops::Deref for RwLockUpgradableIrqGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.assume_init_ref() }.deref()
    }
}

macro_rules! impl_drop_for_guard {
    ($name:ident) => {
        impl<'a, T> Drop for $name<'a, T> {
            fn drop(&mut self) {
                self.0.take();
                $crate::cpu::pop_off();
            }
        }
    };
}

macro_rules! impl_drop_for_maybe_uninit_guard {
    ($name:ident) => {
        impl<'a, T> Drop for $name<'a, T> {
            fn drop(&mut self) {
                unsafe { self.0.assume_init_drop() };
                $crate::cpu::pop_off();
            }
        }
    };
}

macro_rules! impl_deref_for_guard {
    ($name:ident) => {
        impl<'a, T> core::ops::Deref for $name<'a, T> {
            type Target = T;
            fn deref(&self) -> &Self::Target {
                self.0.as_ref().unwrap().deref()
            }
        }
    };
}

macro_rules! impl_deref_mut_for_guard {
    ($name:ident) => {
        impl<'a, T> core::ops::DerefMut for $name<'a, T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                self.0.as_mut().unwrap().deref_mut()
            }
        }
    };
}

impl_drop_for_guard!(MutexIrqGuard);
impl_drop_for_guard!(RwLockReadIrqGuard);
impl_drop_for_maybe_uninit_guard!(RwLockWriteIrqGuard);
impl_drop_for_maybe_uninit_guard!(RwLockUpgradableIrqGuard);

impl_deref_for_guard!(MutexIrqGuard);
impl_deref_for_guard!(RwLockReadIrqGuard);

impl_deref_mut_for_guard!(MutexIrqGuard);
