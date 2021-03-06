#![no_std]

use core::{future::Future, fmt::Debug};
extern crate alloc;

// executor implementation
// Reference https://os.phil-opp.com/async-await

#[cfg(feature = "fifo")]
pub mod fifo;

pub trait ThreadFuture: Future + 'static {
    type ID: Clone + Ord + Send + Sync + Debug;

    type Thread: Clone;

    fn id(&self) -> &Self::ID;

    fn thread(&self) -> &Self::Thread;
}

pub trait WaitForInterrupt {
    fn wfi();
}
