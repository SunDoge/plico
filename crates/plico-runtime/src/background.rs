//! Native blocking work with retained worker-local resources and streamed output.
//! Futures use `tasks::Task`/`TaskService` on the caller's executor instead.
use plico_renderer::WakeCallback;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
struct Shared {
    generation: AtomicUsize,
    alive: AtomicBool,
    pending: AtomicBool,
    wake: Mutex<WakeCallback>,
}
struct Input<I> {
    generation: usize,
    independent: bool,
    value: I,
}
struct Output<O> {
    generation: Option<usize>,
    value: O,
}
/// One persistent worker thread. State is constructed on that thread and need
/// not implement Send. Replacing a job discards stale output, not completed IO.
pub struct Worker<I, O> {
    tx: mpsc::Sender<Input<I>>,
    rx: mpsc::Receiver<Output<O>>,
    shared: Arc<Shared>,
}
/// Per-job cancellation check and output sink. No UI references cross threads.
pub struct Context<O> {
    generation: usize,
    independent: bool,
    tx: mpsc::Sender<Output<O>>,
    shared: Arc<Shared>,
}
impl<O> Context<O> {
    pub fn generation(&self) -> usize {
        self.generation
    }
    pub fn is_current(&self) -> bool {
        self.shared.alive.load(Ordering::Acquire) && (self.independent || self.is_latest())
    }
    pub fn is_latest(&self) -> bool {
        self.shared.alive.load(Ordering::Acquire)
            && self.shared.generation.load(Ordering::Acquire) == self.generation
    }
    pub fn emit(&self, value: O) {
        if self.independent {
            self.emit_always(value)
        } else {
            self.emit_latest(value)
        }
    }
    pub fn emit_latest(&self, value: O) {
        if self.is_latest() {
            self.send(Some(self.generation), value)
        }
    }
    /// Use only for effects that still happened after cancellation (for example
    /// billable usage). This never makes stale UI data current.
    pub fn emit_always(&self, value: O) {
        if self.shared.alive.load(Ordering::Acquire) {
            self.send(None, value)
        }
    }
    fn send(&self, generation: Option<usize>, value: O) {
        if self.tx.send(Output { generation, value }).is_ok()
            && !self.shared.pending.swap(true, Ordering::AcqRel)
        {
            let wake = self
                .shared
                .wake
                .lock()
                .expect("worker wake poisoned")
                .clone();
            wake();
        }
    }
}
impl<I: Send + 'static, O: Send + 'static> Worker<I, O> {
    pub fn new<S: 'static>(
        init: impl FnOnce() -> S + Send + 'static,
        mut handle: impl FnMut(&mut S, I, &Context<O>) + Send + 'static,
    ) -> Self {
        let (tx, inputs) = mpsc::channel::<Input<I>>();
        let (outputs, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            generation: AtomicUsize::new(0),
            alive: AtomicBool::new(true),
            pending: AtomicBool::new(false),
            wake: Mutex::new(Arc::new(|| {})),
        });
        let worker_shared = shared.clone();
        std::thread::spawn(move || {
            let mut state = init();
            while let Ok(input) = inputs.recv() {
                let context = Context {
                    generation: input.generation,
                    independent: input.independent,
                    tx: outputs.clone(),
                    shared: worker_shared.clone(),
                };
                if context.is_current() {
                    handle(&mut state, input.value, &context);
                }
            }
        });
        Self { tx, rx, shared }
    }
    pub fn send_latest(&self, value: I) -> Result<(), I> {
        let generation = self
            .shared
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.tx
            .send(Input {
                generation,
                independent: false,
                value,
            })
            .map_err(|error| error.0.value)
    }
    /// Independent effects are not cancelled by later replaceable UI jobs.
    /// Use emit_latest for any completion that must still belong to the current job.
    pub fn send_background(&self, value: I) -> Result<(), I> {
        self.tx
            .send(Input {
                generation: self.generation(),
                independent: true,
                value,
            })
            .map_err(|error| error.0.value)
    }
}
impl<I, O> Worker<I, O> {
    pub fn generation(&self) -> usize {
        self.shared.generation.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }
    pub fn set_wake_callback(&self, wake: WakeCallback) {
        *self.shared.wake.lock().expect("worker wake poisoned") = wake.clone();
        if self.pending() {
            wake();
        }
    }
    pub fn drain(&self) -> impl Iterator<Item = O> + '_ {
        // Rearm before draining: output arriving during the drain must wake again.
        self.shared.pending.store(false, Ordering::Release);
        self.rx.try_iter().filter_map(|output| {
            (output.generation.is_none_or(|g| g == self.generation())).then_some(output.value)
        })
    }
}
impl<I, O> Drop for Worker<I, O> {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::Release);
        self.cancel();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn cancellation_discards_ui_output_but_preserves_completed_effects() {
        let (started, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let worker = Worker::new(
            || (),
            move |_, (), cx| {
                started.send(()).unwrap();
                wait.recv().unwrap();
                cx.emit("stale");
                cx.emit_always("billed");
            },
        );
        worker.send_latest(()).unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.cancel();
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut output = vec![];
        while std::time::Instant::now() < deadline {
            output.extend(worker.drain());
            if !output.is_empty() {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(output, vec!["billed"]);
    }
    #[test]
    fn non_send_state_lives_on_worker_and_queued_stale_jobs_are_skipped() {
        use std::rc::Rc;
        let worker = Worker::new(
            || Rc::new(4u32),
            |state, input, cx| cx.emit(**state + input),
        );
        worker.send_latest(3u32).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut result = vec![];
        while std::time::Instant::now() < deadline {
            result.extend(worker.drain());
            if !result.is_empty() {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(result, vec![7]);
    }
}
