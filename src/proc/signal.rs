use crate::{
    arch::signal::{set_signal_handler, Context as ArchSigCtx},
    spinlock::MutexIrq,
};
use core::{
    future::Future,
    iter,
    mem::{self, ManuallyDrop},
    pin::Pin,
    ptr,
    sync::atomic::Ordering,
    task::{ready, Poll, Waker},
};

use alloc::{
    boxed::Box,
    collections::{
        linked_list::{self, LinkedList},
        BTreeMap,
    },
    sync::Arc,
};

use futures_util::future::Either;
use mm::VirtualAddress;

use super::{
    process,
    thread::{
        self, State as ThreadState, Thread, ThreadInner, FLAGS_HAS_PENDDING_SIGS,
        FLAGS_SIG_STOPPING,
    },
    tid::{self, RawThreadId},
    Proc, SigBlocked,
};

pub type Result<T> = core::result::Result<T, Error>;

pub enum Error {
    InvalidArgs,
}

#[derive(Debug, Clone, Copy)]
pub struct SignalSet(u64);

impl SignalSet {
    pub fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn difference(&self, other: &Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[inline(always)]
    pub const fn sigmask(sig: &Signo) -> Self {
        Self(1 << (sig.to_primitive() as u64 - 1))
    }

    #[inline(always)]
    pub const fn contains(&self, sig: &Signo) -> bool {
        !Self::sigmask(sig).intersection(self).is_emptry()
    }

    #[inline(always)]
    pub const fn union(&self, other: &Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline(always)]
    pub const fn inv(&self) -> Self {
        Self(!self.0)
    }

    #[inline(always)]
    pub const fn intersection(&self, other: &Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn is_emptry(&self) -> bool {
        self.0 == 0
    }

    pub fn delset(&mut self, sig: &Signo) {
        self.0 &= !Self::sigmask(sig).0
    }

    pub fn min_sig(&self) -> Option<Signo> {
        Signo::from_primitive(self.0.leading_zeros() as u8 + 1)
    }
}

const SIG_HANDLER_DFL: usize = 0;
const SIG_HANDLER_IGN: usize = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub union SigHandler {
    pub handler: extern "C" fn(usize),
    pub info_handler: extern "C" fn(usize, *const Info),
}

impl SigHandler {
    pub fn default_handler() -> Self {
        Self {
            handler: unsafe { mem::transmute::<usize, _>(SIG_HANDLER_DFL) },
        }
    }

    pub fn is_ignored(&self, sig: &Signo) -> bool {
        let handler = self.as_usize();
        handler == SIG_HANDLER_IGN || (handler == SIG_HANDLER_DFL && sig.kernel_ignore())
    }

    pub fn is_default(&self) -> bool {
        self.as_usize() == SIG_HANDLER_DFL
    }

    fn as_usize(&self) -> usize {
        unsafe { mem::transmute::<_, usize>(self) }
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct SigAction {
    handler: Option<SigHandler>,
    pub flags: SigActionFlags,
    mask: SignalSet,
}

impl Default for SigAction {
    fn default() -> Self {
        Self {
            handler: None,
            flags: SigActionFlags::empty(),
            mask: SignalSet::empty(),
        }
    }
}

impl SigAction {
    pub fn handler(&self) -> SigHandler {
        self.handler
            .unwrap_or(unsafe { mem::transmute::<usize, SigHandler>(SIG_HANDLER_DFL) })
    }

    pub fn set_handler(&mut self, h: SigHandler) {
        self.handler = Some(h)
    }
}

bitflags! {
    pub struct SigActionFlags: usize {
        /// NOCLDSTOP flag to turn off SIGCHLD when children stop.
        const NOCLDSTOP = 0x00000001;
        /// NOCLDWAIT flag on SIGCHLD to inhibit zombies.
        const NOCLDWAIT = 0x00000002;
        /// SIGINFO delivers the signal with `Info` structs.
        const SIGINFO = 0x00000004;
        /// ONSTACK indicates that a registered `AltStack` will be used.
        const ONSTACK = 0x08000000;
        /// RESTART flag to get restarting signals (which were the default long ago)
        const RESTART = 0x10000000;
        /// NODEFER prevents the current signal from being masked in the handler.
        const NODEFER = 0x40000000;
        /// RESETHAND clears the handler when the signal is delivered.
        const RESETHAND = 0x80000000;
    }
}

pub fn do_sigaction(
    thread: Pin<&mut Thread>,
    sig: &Signo,
    mut act: SigAction,
) -> Result<SigAction> {
    if sig.kernel_only() {
        return Err(Error::InvalidArgs);
    }

    act.mask = act.mask.difference(
        &SignalSet::sigmask(&Signo::SIGKILL).union(&SignalSet::sigmask(&Signo::SIGSTOP)),
    );

    let act_handler_is_ignored = act.handler().is_ignored(sig);

    let proc = thread.proc();
    let mut proc_signal = proc.signal().lock();
    let old_act = proc_signal.replace_action(sig, act);

    if act_handler_is_ignored {
        // POSIX 3.3.1.3:
        //   "Setting a signal action to SIG_IGN for a signal that is
        //    pending shall cause the pending signal to be discarded,
        //    whether or not it is blocked."

        //   "Setting a signal action to SIG_DFL for a signal that is
        //    pending and whose default action is to ignore the signal
        //    (for example, SIGCHLD), shall cause the pending signal to
        //    be discarded, whether or not it is blocked"
        let mask = SignalSet::sigmask(sig);
        proc_signal.shared_pending.flush_by_mask(&mask);
        proc.threads
            .read()
            .iter()
            .for_each(|(_, t)| unsafe { t.sig_pending.assume_locked() }.flush_by_mask(&mask))
    }
    Ok(old_act)
}

pub struct Signal {
    // TODO When a thread exits, the corresponding waker needs to be deleted
    wakers: MutexIrq<SignalWakers>,
}

static mut SIGNAL: Signal = Signal {
    wakers: MutexIrq::new(SignalWakers(BTreeMap::new())),
};

pub fn signal() -> &'static mut Signal {
    unsafe { &mut SIGNAL }
}

pub fn handle_signal(thread: &Arc<Thread>, thread_inner: &mut ThreadInner) -> Poll<bool> {
    signal().handle_signal(thread, thread_inner)
}

struct SignalWakers(BTreeMap<RawThreadId, Waker>);

impl SignalWakers {
    pub fn contains(&self, tid: &RawThreadId) -> bool {
        self.0.contains_key(tid)
    }

    pub fn get(&self, tid: &RawThreadId) -> Option<&Waker> {
        self.0.get(tid)
    }

    pub fn insert(&mut self, tid: RawThreadId, w: Waker) {
        self.0.insert(tid, w);
    }
}

pub enum SendTo<'a> {
    /// Send a signal to the process that's member of process group.
    /// kill(pid, sig), pid <= 0
    ProcGroup(&'a Arc<Proc>),
    /// Send a signal to the thread.
    /// kill(pid, sig), pid > 0
    Thread(&'a Arc<Thread>),
}

impl Signal {
    fn get_signal(&self, thread: &Arc<Thread>) -> Poll<Option<(SigAction, Info)>> {
        let mut proc_signal = thread.proc().signal().lock();
        let pending = unsafe { thread.sig_pending.assume_locked() };
        let blocked = proc_signal.blocked.blocked;

        let (act, info) = loop {
            let (mut info_opt, mut only_one) = dequeue_signal(pending, &blocked);
            if info_opt.is_none() {
                let (shared_info, shared_only_one) =
                    dequeue_signal(&mut proc_signal.shared_pending, &blocked);
                info_opt = shared_info;
                only_one = shared_only_one;
            }

            match info_opt {
                None => {
                    return Poll::Ready(None);
                }
                Some(info) => {
                    if only_one && !has_pendding_sigs(&pending.signal, &proc_signal) {
                        // remove FLAGS_SIG_STOPPING thread flag
                        let flags = thread.flags.load(Ordering::Acquire);
                        if flags & FLAGS_HAS_PENDDING_SIGS != 0 {
                            thread
                                .flags
                                .store(flags & !FLAGS_HAS_PENDDING_SIGS, Ordering::Release);
                        }
                    }

                    let act = proc_signal.action_mut(&info.sig);
                    if act.handler().is_ignored(&info.sig) {
                        continue;
                    }
                    let ret_act = act.clone();
                    if !act.handler().is_default() && act.flags.contains(SigActionFlags::RESETHAND)
                    {
                        act.set_handler(SigHandler::default_handler())
                    }

                    if proc_signal.flags.contains(SignalFlags::UNKILLABLE)
                        && !info.sig.kernel_only()
                    {
                        // UNKILLABLE proc accept kernel signals only.
                        continue;
                    }

                    if info.sig.kernel_stop() {
                        let mut wakers = self.wakers.lock();
                        thread
                            .proc()
                            .threads
                            .read()
                            .iter()
                            .for_each(|(_, t)| do_sig_stop(t, &mut wakers));

                        return Poll::Pending;
                    }

                    break (ret_act, info);
                }
            }
        };

        Poll::Ready(Some((act, info)))
    }

    /// Handle signals. returns Poll::Ready(true) if the thread has signals.
    pub fn handle_signal(
        &self,
        thread: &Arc<Thread>,
        thread_inner: &mut ThreadInner,
    ) -> Poll<bool> {
        let mut interr_ctx = &mut thread_inner.context;
        if let Some((act, info)) = ready!(self.get_signal(thread)) {
            let signo = info.sig;
            let (sig_sp, info_user_ptr) = if act.flags.contains(SigActionFlags::SIGINFO) {
                let sig_sp = thread_inner.sig_alt_stack.sp;
                (sig_sp, copy_info_to_user(sig_sp, info) as *const _)
            } else {
                (interr_ctx.sp(), ptr::null())
            };

            let sig_ctx = SignalContext {
                arch_ctx: ArchSigCtx::from_interr_ctx(interr_ctx),
                syscall: None,
            };
            thread_inner.sig_ctx = Some(sig_ctx);
            set_signal_handler(
                &mut interr_ctx,
                sig_sp,
                act.handler().as_usize(),
                act.flags,
                signo.to_primitive() as usize,
                info_user_ptr,
            );
            Poll::Ready(true)
        } else {
            Poll::Ready(false)
        }
    }

    /// send_signal returns Err if signal queue overflow.
    pub fn send_signal(
        &self,
        sig: Signo,
        info: Info,
        send_to: SendTo,
    ) -> core::result::Result<(), Info> {
        let proc = match send_to {
            SendTo::ProcGroup(proc) => proc,
            SendTo::Thread(thread) => thread.proc(),
        };

        let mut proc_signal = proc.signal().lock();

        // Should the signal be ignored?
        if !self.prepare_signal(sig, proc, &mut proc_signal) {
            return Ok(());
        }

        let pending = match send_to {
            SendTo::ProcGroup(_) => &mut proc_signal.shared_pending,
            SendTo::Thread(thread) => unsafe { thread.sig_pending.assume_locked() },
        };

        if sig.legacy() && pending.contains(&sig) {
            return Ok(());
        }

        pending.push(info)?;
        self.signal_wakeup(&sig, &send_to, &mut proc_signal);
        Ok(())
    }

    /// Returns true if the signal should be actually delivered, otherwise
    /// it should be dropped.
    fn prepare_signal(&self, sig: Signo, proc: &Arc<Proc>, proc_signal: &mut process::Signal) -> bool {
        if sig.kernel_stop() {
            // This is a stop signal.  Remove SIGCONT from all queues.
            let flush = SignalSet::sigmask(&Signo::SIGCONT);

            proc_signal.shared_pending.flush_by_mask(&flush);
            proc.threads
                .read()
                .iter()
                .for_each(|(_, t)| unsafe { t.sig_pending.assume_locked() }.flush_by_mask(&flush))
        } else if sig == Signo::SIGCONT {
            // Remove all stop signals from all queues, wake all threads.
            proc_signal
                .shared_pending
                .flush_by_mask(&Signo::MASK_SIG_KERNEL_STOP);
            let wakers = self.wakers.lock();
            proc.threads.read().iter().for_each(|(_, t)| {
                unsafe { t.sig_pending.assume_locked() }
                    .flush_by_mask(&Signo::MASK_SIG_KERNEL_STOP);
                let thread_flags = t.flags.load(Ordering::Acquire);
                // remove FLAGS_SIG_STOPPING thread flag
                if thread_flags & FLAGS_SIG_STOPPING != 0 {
                    t.flags
                        .store(thread_flags & !FLAGS_SIG_STOPPING, Ordering::Release);
                }
                if let Some(w) = wakers.get(t.id()) {
                    w.wake_by_ref()
                }
            })
        }

        !sig_ignored(&sig, proc_signal, proc.is_init())
    }

    fn signal_wakeup(&self, sig: &Signo, send_to: &SendTo, proc_signal: &mut process::Signal) {
        let wants_signal_fn = wants_signal_fn(self.thread_is_stop_fn());

        let (target, proc) = match send_to {
            SendTo::ProcGroup(proc) => {
                let mut t = None;
                for thread in thread_iter(&*proc.threads.read(), proc_signal.current_target) {
                    if wants_signal_fn(sig, thread, &proc_signal.blocked) {
                        proc_signal.current_target = Some(*thread.id());
                        t = Some(thread.clone());
                        break;
                    }
                }
                (t, *proc)
            }
            SendTo::Thread(thread) => (
                if wants_signal_fn(sig, thread, &proc_signal.blocked) {
                    Some((*thread).clone())
                } else {
                    None
                },
                thread.proc(),
            ),
        };

        let target_thread = match target {
            None => return,
            Some(thread) => thread,
        };
        if sig_fatal(sig, proc_signal.action(sig))
            && !proc_signal.blocked.real_blocked.contains(sig)
        {
            // This signal will be fatal to the whole thread group.
            proc.threads.read().iter().for_each(|(_, t)| {
                t.try_wake_up_state(&ThreadState::KILLABLE);
                let flags = t.flags.load(Ordering::Acquire);
                if flags & FLAGS_SIG_STOPPING == 0 {
                    t.flags.store(flags & FLAGS_SIG_STOPPING, Ordering::Release);
                }
            });
            return;
        }

        target_thread.try_wake_up_state(if sig == &Signo::SIGKILL {
            &ThreadState::KILLABLE
        } else {
            &ThreadState::INTERRUPTIBLE
        });
        let flags = target_thread.flags.load(Ordering::Acquire);
        if flags & FLAGS_SIG_STOPPING == 0 {
            target_thread
                .flags
                .store(flags & FLAGS_SIG_STOPPING, Ordering::Release);
        }
    }

    fn thread_is_stop_fn(&self) -> impl Fn(&RawThreadId) -> bool + '_ {
        let wakers = self.wakers.lock();
        move |tid| wakers.contains(tid)
    }
}

pub fn copy_info_to_user(sig_sp: usize, info: Info) -> *mut Info {
    let info_user_addr = sig_sp - mem::size_of::<Info>();

    let info_ptr = info_user_addr as *mut Info;
    unsafe {
        ptr::write(info_ptr, info);
    };
    info_ptr
}

fn dequeue_signal(pending: &mut Pending, mask: &SignalSet) -> (Option<Info>, bool) {
    let mut s = pending.signal.difference(mask);
    let target_sig = if !s.is_emptry() {
        // Synchronous signals should be dequeued first.
        let sync = s.intersection(&Signo::MASK_SIG_SYNCHRONOUS);
        if !sync.is_emptry() {
            s = sync;
        }
        s.min_sig().unwrap()
    } else {
        return (None, false);
    };

    let mut target_kernel_info_opt: *mut linked_list::CursorMut<'_, Info> = ptr::null_mut();
    let mut target_info_opt: *mut linked_list::CursorMut<'_, Info> = ptr::null_mut();
    let mut only_one_target = true;

    let mut x = pending.queue.cursor_front_mut();

    while let Some(info) = x.current() {
        if info.sig == target_sig {
            if info.code > SI_USER {
                match unsafe { target_kernel_info_opt.as_mut() } {
                    None => target_kernel_info_opt = &mut x as *mut _,
                    Some(_) => {
                        only_one_target = false;
                        break;
                    }
                }
            } else {
                match unsafe { target_info_opt.as_mut() } {
                    None => target_info_opt = &mut x as *mut _,
                    Some(_) => only_one_target = false,
                }
            }
        }

        x.move_next();
    }

    let target_info = match unsafe { (target_kernel_info_opt.as_mut(), target_info_opt.as_mut()) } {
        (Some(target), Some(_)) => {
            only_one_target = false;
            target
        }
        (None, Some(target)) => target,
        (Some(target), None) => target,
        (None, None) => return (None, false),
    };

    if only_one_target {
        pending.signal.delset(&target_info.current().unwrap().sig);
    }

    target_info.remove_current();
    (target_info.current().cloned(), only_one_target)
}

fn has_pendding_sigs(thread_pending_signal: &SignalSet, proc_signal: &process::Signal) -> bool {
    let blocked = &proc_signal.blocked.blocked;

    thread_pending_signal.difference(blocked).is_emptry()
        && proc_signal
            .shared_pending
            .signal
            .difference(blocked)
            .is_emptry()
}

fn do_sig_stop(thread: &Arc<Thread>, signal_wakers: &mut SignalWakers) {
    let mut flags = thread.flags.load(Ordering::Acquire);
    if flags & thread::FLAGS_SIG_STOPPING != 0 {
        return;
    }
    signal_wakers.insert(*thread.id(), thread.waker());
    flags &= thread::FLAGS_SIG_STOPPING;
    thread.flags.store(flags, Ordering::Release);
}

fn sig_ignored(sig: &Signo, proc_signal: &process::Signal, is_init_proc: bool) -> bool {
    // Blocked signals are never ignored,
    // since the signal handler may change by the time it is unblocked.
    if proc_signal.blocked.blocked.contains(sig) || proc_signal.blocked.real_blocked.contains(sig) {
        return false;
    }

    let handler = proc_signal.action(sig).handler();

    // SIGKILL and SIGSTOP may not be sent to the global init
    if is_init_proc && sig.kernel_only() {
        return true;
    }

    if proc_signal.flags.contains(SignalFlags::UNKILLABLE)
        && handler.is_default()
        && !sig.kernel_only()
    {
        return true;
    }

    handler.is_ignored(sig)
}

fn wants_signal_fn(
    thread_is_stop_fn: impl Fn(&RawThreadId) -> bool,
) -> impl Fn(&Signo, &Arc<Thread>, &SigBlocked) -> bool {
    move |sig, thread, sig_blocked| -> bool {
        if sig_blocked.blocked.contains(sig) {
            return false;
        }
        if sig == &Signo::SIGKILL {
            return true;
        }

        if thread_is_stop_fn(thread.id()) {
            return false;
        }

        let flags = thread.flags.load(Ordering::Acquire);
        flags & FLAGS_SIG_STOPPING == 0
    }
}

fn thread_iter(
    threads: &BTreeMap<RawThreadId, Arc<Thread>>,
    current_tid: Option<RawThreadId>,
) -> impl Iterator<Item = &'_ Arc<Thread>> {
    let mut it = match current_tid {
        Some(current_tid) => Either::Right(
            threads
                .iter()
                .skip_while(move |(&tid, _)| tid == current_tid),
        ),
        None => Either::Left(threads.iter()),
    };

    iter::from_fn(move || {
        loop {
            match &mut it {
                Either::Left(it_left) => match it_left.next() {
                    Some((_, thread)) => return Some(thread),
                    None => it = Either::Left(threads.iter()),
                },
                Either::Right(it_right) => {
                    return it_right.next().map(|(_, thread)| thread);
                }
            }
        }
    })
}

num_enum::num_enum! (
    pub Signo:u8 {
        SIGHUP = 1,
        SIGINT = 2,
        SIGQUIT = 3,
        SIGILL = 4,
        SIGTRAP = 5,
        SIGABRT = 6,
        SIGBUS = 7,
        SIGFPE = 8,
        SIGKILL = 9,
        SIGUSR1 = 10,
        SIGSEGV = 11,
        SIGUSR2 = 12,
        SIGPIPE = 13,
        SIGALRM = 14,
        SIGTERM = 15,
        SIGSTKFLT = 16,
        SIGCHLD = 17,
        SIGCONT = 18,
        SIGSTOP = 19,
        SIGTSTP = 20,
        SIGTTIN = 21,
        SIGTTOU = 22,
        SIGURG = 23,
        SIGXCPU = 24,
        SIGXFSZ = 25,
        SIGVTALRM = 26,
        SIGPROF = 27,
        SIGWINCH = 28,
        SIGIO = 29,
        SIGPWR = 30,
        SIGSYS = 31,
        SIGRTMIN = 32,
        SIGRT33 = 33,
        SIGRT34 = 34,
        SIGRT35 = 35,
        SIGRT36 = 36,
        SIGRT37 = 37,
        SIGRT38 = 38,
        SIGRT39 = 39,
        SIGRT40 = 40,
        SIGRT41 = 41,
        SIGRT42 = 42,
        SIGRT43 = 43,
        SIGRT44 = 44,
        SIGRT45 = 45,
        SIGRT46 = 46,
        SIGRT47 = 47,
        SIGRT48 = 48,
        SIGRT49 = 49,
        SIGRT50 = 50,
        SIGRT51 = 51,
        SIGRT52 = 52,
        SIGRT53 = 53,
        SIGRT54 = 54,
        SIGRT55 = 55,
        SIGRT56 = 56,
        SIGRT57 = 57,
        SIGRT58 = 58,
        SIGRT59 = 59,
        SIGRT60 = 60,
        SIGRT61 = 61,
        SIGRT62 = 62,
        SIGRT63 = 63,
        SIGRTMAX = 64,
    }
);

impl Signo {
    pub const MASK_SIG_KERNEL_ONLY: SignalSet =
        SignalSet::sigmask(&Signo::SIGKILL).union(&SignalSet::sigmask(&Signo::SIGSTOP));

    #[inline(always)]
    pub const fn kernel_only(&self) -> bool {
        Self::MASK_SIG_KERNEL_ONLY.contains(self)
    }

    pub const MASK_SIG_KERNEL_IGNORE: SignalSet = SignalSet::sigmask(&Signo::SIGCONT)
        .union(&SignalSet::sigmask(&Signo::SIGCHLD))
        .union(&SignalSet::sigmask(&Signo::SIGWINCH))
        .union(&SignalSet::sigmask(&Signo::SIGURG));

    #[inline(always)]
    pub const fn kernel_ignore(&self) -> bool {
        Self::MASK_SIG_KERNEL_IGNORE.contains(self)
    }

    pub const MASK_SIG_KERNEL_STOP: SignalSet = SignalSet::sigmask(&Signo::SIGSTOP)
        .union(&SignalSet::sigmask(&Signo::SIGTSTP))
        .union(&SignalSet::sigmask(&Signo::SIGTTIN))
        .union(&SignalSet::sigmask(&Signo::SIGTTOU));

    #[inline(always)]
    pub const fn kernel_stop(&self) -> bool {
        Self::MASK_SIG_KERNEL_STOP.contains(self)
    }

    pub const MASK_SIG_SYNCHRONOUS: SignalSet = SignalSet::sigmask(&Signo::SIGSEGV)
        .union(&SignalSet::sigmask(&Signo::SIGBUS))
        .union(&SignalSet::sigmask(&Signo::SIGILL))
        .union(&SignalSet::sigmask(&Signo::SIGTRAP))
        .union(&SignalSet::sigmask(&Signo::SIGFPE))
        .union(&SignalSet::sigmask(&Signo::SIGSYS));

    #[inline(always)]
    pub const fn synchronous(&self) -> bool {
        Self::MASK_SIG_SYNCHRONOUS.contains(self)
    }

    pub fn legacy(&self) -> bool {
        self <= &Self::SIGRTMIN
    }
}

fn sig_fatal(sig: &Signo, action: &SigAction) -> bool {
    !Signo::MASK_SIG_KERNEL_IGNORE
        .union(&Signo::MASK_SIG_KERNEL_STOP)
        .contains(sig)
        && action.handler().is_default()
}

/// Signal count
pub const NSIG: u8 = Signo::SIGRTMAX as u8;

bitflags! {
    pub struct SignalFlags: usize {
        const UNKILLABLE = 0x00000040;
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct Info {
    pub sig: Signo,
    pub errno: usize,
    pub code: isize,
    fields: InfoFields,
}

/// si_code values
/// Digital reserves positive values for kernel-generated signals.

/// sent by kill, sigsend, raise
pub const SI_USER: isize = 0;
/// sent by the kernel
pub const SI_KERNEL: isize = 0x80;
/// sent by sigqueue
pub const SI_QUEUE: isize = -1;
/// sent by timer expiration
pub const SI_TIMER: isize = -2;
/// sent by real time mesq state change
pub const SI_MESGQ: isize = -3;
/// sent by AIO completion
pub const SI_ASYNCIO: isize = -4;
/// sent by queued SIGIO
pub const SI_SIGIO: isize = -5;
/// sent by tkill system call
pub const SI_TKILL: isize = -6;
/// sent by execve() killing subsidiary threads
pub const SI_DETHREAD: isize = -7;
/// sent by glibc async name lookup completion
pub const SI_ASYNCNL: isize = -60;

#[repr(C)]
#[derive(Clone, Copy)]
pub union InfoFields {
    /// Kill
    kill: ManuallyDrop<InfoFieldsKill>,
    /// POSIX.1b signals
    rt: ManuallyDrop<InfoFieldsRt>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InfoFieldsKill {
    /// Sender's pid
    pid: tid::RawThreadId,
    /// Sender's uid
    uid: u32,
}

/// POSIX.1b signals
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InfoFieldsRt {
    /// Sender's pid
    pid: tid::RawThreadId,
    /// Sender's uid
    uid: u32,
    val: InfoValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union InfoValue {
    int: isize,
    ptr: VirtualAddress,
}

const SIGPENDING_QUEUE_CAP: usize = 11;

pub struct Pending {
    signal: SignalSet,
    queue: LinkedList<Info>,
}

impl Pending {
    pub fn new() -> Self {
        Self {
            signal: SignalSet::empty(),
            queue: LinkedList::new(),
        }
    }

    /// Remove signals in mask from the pending set and queue.
    fn flush_by_mask(&mut self, mask: &SignalSet) {
        let m = self.signal.intersection(mask);
        if m.is_emptry() {
            return;
        }

        self.signal = self.signal.difference(mask);

        let mut queue_cursor = self.queue.cursor_front_mut();

        while let Some(sig_info) = queue_cursor.current() {
            if mask.contains(&sig_info.sig) {
                queue_cursor.remove_current();
            }
            queue_cursor.move_next();
        }
    }

    fn contains(&self, sig: &Signo) -> bool {
        self.signal.contains(sig)
    }

    fn push(&mut self, info: Info) -> core::result::Result<(), Info> {
        if self.queue.len() >= SIGPENDING_QUEUE_CAP {
            return Err(info);
        }

        if self.signal.contains(&info.sig) {
            self.signal = self.signal.union(&SignalSet::sigmask(&info.sig));
        }
        self.queue.push_back(info);

        Ok(())
    }
}

pub struct SignalContext {
    pub arch_ctx: ArchSigCtx,
    pub syscall: Option<Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>>,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct AltStack {
    /// Base address of stack
    pub sp: usize,
    /// Number of bytes in stack
    pub size: usize,
}

impl AltStack {
    pub fn on_stack(&self, sp: usize) -> bool {
        sp <= self.sp && sp > self.sp - self.size
    }
}
