use core::{
    future::Future,
    marker::PhantomPinned,
    mem::{self, MaybeUninit},
    ops::Deref,
    pin::Pin,
    sync::atomic::{AtomicU8, Ordering},
    task::{ready, Context, Poll, Waker},
};

use alloc::{boxed::Box, string::String, sync::Arc};
use mm::{
    arch::page::PageParam as PageParamA,
    memory::{MapType, Segment},
    page::PageParam as _,
    Result as MemoryResult, VirtualAddress,
};

use super::{
    executor::waker,
    signal::{self, SignalContext},
    tid::{self, RawThreadId, ThreadId},
    Error, Proc, Result,
};
use crate::{
    arch::{
        interrupt::{Context as InterruptCtx, Trap},
        memory::{user_stack_offset, user_stack_size},
    },
    spinlock::RwLockIrq,
    syscall::syscall,
};
use pin_project::pin_project;

bitflags! {
    pub struct State: u8 {
        const RUNNING = 0b1;
        const INTERRUPTIBLE = 0b10;
        const UNINTERRUPTIBLE = 0b100;
        const WAKEKILL = 0b1000;
        const EXIT = 0b10000;
        const KILLABLE = Self::WAKEKILL.bits | Self::INTERRUPTIBLE.bits | Self::RUNNING.bits;
        const SLEEPPING = Self::INTERRUPTIBLE.bits| Self::UNINTERRUPTIBLE.bits|Self::WAKEKILL.bits;
    }
}

pub const FLAGS_SIG_STOPPING: u8 = 0b1;
pub const FLAGS_HAS_PENDDING_SIGS: u8 = 0b10;

pub struct ThreadInner {
    // Interrupt context, which holds the values of all CPU general registers
    // when a thread is interrupted.
    // Restore these registers when the thread returns to user state
    pub context: InterruptCtx,
    state: State,
    pub sig_alt_stack: signal::AltStack,
    pub sig_ctx: Option<SignalContext>,
}

impl ThreadInner {
    pub fn try_wake_up_state(&mut self, s: &State, waker_fn: impl Fn() -> Waker) -> bool {
        let origin_state = self.state;
        if !s.contains(origin_state) {
            return false;
        }
        waker_fn().wake();
        true
    }

    pub fn fork(&self) -> Self {
        let mut new_context = self.context.clone();
        new_context.set_syscall_ret(0);
        Self {
            context: new_context,
            state: self.state,
            sig_alt_stack: signal::AltStack::default(),
            sig_ctx: None,
        }
    }
}

pub struct Thread {
    // thread id
    tid: ThreadId,
    cmd: String,
    // The process to which the current thread belongs
    proc: MaybeUninit<Arc<Proc>>,
    /// FLAGS_xxx
    pub flags: AtomicU8,
    /// `sig_pending` holds the signal sent to this thread.
    /// the caller must hold proc.signal lock
    pub sig_pending: MaybeUnlock<signal::Pending>,
    pub inner: RwLockIrq<ThreadInner>,
}

impl Thread {
    pub fn id(&self) -> &RawThreadId {
        self.tid.id()
    }

    pub fn new(tid: ThreadId, cmd: impl Into<String>, entry_point: VirtualAddress) -> Self {
        let mut context = InterruptCtx::default();
        context.set_entry_point(entry_point);
        Self {
            tid,
            cmd: cmd.into(),
            proc: MaybeUninit::uninit(),
            flags: AtomicU8::new(0),
            sig_pending: MaybeUnlock(signal::Pending::new()),

            inner: RwLockIrq::new(ThreadInner {
                context,
                state: State::INTERRUPTIBLE,
                sig_alt_stack: signal::AltStack::default(),
                sig_ctx: None,
            }),
        }
    }

    pub unsafe fn init(&self, proc: Arc<Proc>) -> MemoryResult<()> {
        self.inner
            .write()
            .context
            .set_init_stack(Self::alloc_user_stack(&proc)?);

        #[allow(clippy::cast_ref_to_mut)]
        (*(self as *const Self as *mut Self)).proc = MaybeUninit::new(proc);
        Ok(())
    }

    pub fn fork(self: &Arc<Thread>, new_inner: ThreadInner) -> Result<Self> {
        let tid = tid::alloc().ok_or(Error::ThreadIdNotEnough)?;

        Ok(Self {
            proc: MaybeUninit::new(Arc::new(
                self.proc()
                    .fork(*tid.id() as usize, self.clone())
                    .map_err(Error::MemoryErr)?,
            )),
            cmd: self.cmd.clone(),
            tid,
            flags: AtomicU8::new(0),
            sig_pending: MaybeUnlock(signal::Pending::new()),
            inner: RwLockIrq::new(new_inner),
        })
    }

    // Allocate user stack, return stack pointer on success
    fn alloc_user_stack(proc: &Arc<Proc>) -> MemoryResult<VirtualAddress> {
        let mut memory = proc.memory.write();

        let stack_start = VirtualAddress(user_stack_offset() - user_stack_size());
        let stack_end = VirtualAddress(user_stack_offset());
        memory.add_segment(
            Segment {
                addr_range: stack_start..stack_end,
                flags: PageParamA::flag_set_user(
                    PageParamA::FLAG_PTE_READABLE | PageParamA::FLAG_PTE_WRITEABLE,
                ),
                map_type: MapType::Framed,
            },
            &[],
        )?;
        Ok(stack_end)
    }

    pub fn waker(&self) -> Waker {
        waker(self.id())
    }

    pub fn try_wake_up_state(&self, s: &State) -> bool {
        // TODO: This lock may be deadlocked in here and needs to be opened outside
        let mut inner = self.inner.write();
        inner.try_wake_up_state(s, || self.waker())
    }

    pub fn proc(&self) -> &Arc<Proc> {
        unsafe { self.proc.assume_init_ref() }
    }

    pub fn is_init(&self) -> bool {
        self.proc().is_init()
    }

    pub fn is_main_thread(&self) -> bool {
        self.id() == self.proc().id()
    }

    pub fn exit(&self, status: isize) {
        if self.is_main_thread() {
            // When the main thread exits, it should exit the corresponding process directly.
            self.proc().exit(status);
        }
        self.inner.write().state = State::EXIT;
    }
}

pub struct MaybeUnlock<T: ?Sized>(T);

impl<T> MaybeUnlock<T> {
    pub fn new(v: T) -> Self {
        Self(v)
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn assume_locked(&self) -> &mut T {
        #[allow(clippy::cast_ref_to_mut)]
        &mut *(&self.0 as *const T as *mut T)
    }
}

impl<DT, T: Deref<Target = DT>> Deref for MaybeUnlock<T> {
    type Target = DT;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

enum ThreadFutureState {
    RunUser,
    Syscall(Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>),
    Exit,
}

#[pin_project]
pub struct ThreadFuture {
    thread: Arc<Thread>,
    state: ThreadFutureState,
    _pin: PhantomPinned,
}

pub fn thread_future(thread: Arc<Thread>) -> ThreadFuture {
    ThreadFuture {
        thread,
        state: ThreadFutureState::RunUser,
        _pin: PhantomPinned,
    }
}

impl ThreadFuture {
    pub fn exit(self: Pin<&mut Self>) {
        self.get_mut().state = ThreadFutureState::Exit;
    }
}

impl Future for ThreadFuture {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::println!("thread polled");

        let this = self.project();

        if let ThreadFutureState::Exit = this.state {
            return Poll::Ready(());
        }

        let mut thread_inner = this.thread.inner.write();
        if thread_inner.state == State::EXIT {
            return Poll::Ready(());
        }

        let flags = this.thread.flags.load(Ordering::Acquire);
        if flags & FLAGS_SIG_STOPPING != 0 {
            return Poll::Pending;
        }

        if ready!(signal::handle_signal(this.thread, &mut thread_inner)) {
            if let ThreadFutureState::Syscall(syscall) =
                mem::replace(this.state, ThreadFutureState::RunUser)
            {
                if let Some(sig_ctx) = thread_inner.sig_ctx.as_mut() {
                    sig_ctx.syscall = Some(syscall);
                }
            }
        }
        drop(thread_inner);
        loop {
            *this.state = match this.state {
                ThreadFutureState::RunUser => {
                    // TODO: No need to reactivate if the current page table is this process
                    this.thread.proc().memory.read().activate();
                    let mut thread_ctx = this.thread.inner.write().context.clone();
                    let trap = unsafe { Box::from_raw(thread_ctx.run_user()) };
                    {
                        let mut thread_inner = this.thread.inner.write();
                        thread_inner.context = thread_ctx;
                        thread_inner.state = State::INTERRUPTIBLE;
                    }

                    match *trap {
                        Trap::PageFault(_) => todo!(),
                        Trap::Syscall => ThreadFutureState::Syscall(unsafe {
                            remove_future_lifetime(Box::new(syscall(this.thread)))
                        }),
                        Trap::Timer => {
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                        Trap::Interrupt => ThreadFutureState::RunUser,
                        Trap::Other => todo!(),
                    }
                }
                ThreadFutureState::Syscall(syscall_fut) => {
                    ready!(syscall_fut.as_mut().poll(cx));
                    let mut thread_inner = this.thread.inner.write();
                    if thread_inner.state == State::EXIT {
                        ThreadFutureState::Exit
                    } else {
                        if State::SLEEPPING.contains(thread_inner.state) {
                            thread_inner.state = State::RUNNING;
                        }
                        ThreadFutureState::RunUser
                    }
                }
                ThreadFutureState::Exit => return Poll::Ready(()),
            };
        }
    }
}

impl executor::Thread for ThreadFuture {
    type ID = RawThreadId;

    fn id(&self) -> &Self::ID {
        self.thread.id()
    }
}

unsafe fn remove_future_lifetime<'a, T>(
    f: Box<dyn Future<Output = T> + 'a>,
) -> Pin<Box<dyn Future<Output = T> + Send + Sync + 'static>> {
    Pin::new_unchecked(Box::from_raw(mem::transmute(Box::into_raw(f))))
}
