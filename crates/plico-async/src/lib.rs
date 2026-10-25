//! Experimental async component scopes over one shared immediate Engine.
//!
//! No renderer, executor, dependency graph, unsafe context pointer or new DSL.
//! A synchronous parent mounts scopes; their local Futures declare only their
//! anchored subtree. Commit/layout still scan retained storage: this is not O(Δ).
#![forbid(unsafe_code)]

mod input;
mod queue;
mod ui;
pub use input::Input;
use plico_core::{
    Changes, Engine, Entity, FrameDelta, FrameTime, Layout, NodeChange, RetainedScope, Style,
    Theme, UiContext,
};
use queue::{ReadyQueue, ScopeWake};
use std::{
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context as TaskContext, Poll, Waker},
};
pub use ui::{AsyncUi, WaitOutcome};

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("UI declaration failed: {source}"))]
    Frame { source: plico_core::FrameError },
    #[snafu(display("async UI scope is no longer alive"))]
    Closed,
    #[snafu(display("async UI may only declare while its runtime polls it"))]
    OutsidePoll,
    #[snafu(display("async UI declaration reentered its shared Engine"))]
    Reentrant,
    #[snafu(display("scope belongs to another runtime"))]
    WrongRuntime,
    #[snafu(display("async UI exceeded its {budget} poll budget"))]
    PollBudget { budget: usize },
    #[snafu(display("cannot encode UI checkpoint: {source}"))]
    Encode { source: rmp_serde::encode::Error },
    #[snafu(display("cannot decode UI checkpoint snapshot: {source}"))]
    Decode { source: rmp_serde::decode::Error },
    #[snafu(display("restore checkpoints before polling component Futures"))]
    AlreadyStarted,
}
impl From<plico_core::FrameError> for Error {
    fn from(source: plico_core::FrameError) -> Self {
        Self::Frame { source }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// Runtime-local component instance. Widget identities remain Engine Entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId {
    owner: usize,
    anchor: RetainedScope,
}
impl ScopeId {
    pub fn entity(self) -> Entity {
        self.anchor.entity()
    }
}
type Task = Pin<Box<dyn Future<Output = Result<()>>>>;
type Checkpoints = HashMap<String, Vec<u8>>;
struct ScopeState {
    id: ScopeId,
    wake: Arc<ScopeWake>,
    pass: Cell<u64>,
    drawn: Cell<Option<u64>>,
    polling: Cell<bool>,
    started: Cell<bool>,
    animate: Cell<bool>,
    parent: Cell<Option<Entity>>,
    saved: RefCell<Checkpoints>,
}
struct Entry {
    state: Rc<ScopeState>,
    task: Option<Task>,
    context_style: Option<Style>,
}
impl Drop for Entry {
    fn drop(&mut self) {
        self.state.wake.close();
    }
}
struct Shared {
    owner: usize,
    engine: RefCell<Engine>,
    entries: RefCell<HashMap<Entity, Entry>>,
    ready: Arc<ReadyQueue>,
    external_layout: bool,
    parent_dirty: Cell<bool>,
    parent_animate: Cell<bool>,
    dirty: RefCell<HashMap<Entity, NodeChange>>,
    removed: RefCell<HashSet<Entity>>,
    changes: Cell<Changes>,
}
impl Shared {
    fn capture(&self) {
        let mut engine = self.engine.borrow_mut();
        let frame = engine.changes() & !Changes::VIEW;
        let delta = engine.take_frame_delta();
        drop(engine);
        self.changes.set(self.changes.get() | delta.frame | frame);
        let mut dirty = self.dirty.borrow_mut();
        for change in delta.changes {
            dirty
                .entry(change.entity)
                .and_modify(|old| {
                    old.changes |= change.changes;
                    old.properties |= change.properties;
                })
                .or_insert(change);
        }
        for entity in delta.removed {
            dirty.remove(&entity);
            self.removed.borrow_mut().insert(entity);
        }
    }
    fn reconcile(&self) {
        let engine = self.engine.borrow();
        let mut entries = self.entries.borrow_mut();
        // Capture parent relationships while the shared nodes still exist.
        for entry in entries.values_mut() {
            if engine.scope_mounted(entry.state.id.anchor) {
                entry.state.parent.set(
                    engine
                        .scope_parent(entry.state.id.anchor)
                        .map(RetainedScope::entity),
                );
                let style = engine.node(entry.state.id.entity()).unwrap().style;
                if entry.context_style.is_some_and(|old| old != style) {
                    entry.state.wake.schedule();
                }
                entry.context_style = Some(style);
            }
        }
        let mut removed: Vec<_> = entries
            .iter()
            .filter_map(|(&id, e)| (!engine.scope_mounted(e.state.id.anchor)).then_some(id))
            .collect();
        removed.sort_by_key(|id| std::cmp::Reverse(depth(*id, &entries)));
        for id in removed {
            entries.remove(&id);
        }
    }
}
fn depth(mut id: Entity, entries: &HashMap<Entity, Entry>) -> usize {
    let mut depth = 0;
    while let Some(parent) = entries.get(&id).and_then(|e| e.state.parent.get()) {
        depth += 1;
        id = parent;
    }
    depth
}

/// Synchronous mounting capability. It cannot escape its parent declaration.
pub struct Mounts<'a> {
    shared: &'a Rc<Shared>,
}
impl Mounts<'_> {
    /// Consume a layout as a retained anchor. Same parent/key reuses its Future;
    /// factories run only on first poll. Dynamic lists must provide business keys.
    pub fn mount<F: Future<Output = Result<()>> + 'static>(
        &mut self,
        layout: Layout<'_, '_>,
        make: impl FnOnce(AsyncUi) -> F + 'static,
    ) -> ScopeId {
        let anchor = layout.retain();
        let id = ScopeId {
            owner: self.shared.owner,
            anchor,
        };
        let mut entries = self.shared.entries.borrow_mut();
        entries.entry(anchor.entity()).or_insert_with(|| {
            let wake = ScopeWake::new(anchor.entity(), &self.shared.ready);
            let state = Rc::new(ScopeState {
                id,
                wake,
                pass: Cell::new(0),
                drawn: Cell::new(None),
                polling: Cell::new(false),
                started: Cell::new(false),
                animate: Cell::new(false),
                parent: Cell::new(None),
                saved: RefCell::default(),
            });
            let ui = AsyncUi {
                shared: Rc::downgrade(self.shared),
                state: state.clone(),
            };
            // Defers arbitrary factory code until no Engine/registry borrow exists.
            let task = Box::pin(async move { make(ui).await });
            state.wake.schedule();
            Entry {
                state,
                task: Some(task),
                context_style: None,
            }
        });
        id
    }
}

/// Optional single-threaded UI runtime. Background Futures may wake it from any
/// thread; they are polled on the caller's UI thread. IO execution stays in host.
pub struct Runtime {
    shared: Rc<Shared>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
impl Runtime {
    pub fn new() -> Self {
        Self::with_wake(|| {}, false)
    }
    /// `notify` should post a host event; it must not execute UI work itself.
    /// Use external_layout=true when a backend measures and lays out the tree.
    pub fn with_wake(notify: impl Fn() + Send + Sync + 'static, external_layout: bool) -> Self {
        static NEXT_OWNER: AtomicUsize = AtomicUsize::new(1);
        let owner = NEXT_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("runtime identity exhausted");
        Self {
            shared: Rc::new(Shared {
                owner,
                engine: RefCell::new(Engine::default()),
                entries: RefCell::default(),
                ready: Arc::new(ReadyQueue::new(notify)),
                external_layout,
                parent_dirty: Cell::new(true),
                parent_animate: Cell::new(false),
                dirty: RefCell::default(),
                removed: RefCell::default(),
                changes: Cell::new(Changes::empty()),
            }),
        }
    }
    pub fn engine(&self) -> Ref<'_, Engine> {
        self.shared.engine.borrow()
    }
    pub fn needs_parent_frame(&self) -> bool {
        self.shared.parent_dirty.get()
    }
    pub fn has_ready(&self) -> bool {
        self.shared.ready.has_ready()
    }
    pub fn needs_animation_frame(&self) -> bool {
        self.shared.parent_animate.get()
            || self
                .shared
                .entries
                .borrow()
                .values()
                .any(|entry| entry.task.is_some() && entry.state.animate.get())
    }
    /// Declare the synchronous parent; retained anchors preserve child state/tree.
    pub fn render(&mut self, show: impl FnOnce(&mut UiContext<'_>, &mut Mounts<'_>)) -> Result<()> {
        let before: HashSet<_> = self.shared.entries.borrow().keys().copied().collect();
        let mut mounts = Mounts {
            shared: &self.shared,
        };
        let mut engine = self.shared.engine.borrow_mut();
        let result = if self.shared.external_layout {
            engine.frame_declarations(|cx| show(cx, &mut mounts))
        } else {
            engine.frame(|cx| show(cx, &mut mounts))
        };
        self.shared
            .parent_dirty
            .set(engine.changes().contains(Changes::VIEW));
        self.shared
            .parent_animate
            .set(engine.changes().contains(Changes::ANIMATE));
        drop(engine);
        if let Err(error) = result {
            self.shared
                .entries
                .borrow_mut()
                .retain(|id, _| before.contains(id));
            return Err(error.into());
        }
        self.shared.capture();
        self.shared.reconcile();
        Ok(())
    }
    pub fn pump(&mut self) -> Result<usize> {
        self.pump_with_budget(10_000)
    }
    pub fn pump_with_budget(&mut self, budget: usize) -> Result<usize> {
        let mut polls = 0;
        while self.has_ready() {
            if polls == budget {
                return Err(Error::PollBudget { budget });
            }
            let wake = self.shared.ready.pop().unwrap();
            let mut entry = match self.shared.entries.borrow_mut().remove(&wake.entity) {
                Some(entry) => entry,
                None => continue,
            };
            if !Arc::ptr_eq(&wake, &entry.state.wake) {
                self.shared.entries.borrow_mut().insert(wake.entity, entry);
                continue;
            }
            if !self
                .shared
                .engine
                .borrow()
                .scope_mounted(entry.state.id.anchor)
            {
                drop(entry);
                self.shared.reconcile();
                continue;
            }
            let Some(task) = &mut entry.task else {
                self.shared.entries.borrow_mut().insert(wake.entity, entry);
                continue;
            };
            entry.state.pass.set(
                entry
                    .state
                    .pass
                    .get()
                    .checked_add(1)
                    .expect("scope pass exhausted"),
            );
            entry.state.started.set(true);
            entry.state.polling.set(true);
            struct PollGuard(Rc<ScopeState>);
            impl Drop for PollGuard {
                fn drop(&mut self) {
                    self.0.polling.set(false);
                }
            }
            let guard = PollGuard(entry.state.clone());
            let waker = Waker::from(wake.clone());
            let outcome = task.as_mut().poll(&mut TaskContext::from_waker(&waker));
            drop(guard);
            polls += 1;
            let mut failure = None;
            if let Poll::Ready(result) = outcome {
                entry.state.wake.close();
                entry.task = None;
                self.clear(entry.state.id)?;
                failure = result.err();
            }
            self.shared.entries.borrow_mut().insert(wake.entity, entry);
            self.shared.capture();
            self.shared.reconcile();
            if let Some(error) = failure {
                return Err(error);
            }
        }
        Ok(polls)
    }
    fn clear(&self, id: ScopeId) -> Result<()> {
        let mut engine = self.shared.engine.borrow_mut();
        if !engine.scope_mounted(id.anchor) {
            return Ok(());
        }
        if self.shared.external_layout {
            engine.frame_scope_declarations(id.anchor, |_| {})?;
        } else {
            engine.frame_scope(id.anchor, |_| {})?;
        }
        Ok(())
    }
    pub fn cancel(&mut self, id: ScopeId) -> Result<()> {
        if id.owner != self.shared.owner {
            return Err(Error::WrongRuntime);
        }
        if !self
            .shared
            .entries
            .borrow()
            .get(&id.entity())
            .is_some_and(|entry| entry.state.id == id)
        {
            return Err(Error::Closed);
        }
        self.clear(id)?;
        self.shared.reconcile(); // Descendant nodes are gone; drop child tasks first.
        if let Some(entry) = self.shared.entries.borrow_mut().get_mut(&id.entity()) {
            entry.state.wake.close();
            entry.task = None;
        }
        self.shared.capture();
        Ok(())
    }
    /// Queue input once in the common Engine; no replay/routing table is needed.
    pub fn queue_click(&mut self, entity: Entity) -> bool {
        let mut engine = self.shared.engine.borrow_mut();
        if engine.node(entity).is_none() {
            return false;
        }
        engine.queue_click(entity);
        let scope = engine.scope_for_entity(entity);
        drop(engine);
        self.wake_target(scope);
        true
    }
    pub fn queue_value(&mut self, entity: Entity, value: f64) -> bool {
        let mut engine = self.shared.engine.borrow_mut();
        if engine.node(entity).is_none() {
            return false;
        }
        engine.queue_value(entity, value);
        let scope = engine.scope_for_entity(entity);
        drop(engine);
        self.wake_target(scope);
        true
    }
    /// After native TextBuffer edits, notify the owner without serializing text.
    pub fn input_changed(&mut self, entity: Entity) {
        let scope = self.shared.engine.borrow().scope_for_entity(entity);
        self.wake_target(scope);
    }
    fn wake_target(&self, scope: Option<RetainedScope>) {
        if let Some(entry) = scope.and_then(|scope| {
            self.shared
                .entries
                .borrow()
                .get(&scope.entity())
                .map(|entry| entry.state.wake.clone())
        }) {
            entry.schedule();
        } else {
            self.shared.parent_dirty.set(true);
        }
    }
    pub fn invalidate(&self, id: ScopeId) -> Result<()> {
        if id.owner != self.shared.owner {
            return Err(Error::WrongRuntime);
        }
        let entries = self.shared.entries.borrow();
        let entry = entries
            .get(&id.entity())
            .filter(|entry| entry.state.id == id)
            .ok_or(Error::Closed)?;
        entry.state.wake.schedule();
        Ok(())
    }
    pub fn set_time(&mut self, time: FrameTime) {
        self.shared.engine.borrow_mut().set_time(time);
        if self.shared.parent_animate.get() {
            self.shared.parent_dirty.set(true);
        }
        for entry in self.shared.entries.borrow().values() {
            if entry.state.animate.get() {
                entry.state.wake.schedule();
            }
        }
    }
    pub fn set_theme(&mut self, theme: Theme) {
        let changed = self.shared.engine.borrow().theme() != theme;
        self.shared.engine.borrow_mut().set_theme(theme);
        if changed {
            self.invalidate_environment();
        }
    }
    pub fn set_viewport(&mut self, width: f32, height: f32) -> Result<()> {
        let changed = self.shared.engine.borrow().viewport_size() != [width, height];
        self.shared
            .engine
            .borrow_mut()
            .set_viewport(width, height)?;
        if changed {
            self.invalidate_environment();
        }
        Ok(())
    }
    fn invalidate_environment(&self) {
        self.shared.parent_dirty.set(true);
        for entry in self.shared.entries.borrow().values() {
            entry.state.wake.schedule();
        }
    }
    /// Union of all local/root transactions since the previous drain.
    pub fn take_delta(&mut self) -> FrameDelta {
        FrameDelta {
            changes: self
                .shared
                .dirty
                .borrow_mut()
                .drain()
                .map(|(_, change)| change)
                .collect(),
            removed: self.shared.removed.borrow_mut().drain().collect(),
            frame: self.shared.changes.replace(Changes::empty()),
        }
    }
    /// Selected checkpoints only; execution position is never serialized.
    pub fn dump_state(&self) -> Result<Vec<u8>> {
        let engine = self.shared.engine.borrow();
        let entries: Vec<_> = self
            .shared
            .entries
            .borrow()
            .values()
            .map(|entry| {
                (
                    engine.logical_path(entry.state.id.entity()),
                    entry.state.saved.borrow().clone(),
                )
            })
            .collect();
        rmp_serde::to_vec_named(&entries).map_err(|source| Error::Encode { source })
    }
    /// Restore after render/mount but before pump. A field decode failure falls
    /// back to its initializer in AsyncUi::state; invalid envelopes return error.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<()> {
        if self
            .shared
            .entries
            .borrow()
            .values()
            .any(|entry| entry.state.started.get())
        {
            return Err(Error::AlreadyStarted);
        }
        let snapshot: Vec<(Vec<plico_core::PathKey>, Checkpoints)> =
            rmp_serde::from_slice(bytes).map_err(|source| Error::Decode { source })?;
        let snapshot: HashMap<_, _> = snapshot.into_iter().collect();
        let engine = self.shared.engine.borrow();
        for entry in self.shared.entries.borrow().values() {
            if let Some(saved) = snapshot.get(&engine.logical_path(entry.state.id.entity())) {
                *entry.state.saved.borrow_mut() = saved.clone();
            }
        }
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let mut entries = self.shared.entries.borrow_mut();
        let mut ids: Vec<_> = entries.keys().copied().collect();
        ids.sort_by_key(|id| std::cmp::Reverse(depth(*id, &entries)));
        for id in ids {
            entries.remove(&id);
        }
    }
}
