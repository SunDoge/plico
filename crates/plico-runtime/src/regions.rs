//! ID-driven native regions; components own their local state.
//! Root arguments are captures; watch and property arguments subscribe to changes.
mod properties;
mod reload;
mod routing;
pub use properties::{Reactive, TextProperties};
pub use routing::{outlet, outlet_fill, outlet_keyed};
mod tasks;
use crate::View;
use plico::{
    Button, Changes, Engine, Entity, ExperimentContext, FrameError, FrameStats, Key, Response,
    UiContext,
};
pub use reload::{Checkpoint, CheckpointError, RegionApp, RegionProgram, RestoreStats};
use slotmap::{SlotMap, new_key_type};
use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    marker::PhantomData,
    panic::Location,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
pub use tasks::StateSender;
new_key_type! { pub struct RegionId; pub struct ComponentId; }
type Binding = (usize, ComponentId, usize);
#[derive(Default)]
struct Storage {
    slots: elsa::FrozenVec<Box<Slot>>,
    locals: RefCell<HashMap<u64, usize>>,
    path: Vec<plico::PathKey>,
    restore: Rc<reload::Restore>,
}
struct Component {
    parent: Option<ComponentId>,
    storage: Rc<Storage>,
}
pub struct State<T> {
    owner: usize,
    component: ComponentId,
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
    pub fn sender(&self) -> StateSender<T>
    where
        T: Send,
    {
        self.app.sender(self.state)
    }
    pub fn handle(&self) -> State<T> {
        self.state
    }
    pub fn read(&self) -> Ref<'_, T> {
        Ref::map(
            self.app
                .slot(self.state.component, self.state.index)
                .value
                .borrow(),
            |v| v.downcast_ref().unwrap(),
        )
    }
    /// Notify readers after an existing shared object (e.g. TextBuffer) changed.
    pub fn notify(&self) {
        assert!(
            !self.app.building.get(),
            "notify state in an event callback, not watch"
        );
        self.app.dirty.borrow_mut().extend(
            self.app
                .slot(self.state.component, self.state.index)
                .readers
                .borrow()
                .iter()
                .copied(),
        );
    }
    pub fn set(&self, value: T) {
        self.update(|current| *current = value);
    }
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        assert!(
            !self.app.building.get(),
            "write state in an event callback, not watch"
        );
        let slot = &self.app.slot(self.state.component, self.state.index);
        let result = f(slot.value.borrow_mut().downcast_mut().unwrap());
        self.app
            .dirty
            .borrow_mut()
            .extend(slot.readers.borrow().iter().copied());
        result
    }
}
struct Slot {
    value: RefCell<Box<dyn Any>>,
    readers: RefCell<Vec<RegionId>>,
    persistent: Option<reload::Codec>,
}
/// A dependency tuple selects concrete function-pointer types. This also lets
/// named functions coerce without casts on stable Rust's higher-ranked solver.
pub trait Dependencies: 'static {
    type Callback: Copy;
    type EventCallback: Copy;
    type PropertyCallback: Copy;
    type RouteCallback<R: 'static>: Copy;
    type ChangeCallback<V: 'static>: Copy;
    #[doc(hidden)]
    fn bindings(&self) -> Vec<Binding>;
    #[doc(hidden)]
    fn properties(
        &self,
        callback: Self::PropertyCallback,
        app: &Regions,
        target: &mut TextProperties,
    );
    #[doc(hidden)]
    fn render(&self, callback: Self::Callback, app: &Regions, cx: &mut UiContext<'_>);
    #[doc(hidden)]
    fn event(&self, callback: Self::EventCallback, app: &Regions);
    #[doc(hidden)]
    fn route<R: 'static>(
        &self,
        callback: Self::RouteCallback<R>,
        app: &Regions,
        cx: &mut UiContext<'_>,
        route: R,
    );
    #[doc(hidden)]
    fn change<V: 'static>(&self, callback: Self::ChangeCallback<V>, app: &Regions, value: V);
}
impl Dependencies for () {
    type Callback = for<'a, 'b> fn(&'a mut UiContext<'b>, ());
    type EventCallback = fn(());
    type PropertyCallback = fn(&mut TextProperties, ());
    type RouteCallback<R: 'static> = for<'a, 'b> fn(&'a mut UiContext<'b>, R, ());
    fn route<R: 'static>(
        &self,
        callback: Self::RouteCallback<R>,
        _: &Regions,
        cx: &mut UiContext<'_>,
        route: R,
    ) {
        callback(cx, route, ());
    }
    fn properties(
        &self,
        callback: Self::PropertyCallback,
        _: &Regions,
        target: &mut TextProperties,
    ) {
        callback(target, ());
    }
    type ChangeCallback<V: 'static> = fn((), V);
    fn bindings(&self) -> Vec<Binding> {
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
            type PropertyCallback = for<'a, 'b> fn(&'a mut TextProperties, ($(Access<'b,$name>,)+));
            type RouteCallback<R: 'static> = for<'a,'b,'c> fn(&'a mut UiContext<'b>, R, ($(Access<'c,$name>,)+));
            fn route<R: 'static>(&self, callback: Self::RouteCallback<R>, app: &Regions, cx: &mut UiContext<'_>, route: R) { callback(cx, route, ($(app.access(self.$index),)+)); }
            fn properties(&self, callback: Self::PropertyCallback, app: &Regions, target: &mut TextProperties) { callback(target, ($(app.access(self.$index),)+)); }
            type ChangeCallback<V: 'static> = for<'a> fn(($(Access<'a,$name>,)+), V);
            fn bindings(&self) -> Vec<Binding> { vec![$((self.$index.owner,self.$index.component,self.$index.index),)+] }
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
    fn property(&self) -> bool {
        false
    }
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
    deps: Vec<Binding>,
    subscriptions: Option<Vec<Binding>>,
    render: Box<dyn Render>,
    fill: bool,
}
fn render_spec<D: Dependencies>(deps: D, callback: RenderCallback<D>) -> Spec {
    Spec {
        deps: deps.bindings(),
        subscriptions: None,
        render: Box::new(RenderFn { deps, callback }),
        fill: false,
    }
}
trait Action {
    fn bindings(&self) -> Vec<Binding>;
    fn run(self: Box<Self>, app: &Regions);
}
struct EventSpec(Box<dyn Action>);
struct Event<D: Dependencies> {
    deps: D,
    callback: EventCallback<D>,
}
impl<D: Dependencies> Action for Event<D> {
    fn bindings(&self) -> Vec<Binding> {
        self.deps.bindings()
    }
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
    fn bindings(&self) -> Vec<Binding> {
        self.deps.bindings()
    }
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
impl OnClickWith for Response {
    fn on_click_with<D: Dependencies, V: 'static>(
        self,
        deps: D,
        value: V,
        callback: ChangeCallback<D, V>,
    ) -> Response {
        if self.clicked() {
            queue_change(deps, value, callback);
        }
        self
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
    // Engine identity already includes parent, key and lexical scope. Using its
    // generational target also distinguishes keyed instances of one call site.
    target: Entity,
    component: bool,
}
struct Child {
    key: ChildKey,
    context: ExperimentContext,
    path: Vec<plico::PathKey>,
    spec: Spec,
}
struct Commands {
    owner: usize,
    component: ComponentId,
    storage: Rc<Storage>,
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
struct ReadGuard<'a>(&'a Cell<bool>, bool);
impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
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
    /// A watch whose boundary participates in flex growth, for page-sized UI.
    #[track_caller]
    fn watch_fill<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>);
    fn watch_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>);
    /// A static component uses its call site as identity, like `watch`.
    #[track_caller]
    fn component<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>);
    /// A keyed component owns local state; removing it invalidates all its handles.
    fn component_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>);
    #[track_caller]
    fn local_state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T>;
    /// Opt-in MessagePack checkpoint with an explicit, stable slot key.
    fn local_persisted_state<T: serde::Serialize + serde::de::DeserializeOwned + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> T,
    ) -> State<T>;
    fn local_state_keyed<T: 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T>;
}
impl WatchUi for UiContext<'_> {
    #[track_caller]
    fn watch<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>) {
        let site = Location::caller();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (site.file(), site.line(), site.column()).hash(&mut hash);
        self.watch_keyed(hash.finish(), deps, callback);
    }
    #[track_caller]
    fn watch_fill<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>) {
        let site = Location::caller();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (site.file(), site.line(), site.column()).hash(&mut hash);
        child(
            self,
            hash.finish(),
            false,
            Spec {
                fill: true,
                ..render_spec(deps, callback)
            },
        );
    }
    fn watch_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>) {
        child(self, key, false, render_spec(deps, callback));
    }
    #[track_caller]
    fn component<D: Dependencies>(&mut self, deps: D, callback: RenderCallback<D>) {
        let site = Location::caller();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (site.file(), site.line(), site.column()).hash(&mut hash);
        self.component_keyed(hash.finish(), deps, callback);
    }
    fn component_keyed<D: Dependencies>(&mut self, key: u64, deps: D, callback: RenderCallback<D>) {
        child(self, key, true, render_spec(deps, callback));
    }
    #[track_caller]
    fn local_state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let site = Location::caller();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (site.file(), site.line(), site.column()).hash(&mut hash);
        self.local_state_keyed(hash.finish(), init)
    }
    fn local_persisted_state<T: serde::Serialize + serde::de::DeserializeOwned + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        let output = commands();
        let output = output.borrow();
        let index = output
            .storage
            .local(key, init, Some(reload::Codec::of::<T>(key)));
        State {
            owner: output.owner,
            component: output.component,
            index,
            marker: PhantomData,
        }
    }
    fn local_state_keyed<T: 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        let output = commands();
        let output = output.borrow();
        let index = output.storage.local(key, init, None);
        State {
            owner: output.owner,
            component: output.component,
            index,
            marker: PhantomData,
        }
    }
}
fn child(cx: &mut UiContext<'_>, key: u64, component: bool, spec: Spec) {
    // Distinct identity namespaces for components and watches.
    let identity = if component {
        Key::Number(key)
    } else {
        Key::Generated(key)
    };
    child_with_key(cx, identity, component, spec);
}
fn child_with_key(cx: &mut UiContext<'_>, identity: Key, component: bool, spec: Spec) {
    let output = commands();
    let partial = output.borrow().partial;
    let mut context = None;
    let mut path = vec![];
    let fill = spec.fill;
    let mut boundary = cx.column().key(identity);
    if fill {
        boundary = boundary.width_percent(100.).height(0.).grow(1.);
    }
    boundary.children(|cx| {
        context = Some(cx.experiment_context());
        if component {
            path = cx.experiment_logical_path();
        }
        if partial {
            cx.experiment_keep_children();
        }
    });
    let context = context.unwrap();
    output.borrow_mut().children.push(Child {
        key: ChildKey {
            target: context.parent,
            component,
        },
        context,
        path,
        spec,
    });
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
impl OnClick for Response {
    fn on_click<D: Dependencies>(self, deps: D, callback: EventCallback<D>) -> Response {
        if self.clicked() {
            commands()
                .borrow_mut()
                .actions
                .push(EventSpec(Box::new(Event { deps, callback })));
        }
        self
    }
}
struct Region {
    render: Box<dyn Render>,
    deps: Vec<Binding>,
    component: ComponentId,
    owns_component: bool,
    parent: Option<RegionId>,
    children: HashMap<ChildKey, RegionId>,
    context: Option<ExperimentContext>,
    runs: usize,
}
/// Region runtime. Root and keyed components own state; watches do not.
pub struct Regions {
    owner: usize,
    theme: Option<State<plico::Theme>>,
    mailbox: Arc<tasks::Mailbox>,
    components: SlotMap<ComponentId, Component>,
    root_component: ComponentId,
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
        let mut components = SlotMap::with_key();
        let root_component = components.insert(Component {
            parent: None,
            storage: Rc::new(Storage::default()),
        });
        let mailbox = Arc::new(tasks::Mailbox::default());
        mailbox.register(root_component);
        let mut app = Self {
            owner,
            theme: None,
            mailbox,
            components,
            root_component,
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
    /// Bind global theme to a root-owned state; inherited styles remain supplied
    /// by the ordinary UiContext. A theme change requires a full declaration.
    pub fn theme_state(&mut self, state: State<plico::Theme>) {
        assert!(
            self.try_access(state).is_some(),
            "foreign or expired theme state"
        );
        assert_eq!(
            state.component, self.root_component,
            "theme must be root-owned"
        );
        self.theme = Some(state);
    }
    pub fn state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        assert!(
            !self.initialized,
            "create component state during construction"
        );
        let slots = &self.components[self.root_component].storage.slots;
        let index = slots.len();
        slots.push(Box::new(Slot {
            value: RefCell::new(Box::new(init())),
            readers: RefCell::new(vec![]),
            persistent: None,
        }));
        State {
            owner: self.owner,
            component: self.root_component,
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
        self.check(&spec, self.root_component);
        self.root = Some(self.insert(spec, None, false, self.root_component, false));
    }
    pub fn access<T: 'static>(&self, state: State<T>) -> Access<'_, T> {
        self.try_access(state)
            .expect("foreign, expired or incompatible state")
    }
    pub fn try_access<T: 'static>(&self, state: State<T>) -> Option<Access<'_, T>> {
        if self.owner != state.owner {
            return None;
        }
        let slot = self
            .components
            .get(state.component)?
            .storage
            .slots
            .get(state.index)?;
        slot.value
            .borrow()
            .is::<T>()
            .then_some(Access { app: self, state })
    }
    fn slot(&self, component: ComponentId, index: usize) -> &Slot {
        &self.components[component].storage.slots[index]
    }
    fn check(&self, spec: &Spec, component: ComponentId) {
        self.check_bindings(&spec.deps, component);
    }
    fn check_bindings(&self, bindings: &[Binding], component: ComponentId) {
        for &(owner, origin, index) in bindings {
            assert_eq!(owner, self.owner, "foreign state owner");
            assert!(
                self.components[origin].storage.slots.get(index).is_some(),
                "expired state"
            );
            let mut cursor = Some(component);
            while cursor != Some(origin) {
                cursor = self.components
                    [cursor.expect("state must belong to this component or an ancestor")]
                .parent;
            }
        }
    }
    fn insert(
        &mut self,
        spec: Spec,
        parent: Option<RegionId>,
        subscribe: bool,
        component: ComponentId,
        owns_component: bool,
    ) -> RegionId {
        self.mailbox.register(component);
        self.check(&spec, component);
        let mut deps = if subscribe {
            spec.subscriptions.as_ref().unwrap_or(&spec.deps).clone()
        } else {
            vec![]
        };
        deps.sort_by_key(|d| (d.0, slotmap::Key::data(&d.1).as_ffi(), d.2));
        deps.dedup();
        let id = self.regions.insert(Region {
            render: spec.render,
            component,
            owns_component,
            deps,
            parent,
            children: HashMap::new(),
            context: None,
            runs: 0,
        });
        for &(_, component, index) in &self.regions[id].deps {
            self.slot(component, index).readers.borrow_mut().push(id);
        }
        id
    }
    fn remove(&mut self, id: RegionId) {
        if let Some(region) = self.regions.remove(id) {
            for child in region.children.into_values() {
                self.remove(child);
            }
            for (_, component, index) in region.deps {
                self.slot(component, index)
                    .readers
                    .borrow_mut()
                    .retain(|r| *r != id);
            }
            if region.owns_component {
                self.mailbox.remove(region.component);
                self.components.remove(region.component);
            }
            if let Some(cx) = region.context {
                self.bindings.remove(&cx.parent);
            }
        }
    }
    fn build(&mut self, id: RegionId, cx: &mut UiContext<'_>, partial: bool) {
        // Properties cannot declare children or enqueue UI events. Avoid the
        // per-declaration command buffer and child reconciliation entirely.
        if self.regions[id].render.property() {
            let guard = ReadGuard(&self.building, self.building.replace(true));
            self.regions[id].render.run(self, cx);
            drop(guard);
            self.regions[id].runs += 1;
            return;
        }
        let commands = Rc::new(RefCell::new(Commands {
            partial,
            owner: self.owner,
            component: self.regions[id].component,
            storage: Rc::clone(&self.components[self.regions[id].component].storage),
            children: vec![],
            actions: vec![],
        }));
        BUILD.with(|stack| stack.borrow_mut().push(Rc::clone(&commands)));
        let guard = BuildGuard;
        let read_guard = ReadGuard(&self.building, self.building.replace(true));
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
            self.check(&child.spec, self.regions[id].component);
            assert!(!next.contains_key(&child.key), "duplicate watch key");
            let child_id = if let Some(child_id) = old.remove(&child.key) {
                let mut deps = if child.key.component {
                    vec![]
                } else {
                    child
                        .spec
                        .subscriptions
                        .as_ref()
                        .unwrap_or(&child.spec.deps)
                        .clone()
                };
                deps.sort_by_key(|d| (d.0, slotmap::Key::data(&d.1).as_ffi(), d.2));
                deps.dedup();
                let previous = std::mem::replace(&mut self.regions[child_id].deps, deps);
                for (_, component, index) in previous {
                    self.slot(component, index)
                        .readers
                        .borrow_mut()
                        .retain(|r| *r != child_id);
                }
                for &(_, component, index) in &self.regions[child_id].deps {
                    self.slot(component, index)
                        .readers
                        .borrow_mut()
                        .push(child_id);
                }
                self.regions[child_id].render = child.spec.render;
                child_id
            } else {
                let parent_component = self.regions[id].component;
                let component = if child.key.component {
                    self.components.insert(Component {
                        parent: Some(parent_component),
                        storage: Rc::new(Storage {
                            path: child.path,
                            restore: Rc::clone(
                                &self.components[self.root_component].storage.restore,
                            ),
                            ..Storage::default()
                        }),
                    })
                } else {
                    parent_component
                };
                self.insert(
                    child.spec,
                    Some(id),
                    !child.key.component,
                    component,
                    child.key.component,
                )
            };
            if !self.regions[child_id].render.property() {
                if let Some(previous) = self.regions[child_id].context {
                    self.bindings.remove(&previous.parent);
                }
                self.bindings.insert(child.context.parent, child_id);
            }
            self.regions[child_id].context = Some(child.context);
            next.insert(child.key, child_id);
            visit.push((child_id, child.context));
        }
        for child in old.into_values() {
            self.remove(child);
        }
        self.regions[id].children = next;
        for action in actions {
            self.check_bindings(&action.0.bindings(), self.regions[id].component);
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
        self.mailbox.drain(self);
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
                let result = if self.regions[id].render.property() {
                    engine.experiment_redeclare_properties(context, |cx| self.build(id, cx, true))
                } else {
                    engine.experiment_redeclare(context, |cx| self.build(id, cx, true))
                };
                match result {
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
    fn theme(&self) -> plico::Theme {
        self.theme
            .map_or_else(plico::Theme::default, |state| *self.access(state).read())
    }
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        self.mailbox.set_wake(wake);
    }
    fn poll_reload(&mut self) -> bool {
        self.mailbox.drain(self)
    }
    fn snapshot_changed(&self) -> bool {
        !self.dirty.borrow().is_empty() || self.mailbox.pending()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.mailbox.drain(self);
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
        if !self.initialized {
            self.finish_restore();
        }
        self.initialized = true;
        if !self.dirty.get_mut().is_empty() {
            cx.request_rebuild();
        }
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        let result = self.replay(engine);
        if matches!(result, Some(Err(_))) {
            // A failed local declaration can have partially edited the tree.
            // Re-establish every boundary on the next successful full pass.
            self.initialized = false;
        }
        result
    }
}
