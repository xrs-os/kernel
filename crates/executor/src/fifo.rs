use alloc::{collections::BTreeMap, sync::Arc, task::Wake};
use core::{pin::Pin, task::Context, task::Waker};
use crossbeam_queue::ArrayQueue;

use crate::Thread;

const TASK_QUEUE_FULL: &str = "task_queue full";

type Tasks<TRD> = BTreeMap<<TRD as Thread>::ID, (TRD, Option<Waker>)>;

pub struct FIFOExecutor<MutexType, TRD: Thread> {
    tasks: lock_api::Mutex<MutexType, Tasks<TRD>>,
    task_queue: Arc<ArrayQueue<TRD::ID>>,
}

impl<MutexType, TRD> FIFOExecutor<MutexType, TRD>
where
    MutexType: lock_api::RawMutex,
    TRD: Thread,
{
    pub fn new(queue_size: usize) -> Self {
        Self {
            tasks: lock_api::Mutex::new(BTreeMap::new()),
            task_queue: Arc::new(ArrayQueue::new(queue_size)),
        }
    }

    pub fn spawn(&self, thread: TRD) -> Option<()> {
        let task_id = thread.id().clone();
        let mut tasks = self.tasks.lock();

        if tasks.len() >= self.task_queue.capacity() {
            return None;
        }

        if tasks.insert(task_id.clone(), (thread, None)).is_some() {
            panic!("task with same ID already in tasks");
        }
        self.task_queue.push(task_id).map_or(Some(()), |_| None)
    }

    pub fn run_ready_tasks(&self) {
        let Self { tasks, task_queue } = self;
        while let Some(task_id) = task_queue.pop() {
            let mut tasks = tasks.lock();
            let (thread, waker_opt) = match tasks.get_mut(&task_id) {
                Some(tup) => tup,
                None => continue,
            };

            let waker = match waker_opt {
                Some(ref waker) => waker,
                None => {
                    *waker_opt = Some(self.waker(&task_id));
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

    pub fn waker(&self, task_id: &TRD::ID) -> Waker {
        TaskWaker::<TRD>::new(task_id.clone(), self.task_queue.clone()).waker()
    }
}

struct TaskWaker<TRD: Thread> {
    task_id: TRD::ID,
    task_queue: Arc<ArrayQueue<TRD::ID>>,
}

impl<TRD: Thread> TaskWaker<TRD> {
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

impl<TRD: Thread> Wake for TaskWaker<TRD> {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}
