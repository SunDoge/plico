use plico_dev::tasks::{Task, TaskStatus};
use std::{
    future::{Future, pending, ready},
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};
#[test]
fn completion_replacement_cancellation_and_drop() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    let mut task = Task::<i32, ()>::new(Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    }));
    let mut cx = Context::from_waker(Waker::noop());
    let old = task.start(ready(Ok(1)));
    let new = task.start(ready(Ok(2)));
    assert_eq!(pin!(old).poll(&mut cx), Poll::Ready(()));
    assert!(!task.poll());
    assert_eq!(wakes.load(Ordering::Relaxed), 0);
    assert_eq!(pin!(new).poll(&mut cx), Poll::Ready(()));
    assert!(task.poll());
    assert_eq!(task.status(), &TaskStatus::Ready(Ok(2)));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    assert!(!task.poll());
    let abandoned = task.start(pending());
    drop(abandoned);
    assert!(task.poll());
    assert_eq!(task.status(), &TaskStatus::Cancelled);
    let cancelled = task.start(ready(Ok(3)));
    task.cancel();
    assert_eq!(pin!(cancelled).poll(&mut cx), Poll::Ready(()));
    assert!(!task.poll());
    assert_eq!(task.status(), &TaskStatus::Cancelled);
    let detached = task.start(ready(Ok(4)));
    drop(task);
    assert_eq!(pin!(detached).poll(&mut cx), Poll::Ready(()));
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
}
#[test]
fn completion_can_arrive_from_background_thread() {
    let mut task = Task::<i32, String>::new(Arc::new(|| {}));
    let future = task.start(ready(Err("network error".to_owned())));
    std::thread::spawn(move || {
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(pin!(future).poll(&mut cx), Poll::Ready(()));
    })
    .join()
    .unwrap();
    assert!(task.poll());
    assert_eq!(
        task.status(),
        &TaskStatus::Ready(Err("network error".to_owned()))
    );
}

#[test]
fn installing_host_wake_callback_updates_pending_task() {
    let mut task = Task::<i32, ()>::default();
    let future = task.start(ready(Ok(5)));
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    task.set_wake_callback(Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    }));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(pin!(future).poll(&mut cx), Poll::Ready(()));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    assert!(task.poll());
}
