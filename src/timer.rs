use crate::{arch::interrupt, spinlock::MutexIrq};
use core::{
    future::Future,
    mem::MaybeUninit,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use naive_timer::Timer;

static mut NAIVE_TIMER: MaybeUninit<MutexIrq<Timer>> = MaybeUninit::uninit();

pub fn init() {
    unsafe { NAIVE_TIMER = MaybeUninit::new(MutexIrq::new(Timer::default())) }
}

pub fn on_timer(_kernel: bool) {
    let now = interrupt::timer_now();
    unsafe { NAIVE_TIMER.assume_init_ref().lock().expire(now) }
}

pub fn sleep(duration: Duration) -> SleepFuture {
    let now = interrupt::timer_now();
    SleepFuture {
        deadline: now + duration,
        first: true,
    }
}

pub struct SleepFuture {
    deadline: Duration,
    first: bool,
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if self.first {
            let waker = cx.waker().clone();
            unsafe { NAIVE_TIMER.assume_init_ref() }
                .lock()
                .add(self.deadline, move |_| waker.wake());
            self.as_mut().first = false;
            return Poll::Pending;
        }

        let now = interrupt::timer_now();
        return if now < self.deadline {
            Poll::Pending
        } else {
            Poll::Ready(())
        };
    }
}
