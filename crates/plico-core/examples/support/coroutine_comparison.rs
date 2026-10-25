//! Authoring comparison using identical widgets and deterministic service results.
//! These APIs remain experiment-local; no layout builder accepts async children.
use super::*;

#[derive(Default)]
struct CounterView {
    count: i32,
}
impl CounterView {
    fn show(&mut self, cx: &mut UiContext<'_>, title: &str) {
        text(cx, title);
        text!(cx, "Count: {}", self.count);
        if button(cx).text("Increment").key("increment").clicked() {
            self.count += 1;
            cx.request_rebuild();
        }
    }
}
async fn local_counter(ui: AsyncUi, title: Props<String>) {
    let mut count = 0;
    loop {
        ui.frame(|cx| {
            text(cx, title.get());
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").key("increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        })
        .await;
    }
}
fn settle(engine: &mut Engine, mut show: impl FnMut(&mut UiContext<'_>)) {
    for _ in 0..8 {
        engine.frame(&mut show).unwrap();
        if !engine.changes().contains(plico_core::Changes::VIEW) {
            return;
        }
    }
    panic!("did not settle");
}
fn click(engine: &mut Engine, key: &'static str) {
    let id = engine
        .find(engine.root(), key)
        .expect("comparison button mounted");
    engine.queue_click(id);
}
fn snapshot(engine: &Engine) -> Value {
    serde_json::to_value(engine.snapshot()).unwrap()
}
fn counter_comparison() {
    let mut ordinary = CounterView::default();
    let mut engine = Engine::default();
    let slot = Rc::new(RefCell::new(None));
    let output = slot.clone();
    let mut coroutine = Host::new(move |ui| {
        let title = Props::new(&ui, "Counter".to_owned());
        *output.borrow_mut() = Some(title.clone());
        Box::pin(local_counter(ui, title))
    });
    let mut title = "Counter";
    settle(&mut engine, |cx| ordinary.show(cx, title));
    coroutine.pump();
    for _ in 0..3 {
        click(&mut engine, "increment");
        settle(&mut engine, |cx| ordinary.show(cx, title));
        coroutine.click("increment");
        assert_eq!(
            snapshot(&engine),
            snapshot(&coroutine.ui.session.borrow().engine)
        );
    }
    title = "Updated by parent";
    settle(&mut engine, |cx| ordinary.show(cx, title));
    slot.borrow().as_ref().unwrap().set(title.to_owned());
    coroutine.pump();
    assert_eq!(
        snapshot(&engine),
        snapshot(&coroutine.ui.session.borrow().engine)
    );
    assert_eq!(ordinary.count, 3);
    assert!(coroutine.has_text("Count: 3"));
    println!(
        "authoring/counter: same inline handler; coroutine local state, immediate simpler parent inputs"
    );
}

#[derive(Clone, Default)]
struct Service {
    requests: Rc<RefCell<Vec<Completion>>>,
}
impl Service {
    fn request(&self) -> Completion {
        let task = Completion::default();
        self.requests.borrow_mut().push(task.clone());
        task
    }
    fn finish(&self, index: usize, result: i32) {
        self.requests.borrow()[index].complete(result);
    }
    fn started(&self) -> usize {
        self.requests.borrow().len()
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Confirm,
    Pending,
    Error,
    Done,
}
#[derive(Default)]
enum Command {
    #[default]
    None,
    Start,
    Cancel,
}
#[derive(Default)]
struct DeleteView {
    phase: Phase,
}
impl DeleteView {
    fn show(&mut self, cx: &mut UiContext<'_>) -> Command {
        let before = self.phase;
        let mut command = Command::None;
        match self.phase {
            Phase::Idle => {
                text(cx, "Project");
                if button(cx).text("Delete").key("delete").clicked() {
                    self.phase = Phase::Confirm;
                }
            }
            Phase::Confirm => {
                text(cx, "Delete project?");
                if button(cx).text("Confirm").key("confirm").clicked() {
                    self.phase = Phase::Pending;
                    command = Command::Start;
                }
                if button(cx).text("Cancel").key("cancel").clicked() {
                    self.phase = Phase::Idle;
                }
            }
            Phase::Pending => {
                text(cx, "Deleting...");
                if button(cx).text("Cancel").key("cancel").clicked() {
                    self.phase = Phase::Idle;
                    command = Command::Cancel;
                }
            }
            Phase::Error => {
                text(cx, "Request failed");
                if button(cx).text("Retry").key("retry").clicked() {
                    self.phase = Phase::Pending;
                    command = Command::Start;
                }
                if button(cx).text("Back").key("back").clicked() {
                    self.phase = Phase::Idle;
                }
            }
            Phase::Done => {
                text(cx, "Deleted");
                if button(cx).text("Close").key("close").clicked() {
                    self.phase = Phase::Idle;
                }
            }
        }
        if self.phase != before {
            cx.request_rebuild();
        }
        command
    }
    fn completed(&mut self, result: i32) {
        self.phase = if result < 0 {
            Phase::Error
        } else {
            Phase::Done
        };
    }
}
struct OrdinaryHost {
    engine: Engine,
    view: DeleteView,
    service: Service,
    pending: Option<Completion>,
    wake: Arc<WakeFlag>,
}
impl OrdinaryHost {
    fn new(service: Service) -> Self {
        Self {
            engine: Engine::default(),
            view: DeleteView::default(),
            service,
            pending: None,
            wake: Arc::default(),
        }
    }
    fn drive(&mut self) {
        let waker = Waker::from(self.wake.clone());
        for _ in 0..8 {
            if let Some(task) = &mut self.pending
                && let Poll::Ready(result) = Pin::new(task).poll(&mut Context::from_waker(&waker))
            {
                self.pending = None;
                self.view.completed(result);
            }
            let mut command = Command::None;
            self.engine
                .frame(|cx| command = self.view.show(cx))
                .unwrap();
            // Service execution remains outside the synchronous declaration.
            match command {
                Command::Start => self.pending = Some(self.service.request()),
                Command::Cancel => self.pending = None,
                Command::None => {}
            }
            if !self.engine.changes().contains(plico_core::Changes::VIEW) {
                return;
            }
        }
        panic!("flow did not settle");
    }
    fn click(&mut self, key: &'static str) {
        click(&mut self.engine, key);
        self.drive();
    }
}

// Shared async helpers are shown explicitly: they are part of authoring cost.
async fn wait_button(ui: &AsyncUi, title: &str, label: &'static str, key: &'static str) {
    ui.invalidate();
    loop {
        if ui
            .frame(|cx| {
                text(cx, title);
                button(cx).text(label).key(key).clicked()
            })
            .await
        {
            return;
        }
    }
}
async fn choice(
    ui: &AsyncUi,
    title: &str,
    yes: &'static str,
    yes_key: &'static str,
    no: &'static str,
    no_key: &'static str,
) -> bool {
    ui.invalidate();
    loop {
        let selected = ui
            .frame(|cx| {
                text(cx, title);
                let mut selected = None;
                if button(cx).text(yes).key(yes_key).clicked() {
                    selected = Some(true);
                }
                if button(cx).text(no).key(no_key).clicked() {
                    selected = Some(false);
                }
                selected
            })
            .await;
        if let Some(selected) = selected {
            return selected;
        }
    }
}
async fn request_with_cancel(ui: &AsyncUi, task: Completion) -> Option<i32> {
    ui.invalidate();
    let mut task = std::pin::pin!(task);
    poll_fn(|cx| {
        if let Poll::Ready(result) = task.as_mut().poll(cx) {
            return Poll::Ready(Some(result));
        }
        match ui.draw(|cx| {
            text(cx, "Deleting...");
            button(cx).text("Cancel").key("cancel").clicked()
        }) {
            Poll::Ready(true) => Poll::Ready(None),
            _ => Poll::Pending,
        }
    })
    .await
}
async fn sequential_delete(ui: AsyncUi, service: Service) {
    loop {
        wait_button(&ui, "Project", "Delete", "delete").await;
        if !choice(
            &ui,
            "Delete project?",
            "Confirm",
            "confirm",
            "Cancel",
            "cancel",
        )
        .await
        {
            continue;
        }
        loop {
            let Some(result) = request_with_cancel(&ui, service.request()).await else {
                break;
            };
            if result >= 0 {
                wait_button(&ui, "Deleted", "Close", "close").await;
                break;
            }
            if !choice(&ui, "Request failed", "Retry", "retry", "Back", "back").await {
                break;
            }
        }
    }
}

// Candidate async-children contract: synchronously register an owned scope,
// then run its Future without retaining the parent's &mut UiContext.
// This is NOT LayoutBuilder::children and does not implement transparent layout.
fn async_children<F: Future<Output = ()> + 'static>(
    scopes: &mut Scopes,
    parent: Entity,
    site: u64,
    make: impl FnOnce(AsyncUi) -> F + 'static,
) -> Entity {
    scopes
        .reconcile(
            parent,
            vec![ChildSpec {
                identity: ChildIdentity {
                    site,
                    business_key: None,
                },
                make: Box::new(move |ui| Box::pin(make(ui))),
            }],
        )
        .unwrap()[0]
}
fn flow_comparison() {
    let ordinary_api = Service::default();
    let coroutine_api = Service::default();
    let mut ordinary = OrdinaryHost::new(ordinary_api.clone());
    let mut scopes = Scopes::default();
    let parent = scopes.mount(None, |ui| Box::pin(panel(ui, "Background", Rc::default())));
    let service = coroutine_api.clone();
    let child = async_children(&mut scopes, parent, 1, async move |ui| {
        sequential_delete(ui, service).await;
    });
    ordinary.drive();
    scopes.pump();
    let parent_declarations = scopes.declarations(parent);
    let compare = |ordinary: &OrdinaryHost, scopes: &Scopes| {
        assert_eq!(
            snapshot(&ordinary.engine),
            snapshot(
                &scopes
                    .nodes
                    .get(child)
                    .unwrap()
                    .host
                    .ui
                    .session
                    .borrow()
                    .engine
            )
        );
        assert_eq!(ordinary_api.started(), coroutine_api.started());
    };
    compare(&ordinary, &scopes);
    // Cancel confirmation, fail and retry, succeed and close, cancel in flight.
    for key in ["delete", "cancel", "delete", "confirm"] {
        ordinary.click(key);
        scopes.click(child, key);
        compare(&ordinary, &scopes);
    }
    ordinary_api.finish(0, -1);
    coroutine_api.finish(0, -1);
    ordinary.drive();
    scopes.pump();
    compare(&ordinary, &scopes);
    ordinary.click("retry");
    scopes.click(child, "retry");
    compare(&ordinary, &scopes);
    ordinary_api.finish(1, 42);
    coroutine_api.finish(1, 42);
    ordinary.drive();
    scopes.pump();
    compare(&ordinary, &scopes);
    for key in ["close", "delete", "confirm", "cancel"] {
        ordinary.click(key);
        scopes.click(child, key);
        compare(&ordinary, &scopes);
    }
    // Begin a newer request before the canceled request returns.
    for key in ["delete", "confirm"] {
        ordinary.click(key);
        scopes.click(child, key);
        compare(&ordinary, &scopes);
    }
    ordinary_api.finish(2, 42);
    coroutine_api.finish(2, 42);
    ordinary.drive();
    scopes.pump();
    compare(&ordinary, &scopes);
    assert!(scopes.has_text(child, "Deleting..."));
    ordinary_api.finish(3, -1);
    coroutine_api.finish(3, -1);
    ordinary.drive();
    scopes.pump();
    compare(&ordinary, &scopes);
    ordinary.click("back");
    scopes.click(child, "back");
    compare(&ordinary, &scopes);
    assert!(scopes.has_text(child, "Project"));
    assert_eq!(scopes.declarations(parent), parent_declarations);
    // Unmount while a fifth request is pending, then deliver its late result.
    scopes.click(child, "delete");
    scopes.click(child, "confirm");
    scopes.remove(parent);
    coroutine_api.finish(4, 42);
    assert_eq!(scopes.pump(), 0);
    assert!(scopes.nodes.is_empty());
    println!(
        "authoring/workflow: equivalent cancel/retry/success traces; async child retains parent declarations"
    );
}

pub(super) fn run() {
    counter_comparison();
    flow_comparison();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_events_and_parent_props_match() {
        counter_comparison();
    }
    #[test]
    fn async_child_and_explicit_state_machine_match_retry_cancel_and_completion() {
        flow_comparison();
    }
}
