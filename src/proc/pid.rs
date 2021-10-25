use alloc::{collections::BTreeMap, sync::Arc};

use crate::spinlock::RwLockIrq;

use super::{tid, Proc};

pub struct Pid {
    proc: Arc<Proc>,
}

impl Pid {
    pub fn new(proc: Arc<Proc>) -> Self {
        Self { proc }
    }

    pub fn id(&self) -> &tid::RawThreadId {
        self.proc.id()
    }

    pub fn group(&self) -> &RwLockIrq<BTreeMap<tid::RawThreadId, Arc<Proc>>> {
        &self.proc.children
    }
}
