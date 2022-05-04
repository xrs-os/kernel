use core::{
    future::Future,
    mem::MaybeUninit,
    sync::atomic::AtomicUsize,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};

use alloc::{sync::Arc, task::Wake};
use executor::fifo::FIFOExecutor;
use futures_util::pin_mut;

use crate::arch::interrupt;

use super::thread::{Thread, ThreadFuture};

static mut GLOBAL_EXECUTOR: MaybeUninit<FIFOExecutor<ThreadFuture>> = MaybeUninit::uninit();

pub fn init() {
    unsafe { GLOBAL_EXECUTOR = MaybeUninit::new(FIFOExecutor::new(100)) }
}

fn executor() -> &'static mut FIFOExecutor<ThreadFuture> {
    unsafe { GLOBAL_EXECUTOR.assume_init_mut() }
}

pub fn spawn(thread: ThreadFuture) -> Option<()> {
    executor().spawn(thread)
}

struct Wfi;

impl executor::WaitForInterrupt for Wfi {
    fn wfi() {
        unsafe { interrupt::wfi() };
    }
}

pub fn run_ready_tasks() {
    executor().run_ready_tasks()
}

pub fn waker(tid: &<ThreadFuture as executor::ThreadFuture>::ID) -> Waker {
    executor().waker(tid)
}

/// Returns the thread corresponding to the tid.
pub fn thread(tid: &<ThreadFuture as executor::ThreadFuture>::ID) -> Option<Arc<Thread>> {
    executor().thread(tid)
}

struct BlockOnWaker {
    wake_times: Arc<AtomicUsize>,
}

impl Wake for BlockOnWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref()
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_times.fetch_add(1, Ordering::Release);
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    let wake_times = Arc::new(AtomicUsize::new(1));
    let waker = Waker::from(Arc::new(BlockOnWaker {
        wake_times: wake_times.clone(),
    }));
    let mut cx = Context::from_waker(&waker);
    pin_mut!(fut);
    loop {
        if wake_times.load(Ordering::Acquire) > 0 {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => {
                    wake_times.fetch_sub(1, Ordering::Release);
                }
            }
        }
        unsafe { interrupt::wfi() };
    }
}
