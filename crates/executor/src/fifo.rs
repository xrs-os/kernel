use alloc::{collections::BTreeMap, sync::Arc, task::Wake};
use core::{pin::Pin, task::Context, task::Waker};
use crossbeam_queue::ArrayQueue;

use crate::ThreadFuture;

const TASK_QUEUE_FULL: &str = "task_queue full";

type Tasks<TF> = BTreeMap<<TF as ThreadFuture>::ID, (TF, Option<Waker>)>;

pub struct FIFOExecutor<TF: ThreadFuture> {
    tasks: Tasks<TF>,
    task_queue: Arc<ArrayQueue<TF::ID>>,
}

impl<TF> FIFOExecutor<TF>
where
    TF: ThreadFuture,
{
    pub fn new(queue_size: usize) -> Self {
        Self {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(ArrayQueue::new(queue_size)),
        }
    }

    /// Returns the thread corresponding to the tid.
    pub fn thread(&self, tid: &TF::ID) -> Option<TF::Thread> {
        self.tasks.get(tid).map(|(x, _)| x.thread().clone())
    }

    pub fn spawn(&mut self, thread_fut: TF) -> Option<()> {
        let task_id = thread_fut.id().clone();
        let tasks = &mut self.tasks;

        if tasks.len() >= self.task_queue.capacity() {
            return None;
        }

        if tasks.insert(task_id.clone(), (thread_fut, None)).is_some() {
            panic!("task with same ID already in tasks");
        }
        self.task_queue.push(task_id).map_or(Some(()), |_| None)
    }

    pub fn run_ready_tasks(&mut self) {
        let Self { tasks, task_queue } = self;
        while let Some(task_id) = task_queue.pop() {
            let (thread, waker_opt) = match tasks.get_mut(&task_id) {
                Some(tup) => tup,
                None => continue,
            };

            let waker = match waker_opt {
                Some(ref waker) => waker,
                None => {
                    *waker_opt =
                        Some(TaskWaker::<TF>::new(task_id.clone(), task_queue.clone()).waker());
                    waker_opt.as_ref().unwrap()
                }
            };

            let mut context = Context::from_waker(waker);

            if unsafe { Pin::new_unchecked(thread) }
                .poll(&mut context)
                .is_ready()
            {
                // Remove from tasks and waker_cache when task is complete
                tasks.remove(&task_id);
            }
        }
    }

    pub fn waker(&self, task_id: &TF::ID) -> Waker {
        TaskWaker::<TF>::new(task_id.clone(), self.task_queue.clone()).waker()
    }
}

struct TaskWaker<TRD: ThreadFuture> {
    task_id: TRD::ID,
    task_queue: Arc<ArrayQueue<TRD::ID>>,
}

impl<TRD: ThreadFuture> TaskWaker<TRD> {
    fn new(task_id: TRD::ID, task_queue: Arc<ArrayQueue<TRD::ID>>) -> Self {
        Self {
            task_id,
            task_queue,
        }
    }
    fn waker(self) -> Waker {
        Waker::from(Arc::new(self))
    }

    fn wake_task(&self) {
        if self.task_queue.push(self.task_id.clone()).is_err() {
            panic!("{}", TASK_QUEUE_FULL);
        }
    }
}

impl<TRD: ThreadFuture> Wake for TaskWaker<TRD> {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}
