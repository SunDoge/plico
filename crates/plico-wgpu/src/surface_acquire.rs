//! Single-slot surface acquisition, inspired by MyGO's Metal presenter.
//! Only the fetch closure runs on the worker; configuration stays on the host.
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

type Wake = Arc<dyn Fn() + Send + Sync>;
#[derive(Debug, snafu::Snafu)]
#[snafu(display("surface acquisition worker panicked"))]
pub(crate) struct AcquireError;
struct State<T> {
    fetching: bool,
    ready: Option<T>,
    failed: bool,
    waiter: bool,
    stopping: bool,
    signal: bool,
}
struct Shared<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
}
pub(crate) struct Acquire<T: Send + 'static> {
    shared: Arc<Shared<T>>,
    thread: Option<JoinHandle<()>>,
}
impl<T: Send + 'static> Acquire<T> {
    pub fn new(mut fetch: impl FnMut() -> T + Send + 'static, wake: Wake) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                fetching: false,
                ready: None,
                failed: false,
                waiter: false,
                stopping: false,
                signal: false,
            }),
            changed: Condvar::new(),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("plico-surface-acquire".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                {
                    // MyGO's Metal presenter uses the UI's QoS for nextDrawable:
                    // display-paced work should not run at background-worker priority.
                    // This dedicated thread owns no unrelated application jobs.
                    let status = unsafe {
                        libc::pthread_set_qos_class_self_np(
                            libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
                            0,
                        )
                    };
                    if status != 0 {
                        tracing::debug!(status, "could not set surface acquisition thread QoS");
                    }
                }
                loop {
                    let mut state = worker.state.lock().unwrap();
                    while !state.fetching && !state.stopping {
                        state = worker.changed.wait(state).unwrap();
                    }
                    if state.stopping {
                        break;
                    }
                    drop(state);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut fetch));
                    let mut state = worker.state.lock().unwrap();
                    state.fetching = false;
                    match result {
                        Ok(value) => state.ready = Some(value),
                        Err(_) => state.failed = true,
                    }
                    let notify = state.waiter;
                    state.waiter = false;
                    state.signal |= notify;
                    worker.changed.notify_all();
                    drop(state);
                    // Must be a nonblocking event-loop proxy, never a call waiting on
                    // the host thread. drain()/Drop may be waiting there for this fetch.
                    if notify
                        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wake()))
                            .is_err()
                    {
                        // Do not strand a later request on an unwound worker.
                        let mut state = worker.state.lock().unwrap();
                        state.failed = true;
                        state.fetching = false;
                        worker.changed.notify_all();
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    /// Prefetch at most one result. Idle prefetch completion does not wake UI.
    pub fn start(&self) {
        let mut state = self.shared.state.lock().unwrap();
        if !state.fetching && state.ready.is_none() && !state.failed {
            state.fetching = true;
            self.shared.changed.notify_one();
        }
    }
    /// Whether a request would wait for a result, without starting work or arming a wake.
    #[cfg(test)]
    pub fn needs_result(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.ready.is_none() && !state.failed
    }
    /// Peek without consuming the texture. If not ready, request one fetch and
    /// arm its wake. Frame validation can precede this check, damage planning follow it.
    pub fn ready(&self) -> Result<bool, AcquireError> {
        let mut state = self.shared.state.lock().unwrap();
        if state.failed {
            return Err(AcquireError);
        }
        if state.ready.is_some() {
            return Ok(true);
        }
        state.waiter = true;
        if !state.fetching {
            state.fetching = true;
            self.shared.changed.notify_one();
        }
        Ok(false)
    }
    /// Take a result, or arrange one wake when an outstanding/new fetch finishes.
    pub fn take(&self) -> Result<Option<T>, AcquireError> {
        let mut state = self.shared.state.lock().unwrap();
        state.signal = false;
        if state.failed {
            return Err(AcquireError);
        }
        if let Some(value) = state.ready.take() {
            return Ok(Some(value));
        }
        state.waiter = true;
        if !state.fetching {
            state.fetching = true;
            self.shared.changed.notify_one();
        }
        Ok(None)
    }
    /// The latest request needs no drawable. Preserve idle prefetch, but cancel
    /// its readiness wake rather than letting an obsolete changed frame redraw.
    pub fn cancel_waiter(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.waiter = false;
        state.signal = false;
    }
    pub fn take_signal(&self) -> bool {
        let mut state = self.shared.state.lock().unwrap();
        std::mem::take(&mut state.signal)
    }
    /// Wait for any in-flight acquisition and release its texture before surface
    /// configuration/trim. Cancel a queued wake; do not start another fetch.
    pub fn drain(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.waiter = false;
        state.signal = false;
        while state.fetching {
            state = self.shared.changed.wait(state).unwrap();
        }
        let value = state.ready.take();
        drop(state);
        drop(value);
    }
}
impl<T: Send + 'static> Drop for Acquire<T> {
    fn drop(&mut self) {
        self.drain();
        {
            let mut state = self.shared.state.lock().unwrap();
            state.stopping = true;
            self.shared.changed.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;
    #[cfg(target_os = "macos")]
    #[test]
    fn drawable_fetch_runs_at_user_interactive_qos() {
        let (send, receive) = mpsc::channel();
        let worker = Acquire::new(
            move || {
                let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
                let mut priority = 0;
                // Query inside the actual fetch callback, not on the test thread.
                let status = unsafe {
                    libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut priority)
                };
                send.send((status, class as u32, priority)).unwrap();
            },
            Arc::new(|| {}),
        )
        .unwrap();
        worker.start();
        let (status, class, priority) = receive.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.drain();
        assert_eq!(status, 0);
        assert_eq!(class, libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE as u32);
        assert_eq!(priority, 0);
    }
    #[test]
    fn pending_fetch_is_single_slot_and_wakes_only_a_waiting_frame() {
        let (permit, receive) = mpsc::channel();
        let (started, starts) = mpsc::channel();
        let (wake, wakes) = mpsc::channel();
        let worker = Acquire::new(
            move || {
                started.send(()).unwrap();
                receive.recv().unwrap()
            },
            Arc::new(move || {
                wake.send(()).unwrap();
            }),
        )
        .unwrap();
        worker.start();
        starts.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.start();
        assert!(!worker.ready().unwrap());
        assert!(!worker.ready().unwrap());
        assert_eq!(worker.take().unwrap(), None);
        permit.send(7).unwrap();
        wakes.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.take_signal());
        assert!(!worker.take_signal());
        assert!(worker.ready().unwrap());
        assert!(
            worker.ready().unwrap(),
            "readiness must not consume a drawable"
        );
        assert_eq!(worker.take().unwrap(), Some(7));
        worker.start();
        starts.recv_timeout(Duration::from_secs(2)).unwrap();
        permit.send(9).unwrap();
        worker.drain();
        assert!(
            wakes.try_recv().is_err(),
            "idle prefetch must not animate a static window"
        );
        assert!(
            starts.try_recv().is_err(),
            "no second in-flight acquisition"
        );
    }
    #[test]
    fn reverting_a_waiting_frame_cancels_its_wake_without_dropping_prefetch() {
        let (permit, receive) = mpsc::channel();
        let (wake, wakes) = mpsc::channel();
        let worker = Acquire::new(
            move || receive.recv().unwrap(),
            Arc::new(move || {
                wake.send(()).unwrap();
            }),
        )
        .unwrap();
        assert!(!worker.ready().unwrap());
        assert!(worker.needs_result());
        worker.cancel_waiter();
        permit.send(7).unwrap();
        // Wait for completion without registering a new waiter.
        let mut state = worker.shared.state.lock().unwrap();
        while state.fetching {
            state = worker.shared.changed.wait(state).unwrap();
        }
        drop(state);
        assert!(!worker.take_signal());
        assert!(!worker.needs_result());
        assert_eq!(worker.take().unwrap(), Some(7));
        drop(worker); // Join before asserting that no late wake ran.
        assert!(wakes.try_recv().is_err());
    }
    #[test]
    fn drain_releases_acquired_owners_and_cancels_stale_wakes() {
        struct Owner(Arc<AtomicUsize>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let captured = drops.clone();
        let worker = Acquire::new(move || Owner(captured.clone()), Arc::new(|| {})).unwrap();
        worker.start();
        worker.drain();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(!worker.take_signal());
        worker.start();
        drop(worker);
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn failed_fetch_unblocks_drain_and_reports_failure_instead_of_hanging() {
        let (wake, wakes) = mpsc::channel();
        let worker = Acquire::<()>::new(
            || panic!("controlled fetch failure"),
            Arc::new(move || {
                wake.send(()).unwrap();
            }),
        )
        .unwrap();
        assert!(worker.take().unwrap().is_none());
        wakes.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.take().is_err());
        worker.drain();
    }
}
