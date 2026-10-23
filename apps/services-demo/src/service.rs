//! Executable-owned worker. No code from a reloadable module runs on this thread.
use plico_dev::services::{HostService, Wake};
use plico_services_demo::{TaskCommand, TaskSnapshot, TaskStatus};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
struct Update {
    id: u64,
    progress: u8,
    status: TaskStatus,
}
enum Ticks {
    Realtime,
    Controlled(Receiver<()>),
}
pub struct TaskService {
    snapshot: TaskSnapshot,
    revision: u64,
    jobs: Option<SyncSender<u64>>,
    updates: Receiver<Update>,
    cancel: Arc<AtomicBool>,
    wake: Arc<Mutex<Option<Wake>>>,
    worker: Option<JoinHandle<()>>,
}
impl TaskService {
    pub fn new() -> Self {
        Self::spawn(Ticks::Realtime)
    }
    #[allow(dead_code)]
    pub fn controlled() -> (Self, Sender<()>) {
        let (tx, rx) = mpsc::channel();
        (Self::spawn(Ticks::Controlled(rx)), tx)
    }
    fn spawn(ticks: Ticks) -> Self {
        let (job_tx, job_rx) = mpsc::sync_channel(1);
        let (updates_tx, updates) = mpsc::sync_channel(128);
        let cancel = Arc::new(AtomicBool::new(false));
        let wake: Arc<Mutex<Option<Wake>>> = Arc::default();
        let worker_cancel = cancel.clone();
        let worker_wake = wake.clone();
        let worker = thread::spawn(move || {
            while let Ok(id) = job_rx.recv() {
                for step in 1..=50 {
                    match &ticks {
                        Ticks::Realtime => thread::sleep(Duration::from_millis(200)),
                        Ticks::Controlled(rx) => loop {
                            if worker_cancel.load(Ordering::Acquire) {
                                break;
                            }
                            match rx.recv_timeout(Duration::from_millis(10)) {
                                Ok(()) => break,
                                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            }
                        },
                    }
                    let cancelled = worker_cancel.load(Ordering::Acquire);
                    let status = if cancelled {
                        TaskStatus::Cancelled
                    } else if step == 50 {
                        TaskStatus::Completed
                    } else {
                        TaskStatus::Running
                    };
                    if updates_tx
                        .send(Update {
                            id,
                            progress: if cancelled { (step - 1) * 2 } else { step * 2 },
                            status,
                        })
                        .is_err()
                    {
                        return;
                    }
                    let callback = worker_wake.lock().unwrap().clone();
                    if let Some(wake) = callback {
                        wake();
                    }
                    if cancelled {
                        break;
                    }
                }
            }
        });
        Self {
            snapshot: TaskSnapshot {
                instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
                ..Default::default()
            },
            revision: 0,
            jobs: Some(job_tx),
            updates,
            cancel,
            wake,
            worker: Some(worker),
        }
    }
}
impl HostService for TaskService {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    fn snapshot(&self) -> TaskSnapshot {
        self.snapshot.clone()
    }
    fn snapshot_revision(&self) -> Option<u64> {
        Some(self.revision)
    }
    fn send(&mut self, command: TaskCommand) {
        match command {
            TaskCommand::Start if self.snapshot.status != TaskStatus::Running => {
                self.revision = self.revision.wrapping_add(1);
                self.cancel.store(false, Ordering::Release);
                let id = self.snapshot.task_id + 1;
                if self.jobs.as_ref().is_some_and(|tx| tx.try_send(id).is_ok()) {
                    self.snapshot.task_id = id;
                    self.snapshot.started += 1;
                    self.snapshot.progress = 0;
                    self.snapshot.status = TaskStatus::Running;
                } else {
                    self.snapshot.status = TaskStatus::Failed;
                }
            }
            TaskCommand::Cancel if self.snapshot.status == TaskStatus::Running => {
                self.cancel.store(true, Ordering::Release)
            }
            _ => {}
        }
    }
    fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(update) = self.updates.try_recv() {
            if update.id != self.snapshot.task_id {
                continue;
            }
            changed = true;
            self.snapshot.progress = update.progress;
            if update.status == TaskStatus::Completed
                && self.snapshot.status != TaskStatus::Completed
            {
                self.snapshot.completed += 1;
            }
            self.snapshot.status = update.status;
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
    fn set_wake_callback(&mut self, wake: Wake) {
        *self.wake.lock().unwrap() = Some(wake);
    }
}
impl Drop for TaskService {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
