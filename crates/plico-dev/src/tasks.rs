//! Host-owned one-shot tasks. Spawn the returned future on your existing executor;
//! call `poll` after a host wake, then expose a value-only service snapshot to UI.
use crate::services::Wake;
use std::{
    future::Future,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus<T, E> {
    #[default]
    Idle,
    Pending,
    Ready(Result<T, E>),
    Cancelled,
}
enum Completion<T, E> {
    Ready(Result<T, E>),
    Cancelled,
}
struct Mailbox<T, E> {
    active: bool,
    result: Option<Completion<T, E>>,
}
struct Sender<T, E> {
    mailbox: Arc<Mutex<Mailbox<T, E>>>,
    wake: Arc<Mutex<Wake>>,
}
impl<T, E> Sender<T, E> {
    fn send(&self, result: Completion<T, E>) {
        let notify = {
            let mut slot = self.mailbox.lock().expect("task mailbox poisoned");
            if slot.active && slot.result.is_none() {
                slot.result = Some(result);
                true
            } else {
                false
            }
        };
        if notify {
            let wake = Arc::clone(&self.wake.lock().expect("task wake poisoned"));
            wake();
        }
    }
}
impl<T, E> Drop for Sender<T, E> {
    fn drop(&mut self) {
        self.send(Completion::Cancelled);
    }
}

/// This belongs to the host service, never a reloadable app or serialized state.
/// Replacement/cancellation invalidates the old completion; it does not undo IO.
pub struct Task<T, E> {
    status: TaskStatus<T, E>,
    mailbox: Option<Arc<Mutex<Mailbox<T, E>>>>,
    wake: Arc<Mutex<Wake>>,
}
impl<T, E> Task<T, E> {
    pub fn new(wake: Wake) -> Self {
        Self {
            status: TaskStatus::Idle,
            mailbox: None,
            wake: Arc::new(Mutex::new(wake)),
        }
    }
    /// Forward HostService::set_wake_callback here, including for pending tasks.
    pub fn set_wake_callback(&mut self, wake: Wake) {
        *self.wake.lock().expect("task wake poisoned") = wake;
    }
    pub fn status(&self) -> &TaskStatus<T, E> {
        &self.status
    }
    pub fn pending(&self) -> bool {
        matches!(self.status, TaskStatus::Pending)
    }
    fn invalidate(&mut self) {
        if let Some(mailbox) = self.mailbox.take() {
            mailbox.lock().expect("task mailbox poisoned").active = false;
        }
    }
    /// The caller must spawn/poll this future. Dropping it reports cancellation.
    /// The caller can also abort its executor handle to stop polling IO early.
    pub fn start<F: Future<Output = Result<T, E>>>(
        &mut self,
        future: F,
    ) -> impl Future<Output = ()> + use<T, E, F> {
        self.invalidate();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            active: true,
            result: None,
        }));
        let sender = Sender {
            mailbox: Arc::clone(&mailbox),
            wake: Arc::clone(&self.wake),
        };
        self.mailbox = Some(mailbox);
        self.status = TaskStatus::Pending;
        async move {
            sender.send(Completion::Ready(future.await));
        }
    }
    pub fn cancel(&mut self) {
        if self.pending() {
            self.invalidate();
            self.status = TaskStatus::Cancelled;
        }
    }
    pub fn reset(&mut self) {
        self.invalidate();
        self.status = TaskStatus::Idle;
    }
    /// Drain a completion after a wake. Returns whether the service snapshot changed.
    pub fn poll(&mut self) -> bool {
        let result = self
            .mailbox
            .as_ref()
            .and_then(|slot| slot.lock().expect("task mailbox poisoned").result.take());
        if let Some(result) = result {
            self.invalidate();
            self.status = match result {
                Completion::Ready(value) => TaskStatus::Ready(value),
                Completion::Cancelled => TaskStatus::Cancelled,
            };
            true
        } else {
            false
        }
    }
}
impl<T, E> Drop for Task<T, E> {
    fn drop(&mut self) {
        self.invalidate();
    }
}

impl<T, E> Default for Task<T, E> {
    fn default() -> Self {
        Self::new(Arc::new(|| {}))
    }
}

/// Value-only UI commands; Futures and executor handles stay in the host.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum TaskCommand<I> {
    Start(I),
    Cancel,
    Reset,
}

/// One host-executed job, submitted to an application-provided executor.
pub type TaskFuture = std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
type Operation<I, T, E> =
    Box<dyn FnMut(I) -> std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send>>>;

/// A ready-to-use service for one replaceable task. Compose these in a larger
/// HostService for multiple independent operations. Only values cross the ABI.
pub struct TaskService<I, T, E> {
    task: Task<T, E>,
    operation: Operation<I, T, E>,
    spawn: Box<dyn FnMut(TaskFuture)>,
    revision: u64,
}
impl<I: 'static, T: Send + 'static, E: Send + 'static> TaskService<I, T, E> {
    pub fn new<F, Fut>(mut operation: F, spawn: impl FnMut(TaskFuture) + 'static) -> Self
    where
        F: FnMut(I) -> Fut + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        Self {
            task: Task::default(),
            operation: Box::new(move |input| Box::pin(operation(input))),
            spawn: Box::new(spawn),
            revision: 0,
        }
    }
    pub fn status(&self) -> &TaskStatus<T, E> {
        self.task.status()
    }
    pub fn send(&mut self, command: TaskCommand<I>) {
        match command {
            TaskCommand::Start(input) => {
                let future = (self.operation)(input);
                let job = self.task.start(future);
                (self.spawn)(Box::pin(job));
            }
            TaskCommand::Cancel => self.task.cancel(),
            TaskCommand::Reset => self.task.reset(),
        }
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn poll(&mut self) -> bool {
        let changed = self.task.poll();
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
    pub fn set_wake_callback(&mut self, wake: Wake) {
        self.task.set_wake_callback(wake);
    }
}
impl<I, T, E> crate::services::HostService for TaskService<I, T, E>
where
    I: serde::Serialize + serde::de::DeserializeOwned + 'static,
    T: Clone + Send + serde::Serialize + serde::de::DeserializeOwned + 'static,
    E: Clone + Send + serde::Serialize + serde::de::DeserializeOwned + 'static,
{
    type Snapshot = TaskStatus<T, E>;
    type Command = TaskCommand<I>;
    fn snapshot(&self) -> Self::Snapshot {
        self.task.status().clone()
    }
    fn snapshot_revision(&self) -> Option<u64> {
        Some(self.revision)
    }
    fn send(&mut self, command: Self::Command) {
        self.send(command);
    }
    fn poll(&mut self) -> bool {
        self.poll()
    }
    fn set_wake_callback(&mut self, wake: Wake) {
        self.set_wake_callback(wake);
    }
}

/// Declaration-local access to a task's snapshot and command outbox.
/// Creating this view does not start work or allocate a task.
pub struct TaskUi<'a, I, T, E> {
    pub(crate) status: &'a TaskStatus<T, E>,
    pub(crate) commands: &'a mut Vec<TaskCommand<I>>,
}
impl<I, T, E> TaskUi<'_, I, T, E> {
    pub fn status(&self) -> &TaskStatus<T, E> {
        self.status
    }
    pub fn pending(&self) -> bool {
        matches!(self.status, TaskStatus::Pending)
    }
    pub fn start(&mut self, input: I) {
        self.commands.push(TaskCommand::Start(input));
    }
    pub fn cancel(&mut self) {
        self.commands.push(TaskCommand::Cancel);
    }
    pub fn reset(&mut self) {
        self.commands.push(TaskCommand::Reset);
    }
}
