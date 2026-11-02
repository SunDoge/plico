//! Shared clock and wake driver for native CPU workload examples.
use plico_renderer::WakeCallback;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub struct Feed {
    pub value: Arc<AtomicUsize>,
    pub wake: Arc<Mutex<Option<WakeCallback>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Feed {
    pub fn new(hz: usize) -> Self {
        let value = Arc::new(AtomicUsize::new(0));
        let wake = Arc::new(Mutex::new(None::<WakeCallback>));
        let stopped = Arc::new(AtomicBool::new(false));
        let (v, w, stop) = (value.clone(), wake.clone(), stopped.clone());
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                thread::park_timeout(Duration::from_secs_f64(1. / hz as f64));
                if stop.load(Ordering::Acquire) {
                    break;
                }
                v.fetch_add(1, Ordering::AcqRel);
                let callback = w.lock().unwrap().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
        });
        Self {
            value,
            wake,
            stopped,
            worker: Some(worker),
        }
    }
}
impl Drop for Feed {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker.join().unwrap();
        }
    }
}
// Two clock reads around the entire measured batch, never per node/property.
pub fn thread_cpu_ns() -> u128 {
    #[cfg(target_os = "linux")]
    {
        let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
        // SAFETY: clock_gettime writes a timespec to a correctly sized output.
        assert_eq!(
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, time.as_mut_ptr()) },
            0
        );
        // SAFETY: successful clock_gettime initialized the output.
        let time = unsafe { time.assume_init() };
        time.tv_sec as u128 * 1_000_000_000 + time.tv_nsec as u128
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}
