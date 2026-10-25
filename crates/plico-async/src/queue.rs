use plico_core::Entity;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::Wake,
};
pub(crate) struct ReadyQueue {
    queue: Mutex<VecDeque<Arc<ScopeWake>>>,
    notify: Box<dyn Fn() + Send + Sync>,
}
impl ReadyQueue {
    pub(crate) fn new(notify: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            queue: Mutex::default(),
            notify: Box::new(notify),
        }
    }
    pub(crate) fn has_ready(&self) -> bool {
        !self.queue.lock().unwrap().is_empty()
    }
    pub(crate) fn pop(&self) -> Option<Arc<ScopeWake>> {
        let wake = self.queue.lock().unwrap().pop_front()?;
        wake.queued.store(false, Ordering::Release);
        Some(wake)
    }
}
pub(crate) struct ScopeWake {
    pub(crate) entity: Entity,
    queue: Weak<ReadyQueue>,
    queued: AtomicBool,
    alive: AtomicBool,
}
impl ScopeWake {
    pub(crate) fn new(entity: Entity, queue: &Arc<ReadyQueue>) -> Arc<Self> {
        Arc::new(Self {
            entity,
            queue: Arc::downgrade(queue),
            queued: AtomicBool::new(false),
            alive: AtomicBool::new(true),
        })
    }
    pub(crate) fn close(&self) {
        self.alive.store(false, Ordering::Release);
    }
    pub(crate) fn schedule(self: &Arc<Self>) {
        if !self.alive.load(Ordering::Acquire) || self.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(queue) = self.queue.upgrade() {
            queue.queue.lock().unwrap().push_back(self.clone());
            (queue.notify)();
        }
    }
}
impl Wake for ScopeWake {
    fn wake(self: Arc<Self>) {
        self.schedule();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.schedule();
    }
}
