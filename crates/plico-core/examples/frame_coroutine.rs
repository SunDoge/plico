//! Safe, isolated experiment: `cargo run -p plico-core --example frame_coroutine`.
//! One coroutine owns one Engine. This is not a production scheduler or scope API.
use plico_core::{Engine, UiContext, prelude::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

#[derive(Default)]
struct WakeFlag(AtomicBool);
impl Wake for WakeFlag {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Default)]
struct Session {
    engine: Engine,
    pass: u64,
    drawn_pass: Option<u64>,
    declarations: usize,
    // An owned, serializable checkpoint, not a pointer into the Future.
    saved: HashMap<String, Value>,
}

#[derive(Clone)]
struct AsyncUi {
    session: Rc<RefCell<Session>>,
    wake: Arc<WakeFlag>,
}
impl AsyncUi {
    fn state<T: DeserializeOwned>(&self, key: &str, init: impl FnOnce() -> T) -> T {
        self.session
            .borrow()
            .saved
            .get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_else(init)
    }

    fn checkpoint(&self, key: &str, value: &impl Serialize) {
        let value = serde_json::to_value(value).expect("experiment state is serializable");
        let mut session = self.session.borrow_mut();
        if let Some(slot) = session.saved.get_mut(key) {
            *slot = value;
        } else {
            session.saved.insert(key.to_owned(), value);
        }
    }

    fn invalidate(&self) {
        self.wake.wake_by_ref();
    }

    // Pending means this pass has already declared. Only the host can advance
    // the pass; polling twice in one pass must not invoke the closure twice.
    fn draw<R>(&self, draw: impl FnOnce(&mut UiContext<'_>) -> R) -> Poll<R> {
        let mut session = self.session.borrow_mut();
        if session.drawn_pass == Some(session.pass) {
            return Poll::Pending;
        }
        session.drawn_pass = Some(session.pass);
        let mut output = None;
        session.engine.frame(|cx| output = Some(draw(cx))).unwrap();
        session.declarations += 1;
        Poll::Ready(output.unwrap())
    }

    async fn frame<R>(&self, draw: impl FnOnce(&mut UiContext<'_>) -> R) -> R {
        let mut draw = Some(draw);
        poll_fn(|_| {
            // Keep the borrowed closure in this Future, never in a global queue.
            if self.session.borrow().drawn_pass == Some(self.session.borrow().pass) {
                return Poll::Pending;
            }
            self.draw(draw.take().unwrap())
        })
        .await
    }

    async fn draw_until<F: Future>(
        &self,
        task: F,
        mut loading: impl FnMut(&mut UiContext<'_>),
    ) -> F::Output {
        let mut task = std::pin::pin!(task);
        poll_fn(|cx| {
            if let Poll::Ready(value) = task.as_mut().poll(cx) {
                return Poll::Ready(value);
            }
            let _ = self.draw(&mut loading);
            Poll::Pending
        })
        .await
    }
}

struct Host {
    ui: AsyncUi,
    task: Option<Pin<Box<dyn Future<Output = ()>>>>,
}
impl Host {
    fn new(make: impl FnOnce(AsyncUi) -> Pin<Box<dyn Future<Output = ()>>>) -> Self {
        let ui = AsyncUi {
            session: Rc::default(),
            wake: Arc::default(),
        };
        let task = Some(make(ui.clone()));
        ui.invalidate();
        Self { ui, task }
    }

    fn poll_current_pass(&mut self) {
        let waker = Waker::from(self.ui.wake.clone());
        if let Some(task) = &mut self.task
            && task
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_ready()
        {
            self.cancel();
        }
    }

    // Models an event-loop callback, not continuous vsync polling.
    fn pump(&mut self) -> usize {
        let mut passes = 0;
        while self.ui.wake.0.swap(false, Ordering::AcqRel) {
            assert!(passes < 8, "view did not settle");
            self.ui.session.borrow_mut().pass += 1;
            self.poll_current_pass();
            passes += 1;
        }
        passes
    }

    fn click(&mut self, key: &'static str) {
        let mut session = self.ui.session.borrow_mut();
        fn find(
            engine: &Engine,
            parent: plico_core::Entity,
            key: &'static str,
        ) -> Option<plico_core::Entity> {
            engine.find(parent, key).or_else(|| {
                engine
                    .node(parent)
                    .unwrap()
                    .children
                    .iter()
                    .find_map(|&child| find(engine, child, key))
            })
        }
        let id = find(&session.engine, session.engine.root(), key).expect("button mounted");
        session.engine.queue_click(id);
        drop(session);
        self.ui.invalidate();
        self.pump();
    }

    fn reload(&mut self, make: impl FnOnce(AsyncUi) -> Pin<Box<dyn Future<Output = ()>>>) {
        // Drop old code/state before constructing the replacement. Only explicit
        // checkpoints survive; the instruction pointer always starts over.
        self.task = None;
        self.task = Some(make(self.ui.clone()));
        self.ui.invalidate();
        self.pump();
    }

    fn cancel(&mut self) {
        self.task = None;
        self.ui.session.borrow_mut().engine.frame(|_| {}).unwrap();
        self.ui.wake.0.store(false, Ordering::Release);
    }

    fn has_text(&self, text: &str) -> bool {
        fn contains(engine: &Engine, id: plico_core::Entity, text: &str) -> bool {
            let node = engine.node(id).unwrap();
            node.text == text || node.children.iter().any(|&id| contains(engine, id, text))
        }
        let session = self.ui.session.borrow();
        contains(&session.engine, session.engine.root(), text)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn counter(ui: AsyncUi, dropped: Arc<AtomicBool>) {
    struct OnDrop(Arc<AtomicBool>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let _guard = OnDrop(dropped);
    let mut count: i32 = ui.state("count", || 0);
    let mut transient = 0;
    loop {
        let incremented = ui
            .frame(|cx| {
                let mut clicked = false;
                let _ = column(cx).gap(12.).padding(24.)
                    + |cx| {
                        text!(cx, "Count: {count}");
                        text!(cx, "Transient: {transient}");
                        clicked = button(cx).text("Increment").key("increment").clicked();
                    };
                clicked
            })
            .await;
        if incremented {
            count += 1;
            transient += 1;
            ui.invalidate(); // Explicit invalidation: ordinary locals aren't tracked.
        }
        ui.checkpoint("count", &count);
    }
}

#[derive(Clone, Default)]
struct Completion(Arc<Mutex<(Option<i32>, Option<Waker>)>>);
impl Completion {
    fn complete(&self, value: i32) {
        let mut inner = self.0.lock().unwrap();
        inner.0 = Some(value);
        let waker = inner.1.take();
        drop(inner);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
impl Future for Completion {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let mut inner = self.0.lock().unwrap();
        match inner.0.take() {
            Some(value) => Poll::Ready(value),
            None => {
                inner.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

async fn workflow(ui: AsyncUi, completion: Completion) {
    loop {
        let confirmed = ui
            .frame(|cx| {
                text(cx, "Confirm deletion?");
                button(cx).text("Confirm").key("confirm").clicked()
            })
            .await;
        if confirmed {
            break;
        }
    }
    ui.invalidate(); // The confirmation pass already drew; schedule loading.
    let result = ui
        .draw_until(completion, |cx| {
            text(cx, "Deleting...");
        })
        .await;
    loop {
        ui.frame(|cx| {
            text!(cx, "Deleted: {result}");
        })
        .await;
    }
}

fn main() {
    let dropped = Arc::new(AtomicBool::new(false));
    let mut host = Host::new(|ui| Box::pin(counter(ui, dropped.clone())));
    assert_eq!(host.pump(), 1);
    assert!(host.has_text("Count: 0"));
    let declarations = host.ui.session.borrow().declarations;
    host.poll_current_pass();
    host.poll_current_pass();
    assert_eq!(host.ui.session.borrow().declarations, declarations);
    assert_eq!(host.pump(), 0); // No input, animation or completion: idle.
    host.click("increment");
    host.click("increment");
    assert!(host.has_text("Count: 2"));
    assert!(host.has_text("Transient: 2"));
    let encoded = serde_json::to_vec(&host.ui.session.borrow().saved).unwrap();
    host.ui.session.borrow_mut().saved = serde_json::from_slice(&encoded).unwrap();
    host.reload(|ui| Box::pin(counter(ui, Arc::default())));
    assert!(dropped.load(Ordering::Acquire));
    assert!(host.has_text("Count: 2"));
    assert!(host.has_text("Transient: 0"));
    host.ui
        .session
        .borrow_mut()
        .saved
        .insert("count".into(), Value::String("incompatible".into()));
    host.reload(|ui| Box::pin(counter(ui, Arc::default())));
    assert!(host.has_text("Count: 0"));
    host.cancel();
    assert!(
        host.ui
            .session
            .borrow()
            .engine
            .node(host.ui.session.borrow().engine.root())
            .unwrap()
            .children
            .is_empty()
    );
    println!(
        "counter: local mutation, idle, same-pass repoll, checkpoint restore/reset, cancellation OK"
    );

    let completion = Completion::default();
    let mut flow = Host::new(|ui| Box::pin(workflow(ui, completion.clone())));
    flow.pump();
    flow.click("confirm");
    assert!(flow.has_text("Deleting..."));
    assert!(!flow.has_text("Confirm deletion?")); // Deliberately replaces whole view.
    assert_eq!(flow.pump(), 0); // Waiting does not require vsync polling.
    completion.complete(42); // Real Waker path, not a poll-count fake timer.
    assert_eq!(flow.pump(), 1);
    assert!(flow.has_text("Deleted: 42"));
    flow.cancel();
    println!("workflow: confirmation → sleeping task → wake → result, cleanup OK");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_state_frames_reload_and_external_wake() {
        main();
    }

    #[test]
    fn cancellation_while_waiting_does_not_remount_on_late_completion() {
        let completion = Completion::default();
        let mut host = Host::new(|ui| Box::pin(workflow(ui, completion.clone())));
        host.pump();
        host.click("confirm");
        assert!(host.has_text("Deleting..."));
        host.cancel();
        let declarations = host.ui.session.borrow().declarations;
        completion.complete(7);
        host.pump();
        assert_eq!(host.ui.session.borrow().declarations, declarations);
        assert!(!host.has_text("Deleted: 7"));
    }

    #[test]
    fn completed_future_unmounts_its_view() {
        let mut host = Host::new(|ui| {
            Box::pin(async move {
                ui.frame(|cx| {
                    text(cx, "Temporary");
                })
                .await;
            })
        });
        host.pump();
        assert!(host.task.is_none());
        assert!(!host.has_text("Temporary"));
        assert_eq!(host.pump(), 0);
    }
}
