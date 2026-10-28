//! Opt-in ID-driven native regions. Experimental; component root owns all state.
//! Root arguments are captures; only watch arguments subscribe to changes.
use crate::View;
use plico::{
    Button, Changes, Engine, Entity, ExperimentContext, FrameError, FrameStats, Key, Response,
    UiContext,
};
use slotmap::{SlotMap, new_key_type};
use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    marker::PhantomData,
    panic::Location,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
new_key_type! { pub struct RegionId; }
pub struct State<T> {
    owner: usize,
    index: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for State<T> {}
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}
pub struct Access<'a, T> {
    app: &'a Regions,
    state: State<T>,
}
impl<T: 'static> Access<'_, T> {
    pub fn handle(&self) -> State<T> {
        self.state
    }
    pub fn read(&self) -> Ref<'_, T> {
        Ref::map(self.app.slots[self.state.index].value.borrow(), |v| {
            v.downcast_ref().unwrap()
        })
    }
    /// Notify readers after an existing shared object (e.g. TextBuffer) changed.
    pub fn notify(&self) {
        assert!(
            !self.app.building.get(),
            "notify state in an event callback, not watch"
        );
        self.app
            .dirty
            .borrow_mut()
            .extend(self.app.slots[self.state.index].readers.iter().copied());
    }
    pub fn set(&self, value: T) {
        self.update(|current| *current = value);
    }
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        assert!(
            !self.app.building.get(),
            "write state in an event callback, not watch"
        );
        let slot = &self.app.slots[self.state.index];
        let result = f(slot.value.borrow_mut().downcast_mut().unwrap());
        self.app
            .dirty
            .borrow_mut()
            .extend(slot.readers.iter().copied());
        result
    }
}
struct Slot {
    value: RefCell<Box<dyn Any>>,
    readers: Vec<RegionId>,
}
/// A dependency tuple selects concrete function-pointer types. This also lets
/// named functions coerce without casts on stable Rust's higher-ranked solver.
pub trait Dependencies: 'static {
    type Callback: Copy;
    type EventCallback: Copy;
    type ChangeCallback<V: 'static>: Copy;
    #[doc(hidden)]
    fn bindings(&self) -> Vec<(usize, usize)>;
    #[doc(hidden)]
    fn render(&self, callback: Self::Callback, app: &Regions, cx: &mut UiContext<'_>);
    #[doc(hidden)]
    fn event(&self, callback: Self::EventCallback, app: &Regions);
    #[doc(hidden)]
    fn change<V: 'static>(&self, callback: Self::ChangeCallback<V>, app: &Regions, value: V);
}
impl Dependencies for () {
    type Callback = for<'a, 'b> fn(&'a mut UiContext<'b>, ());
    type EventCallback = fn(());
    type ChangeCallback<V: 'static> = fn((), V);
    fn bindings(&self) -> Vec<(usize, usize)> {
        Vec::new()
    }
    fn render(&self, callback: Self::Callback, _: &Regions, cx: &mut UiContext<'_>) {
        callback(cx, ());
    }
    fn event(&self, callback: Self::EventCallback, _: &Regions) {
        callback(());
    }
    fn change<V: 'static>(&self, callback: Self::ChangeCallback<V>, _: &Regions, value: V) {
        callback((), value);
    }
}
macro_rules! dependency_tuple {
    ($($name:ident : $index:tt),+) => {
        impl<$($name: 'static),+> Dependencies for ($(State<$name>,)+) {
            type Callback = for<'a,'b,'c> fn(&'a mut UiContext<'b>, ($(Access<'c,$name>,)+));
            type EventCallback = for<'a> fn(($(Access<'a,$name>,)+));
            type ChangeCallback<V: 'static> = for<'a> fn(($(Access<'a,$name>,)+), V);
            fn bindings(&self) -> Vec<(usize,usize)> { vec![$((self.$index.owner,self.$index.index),)+] }
            fn render(&self, callback: Self::Callback, app: &Regions, cx: &mut UiContext<'_>) { callback(cx, ($(app.access(self.$index),)+)); }
            fn event(&self, callback: Self::EventCallback, app: &Regions) { callback(($(app.access(self.$index),)+)); }
            fn change<V: 'static>(&self, callback: Self::ChangeCallback<V>, app: &Regions, value: V) { callback(($(app.access(self.$index),)+), value); }
        }
    };
}
dependency_tuple!(A:0);
dependency_tuple!(A:0,B:1);
dependency_tuple!(A:0,B:1,C:2);
dependency_tuple!(A:0,B:1,C:2,D:3);
dependency_tuple!(A:0,B:1,C:2,D:3,E:4);
dependency_tuple!(A:0,B:1,C:2,D:3,E:4,F:5);
pub type RenderCallback<D> = <D as Dependencies>::Callback;
pub type EventCallback<D> = <D as Dependencies>::EventCallback;
trait Render {
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>);
}
struct RenderFn<D: Dependencies> {
    deps: D,
    callback: RenderCallback<D>,
}
impl<D: Dependencies> Render for RenderFn<D> {
    fn run(&self, app: &Regions, cx: &mut UiContext<'_>) {
        self.deps.render(self.callback, app, cx);
    }
}
struct Spec {
    deps: Vec<(usize, usize)>,
    render: Box<dyn Render>,
}
fn render_spec<D: Dependencies>(deps: D, callback: RenderCallback<D>) -> Spec {
    Spec {
        deps: deps.bindings(),
        render: Box::new(RenderFn { deps, callback }),
    }
}
trait Action {
    fn run(self: Box<Self>, app: &Regions);
}
struct EventSpec(Box<dyn Action>);
struct Event<D: Dependencies> {
    deps: D,
    callback: EventCallback<D>,
}
impl<D: Dependencies> Action for Event<D> {
    fn run(self: Box<Self>, app: &Regions) {
        self.deps.event(self.callback, app);
    }
}
pub type ChangeCallback<D, V> = <D as Dependencies>::ChangeCallback<V>;
struct Change<D: Dependencies, V: 'static> {
    deps: D,
    value: V,
    callback: ChangeCallback<D, V>,
}
impl<D: Dependencies, V: 'static> Action for Change<D, V> {
    fn run(self: Box<Self>, app: &Regions) {
        self.deps.change(self.callback, app, self.value);
    }
}
fn queue_change<D: Dependencies, V: 'static>(deps: D, value: V, callback: ChangeCallback<D, V>) {
    commands()
        .borrow_mut()
        .actions
        .push(EventSpec(Box::new(Change {
            deps,
            value,
            callback,
        })));
}
/// Owned event payloads, such as a row's business ID, are explicit too.
/// ```compile_fail
/// use plico_runtime::regions::{Regions, OnClickWith};
/// let app = Regions::new(|app| {
///     let count = app.state(|| 0);
///     app.root((count,), |cx, (count,)| {
///         let extra = 2;
///         cx.button("+").on_click_with((count.handle(),), 1, |(count,), amount| {
///             count.update(|value| *value += amount + extra);
///         });
///     });
/// });
/// ```
pub trait OnClickWith {
    fn on_click_with<D: Dependencies, V: 'static>(
        self,
        deps: D,
        value: V,
        callback: ChangeCallback<D, V>,
    ) -> Response;
}
impl OnClickWith for Button<'_, '_, '_> {
    fn on_click_with<D: Dependencies, V: 'static>(
        self,
        deps: D,
        value: V,
        callback: ChangeCallback<D, V>,
    ) -> Response {
        let response = self.response();
        if response.clicked() {
            queue_change(deps, value, callback);
        }
        response
    }
}
pub trait OnChange {
    type Value: 'static;
    type Response;
    fn on_change<D: Dependencies>(
        self,
        deps: D,
        callback: ChangeCallback<D, Self::Value>,
    ) -> Self::Response;
}
impl OnChange for plico::Toggle<'_, '_, '_> {
    type Value = bool;
    type Response = plico::ToggleResponse;
    fn on_change<D: Dependencies>(
        self,
        deps: D,
        callback: ChangeCallback<D, bool>,
    ) -> Self::Response {
        let response = self.response();
        if response.changed() {
            queue_change(deps, response.checked(), callback);
        }
        response
    }
}
impl OnChange for plico::Slider<'_, '_, '_> {
    type Value = f64;
    type Response = plico::SliderResponse;
    fn on_change<D: Dependencies>(
        self,
        deps: D,
        callback: ChangeCallback<D, f64>,
    ) -> Self::Response {
        let response = self.response();
        if response.changed {
            queue_change(deps, response.value, callback);
        }
        response
    }
}
impl OnChange for plico::Input<'_, '_, '_> {
    type Value = plico::InputResponse;
    type Response = plico::InputResponse;
    fn on_change<D: Dependencies>(
        self,
        deps: D,
        callback: ChangeCallback<D, plico::InputResponse>,
    ) -> Self::Response {
        let response = self.response();
        if response.changed {
            queue_change(deps, response, callback);
        }
        response
    }
}
/// Bind a controlled response after all temporary declaration borrows end.
pub trait BindState<T> {
    type Response;
    fn bind(self, state: State<T>) -> Self::Response;
}
impl BindState<bool> for plico::Toggle<'_, '_, '_> {
    type Response = plico::ToggleResponse;
    fn bind(self, state: State<bool>) -> Self::Response {
        self.on_change((state,), |(state,), value| state.set(value))
    }
}
impl BindState<f64> for plico::Slider<'_, '_, '_> {
    type Response = plico::SliderResponse;
    fn bind(self, state: State<f64>) -> Self::Response {
        self.on_change((state,), |(state,), value| state.set(value))
    }
}
impl BindState<plico::TextBuffer> for plico::Input<'_, '_, '_> {
    type Response = plico::InputResponse;
    fn bind(self, state: State<plico::TextBuffer>) -> Self::Response {
        let response = self.response();
        if response.changed || response.submitted || response.navigation.is_some() {
            queue_change((state,), response, |(state,), _| state.notify());
        }
        response
    }
}
pub trait OnSubmit {
    fn on_submit<D: Dependencies>(self, deps: D, callback: EventCallback<D>) -> Self;
}
impl OnSubmit for plico::InputResponse {
    fn on_submit<D: Dependencies>(self, deps: D, callback: EventCallback<D>) -> Self {
        if self.submitted {
            commands()
                .borrow_mut()
                .actions
                .push(EventSpec(Box::new(Event { deps, callback })));
        }
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ChildKey {
    parent: Entity,
    key: u64,
}
struct Child {
    key: ChildKey,
    context: ExperimentContext,
    spec: Spec,
}
#[derive(Default)]
struct Commands {
    partial: bool,
    children: Vec<Child>,
    actions: Vec<EventSpec>,
}
thread_local! {static BUILD:RefCell<Vec<Rc<RefCell<Commands>>>>=const{RefCell::new(Vec::new())};}
fn commands() -> Rc<RefCell<Commands>> {
    BUILD.with(|stack| {
        Rc::clone(
            stack
                .borrow()
                .last()
                .expect("region API outside a region callback"),
        )
    })
}
struct BuildGuard;
impl Drop for BuildGuard {
    fn drop(&mut self) {
        BUILD.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}
/// Extension is available inside any ordinary trailing-lambda children closure.
pub trait WatchUi {
    #[track_caller]
    fn watch<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>);
    fn watch_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>);
}
impl WatchUi for UiContext<'_> {
    #[track_caller]
    fn watch<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>) {
        let site = Location::caller();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (site.file(), site.line(), site.column()).hash(&mut hash);
        self.watch_keyed(hash.finish(), deps, callback);
    }
    fn watch_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>) {
        let output = commands();
        let partial = output.borrow().partial;
        let parent = self.experiment_context().parent;
        let mut context = None;
        self.column().key(Key::Generated(key)).children(|cx| {
            context = Some(cx.experiment_context());
            if partial {
                cx.experiment_keep_children();
            }
        });
        output.borrow_mut().children.push(Child {
            key: ChildKey { parent, key },
            context: context.unwrap(),
            spec: render_spec(deps, callback),
        });
    }
}
/// Stored event functions cannot capture values outside the dependency tuple.
/// ```compile_fail
/// use plico_runtime::regions::{Regions, OnClick};
/// let app = Regions::new(|app| {
///     let count = app.state(|| 0);
///     app.root((count,), |cx, (count,)| {
///         let amount = 2;
///         cx.button("+").on_click((count.handle(),), |(count,)| {
///             count.update(|value| *value += amount);
///         });
///     });
/// });
/// ```
pub trait OnClick {
    fn on_click<D: Dependencies>(self, deps: D, callback: EventCallback<D>) -> Response;
}
impl OnClick for Button<'_, '_, '_> {
    fn on_click<D: Dependencies>(self, deps: D, callback: EventCallback<D>) -> Response {
        let response = self.response();
        if response.clicked() {
            commands()
                .borrow_mut()
                .actions
                .push(EventSpec(Box::new(Event { deps, callback })));
        }
        response
    }
}
struct Region {
    render: Box<dyn Render>,
    deps: Vec<usize>,
    parent: Option<RegionId>,
    children: HashMap<ChildKey, RegionId>,
    context: Option<ExperimentContext>,
    runs: usize,
}
/// A single component instance. State values die with this owner, not with watches.
pub struct Regions {
    owner: usize,
    slots: Vec<Slot>,
    regions: SlotMap<RegionId, Region>,
    root: Option<RegionId>,
    dirty: RefCell<VecDeque<RegionId>>,
    building: Cell<bool>,
    bindings: HashMap<Entity, RegionId>,
    initialized: bool,
}
#[doc(hidden)]
pub struct ReplayBatch {
    pub stats: FrameStats,
    pub dirty: Vec<Entity>,
    pub changes: Changes,
    pub passes: usize,
}
impl Regions {
    pub fn new(build: impl FnOnce(&mut Self)) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let owner = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .expect("region owner IDs exhausted");
        let mut app = Self {
            owner,
            slots: vec![],
            regions: SlotMap::with_key(),
            root: None,
            dirty: RefCell::new(VecDeque::new()),
            building: Cell::new(false),
            bindings: HashMap::new(),
            initialized: false,
        };
        build(&mut app);
        assert!(app.root.is_some(), "component root missing");
        app
    }
    pub fn state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        assert!(
            !self.initialized,
            "create component state during construction"
        );
        let index = self.slots.len();
        self.slots.push(Slot {
            value: RefCell::new(Box::new(init())),
            readers: vec![],
        });
        State {
            owner: self.owner,
            index,
            marker: PhantomData,
        }
    }
    /// Root arguments are explicit captures, not reactive subscriptions.
    /// ```compile_fail
    /// use plico_runtime::regions::Regions;
    /// let label = String::from("implicit capture");
    /// let app = Regions::new(|app| app.root((), |cx, ()| { cx.text(&label); }));
    /// ```
    pub fn root<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>) {
        assert!(self.root.is_none());
        let spec = render_spec(deps, callback);
        self.check(&spec);
        self.root = Some(self.insert(spec, None, false));
    }
    pub fn access<T: 'static>(&self, state: State<T>) -> Access<'_, T> {
        assert_eq!(self.owner, state.owner, "foreign state owner");
        assert!(state.index < self.slots.len());
        Access { app: self, state }
    }
    fn check(&self, spec: &Spec) {
        for &(owner, index) in &spec.deps {
            assert_eq!(owner, self.owner, "foreign state owner");
            assert!(index < self.slots.len());
        }
    }
    fn insert(&mut self, spec: Spec, parent: Option<RegionId>, subscribe: bool) -> RegionId {
        self.check(&spec);
        let mut deps: Vec<_> = if subscribe {
            spec.deps.iter().map(|v| v.1).collect()
        } else {
            vec![]
        };
        deps.sort_unstable();
        deps.dedup();
        let id = self.regions.insert(Region {
            render: spec.render,
            deps,
            parent,
            children: HashMap::new(),
            context: None,
            runs: 0,
        });
        for &index in &self.regions[id].deps {
            self.slots[index].readers.push(id);
        }
        id
    }
    fn remove(&mut self, id: RegionId) {
        if let Some(region) = self.regions.remove(id) {
            for child in region.children.into_values() {
                self.remove(child);
            }
            for index in region.deps {
                self.slots[index].readers.retain(|r| *r != id);
            }
            if let Some(cx) = region.context {
                self.bindings.remove(&cx.parent);
            }
        }
    }
    fn build(&mut self, id: RegionId, cx: &mut UiContext<'_>, partial: bool) {
        let commands = Rc::new(RefCell::new(Commands {
            partial,
            ..Commands::default()
        }));
        BUILD.with(|stack| stack.borrow_mut().push(Rc::clone(&commands)));
        let guard = BuildGuard;
        struct ReadGuard<'a>(&'a Cell<bool>);
        impl Drop for ReadGuard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        self.building.set(true);
        let read_guard = ReadGuard(&self.building);
        self.regions[id].render.run(self, cx);
        drop(read_guard);
        drop(guard);
        let mut output = commands.borrow_mut();
        let children = std::mem::take(&mut output.children);
        let actions = std::mem::take(&mut output.actions);
        drop(output);
        self.regions[id].runs += 1;
        let mut old = std::mem::take(&mut self.regions[id].children);
        let mut next = HashMap::new();
        let mut visit = Vec::new();
        for child in children {
            self.check(&child.spec);
            assert!(!next.contains_key(&child.key), "duplicate watch key");
            let child_id = if let Some(child_id) = old.remove(&child.key) {
                let mut deps: Vec<_> = child.spec.deps.iter().map(|d| d.1).collect();
                deps.sort_unstable();
                deps.dedup();
                let previous = std::mem::replace(&mut self.regions[child_id].deps, deps);
                for index in previous {
                    self.slots[index].readers.retain(|r| *r != child_id);
                }
                for &index in &self.regions[child_id].deps {
                    self.slots[index].readers.push(child_id);
                }
                self.regions[child_id].render = child.spec.render;
                child_id
            } else {
                self.insert(child.spec, Some(id), true)
            };
            if let Some(previous) = self.regions[child_id].context {
                self.bindings.remove(&previous.parent);
            }
            self.regions[child_id].context = Some(child.context);
            self.bindings.insert(child.context.parent, child_id);
            next.insert(child.key, child_id);
            visit.push((child_id, child.context));
        }
        for child in old.into_values() {
            self.remove(child);
        }
        self.regions[id].children = next;
        for action in actions {
            action.0.run(self);
        }
        for (child, context) in visit {
            if partial {
                self.dirty.get_mut().push_back(child);
            } else {
                cx.experiment_enter(context, |cx| self.build(child, cx, false));
            }
        }
    }
    fn depth(&self, mut id: RegionId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.regions.get(id).and_then(|r| r.parent) {
            depth += 1;
            id = parent;
        }
        depth
    }
    fn replay(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        if !self.initialized || !engine.experiment_replay_ready() {
            return None;
        }
        for entity in engine.experiment_input_entities() {
            let mut cursor = Some(entity);
            while let Some(entity) = cursor {
                if let Some(&id) = self.bindings.get(&entity) {
                    self.dirty.get_mut().push_back(id);
                    break;
                }
                cursor = engine.experiment_parent(entity);
            }
        }
        if self.dirty.get_mut().is_empty() {
            return None;
        }
        let mut batch = ReplayBatch {
            stats: FrameStats::default(),
            dirty: vec![],
            changes: Changes::empty(),
            passes: 0,
        };
        let mut seen = HashSet::new();
        let mut executions = HashMap::<RegionId, usize>::new();
        loop {
            let mut queued: Vec<_> = self
                .dirty
                .get_mut()
                .drain(..)
                .filter(|id| self.regions.contains_key(*id))
                .collect();
            queued.sort_by_key(|id| self.depth(*id));
            let mut unique = HashSet::new();
            queued.retain(|id| unique.insert(*id));
            if queued.is_empty() {
                batch.changes.remove(Changes::VIEW);
                return Some(Ok(batch));
            }
            // If an ancestor is dirty it refreshes its children after it runs.
            let selected: HashSet<_> = queued.iter().copied().collect();
            queued.retain(|id| {
                let mut p = self.regions[*id].parent;
                while let Some(parent) = p {
                    if selected.contains(&parent) {
                        return false;
                    }
                    p = self.regions[parent].parent;
                }
                true
            });
            for id in queued {
                let Some(context) = self.regions.get(id).and_then(|r| r.context) else {
                    continue;
                };
                let count = executions.entry(id).or_default();
                *count += 1;
                if *count > 8 {
                    return Some(Err(FrameError::DidNotSettle));
                }
                match engine.experiment_redeclare(context, |cx| self.build(id, cx, true)) {
                    Ok(stats) => {
                        batch.stats.visited += stats.visited;
                        batch.stats.created += stats.created;
                        batch.stats.reused += stats.reused;
                        batch.stats.removed += stats.removed;
                        batch.stats.text_writes += stats.text_writes;
                        batch.stats.paint_updates += stats.paint_updates;
                        batch.stats.order_changes += stats.order_changes;
                    }
                    Err(error) => return Some(Err(error)),
                }
                batch.passes += 1;
                if engine.changes().contains(Changes::VIEW) {
                    self.dirty.get_mut().push_back(id);
                }
                batch.changes |= engine.changes();
                for &entity in engine.dirty_nodes() {
                    if seen.insert(entity) {
                        batch.dirty.push(entity);
                    }
                }
            }
        }
    }
    pub fn region_runs(&self) -> Vec<(Entity, usize)> {
        self.regions
            .values()
            .filter_map(|r| r.context.map(|cx| (cx.parent, r.runs)))
            .collect()
    }
}
impl View for Regions {
    fn snapshot_changed(&self) -> bool {
        !self.dirty.borrow().is_empty()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.dirty.get_mut().clear();
        let root = self.root.unwrap();
        cx.column()
            .key("regions-root")
            .width_percent(100.)
            .height_percent(100.)
            .children(|cx| {
                let context = cx.experiment_context();
                self.regions[root].context = Some(context);
                self.bindings.insert(context.parent, root);
                self.build(root, cx, false);
            });
        self.initialized = true;
        if !self.dirty.get_mut().is_empty() {
            cx.request_rebuild();
        }
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        self.replay(engine)
    }
}
