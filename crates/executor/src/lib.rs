#![no_std]

use core::future::Future;
extern crate alloc;

// executor implementation
// Reference https://os.phil-opp.com/async-await

#[cfg(feature = "fifo")]
pub mod fifo;

pub trait Thread: Future + 'static {
    type ID: Clone + Ord + Send + Sync;

    fn id(&self) -> &Self::ID;
}

pub trait WaitForInterrupt {
    fn wfi();
}
