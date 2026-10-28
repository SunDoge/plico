//! Host completion mailbox: no executor and no UI references cross threads.
use super::{Binding, ComponentId, Regions, State};
use plico_renderer::WakeCallback;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
};

trait Completion: Send {
    fn apply(self: Box<Self>, app: &Regions);
}
struct Set<T> {
    state: State<T>,
    value: T,
}
impl<T: Send + 'static> Completion for Set<T> {
    fn apply(self: Box<Self>, app: &Regions) {
        if let Some(state) = app.try_access(self.state) {
            state.set(self.value);
        }
    }
}
#[derive(Default)]
struct Inbox {
    live: HashSet<ComponentId>,
    pending: HashMap<Binding, Box<dyn Completion>>,
    wake: Option<WakeCallback>,
}
#[derive(Default)]
pub(super) struct Mailbox(Mutex<Inbox>);
impl Mailbox {
    pub(super) fn register(&self, component: ComponentId) {
        self.0.lock().unwrap().live.insert(component);
    }
    pub(super) fn remove(&self, component: ComponentId) {
        // Drop user payloads outside the lock (including their destructors).
        let discarded = {
            let mut inbox = self.0.lock().unwrap();
            inbox.live.remove(&component);
            let keys: Vec<_> = inbox
                .pending
                .keys()
                .filter(|key| key.1 == component)
                .copied()
                .collect();
            keys.into_iter()
                .filter_map(|key| inbox.pending.remove(&key))
                .collect::<Vec<_>>()
        };
        drop(discarded);
    }
    pub(super) fn set_wake(&self, wake: WakeCallback) {
        let pending = {
            let mut inbox = self.0.lock().unwrap();
            inbox.wake = Some(Arc::clone(&wake));
            !inbox.pending.is_empty()
        };
        if pending {
            wake();
        }
    }
    pub(super) fn pending(&self) -> bool {
        !self.0.lock().unwrap().pending.is_empty()
    }
    pub(super) fn drain(&self, app: &Regions) -> bool {
        let pending = std::mem::take(&mut self.0.lock().unwrap().pending);
        let changed = !pending.is_empty();
        for completion in pending.into_values() {
            completion.apply(app);
        }
        changed
    }
}
/// A non-owning, thread-safe endpoint for a host task's latest result.
///
/// `send` queues a replacement; the UI thread applies it on its next poll.
/// Repeated sends before that poll coalesce per state. This is not an event
/// queue: use a collection/task status as the value when every result matters.
/// Returns the original value if the component or runtime has been destroyed.
/// Successful enqueue does not guarantee delivery if it unmounts before polling.
pub struct StateSender<T> {
    mailbox: Weak<Mailbox>,
    state: State<T>,
}
impl<T> Clone for StateSender<T> {
    fn clone(&self) -> Self {
        Self {
            mailbox: self.mailbox.clone(),
            state: self.state,
        }
    }
}
impl<T: Send + 'static> StateSender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        let Some(mailbox) = self.mailbox.upgrade() else {
            return Err(value);
        };
        let (replaced, wake) = {
            let mut inbox = mailbox.0.lock().unwrap();
            if !inbox.live.contains(&self.state.component) {
                return Err(value);
            }
            let was_empty = inbox.pending.is_empty();
            let key = (self.state.owner, self.state.component, self.state.index);
            let replaced = inbox.pending.insert(
                key,
                Box::new(Set {
                    state: self.state,
                    value,
                }),
            );
            (replaced, was_empty.then(|| inbox.wake.clone()).flatten())
        };
        drop(replaced);
        if let Some(wake) = wake {
            wake();
        }
        Ok(())
    }
}
impl Regions {
    pub fn sender<T: Send + 'static>(&self, state: State<T>) -> StateSender<T> {
        assert!(self.try_access(state).is_some(), "foreign or expired state");
        StateSender {
            mailbox: Arc::downgrade(&self.mailbox),
            state,
        }
    }
}
