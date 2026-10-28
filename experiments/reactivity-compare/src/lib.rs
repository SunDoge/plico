//! Isolated explicit-dependency experiment; not a Plico public API.
pub mod engine_bridge;
use slotmap::{SlotMap, new_key_type};
use std::{
    any::{Any, TypeId},
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

new_key_type! { struct StateKey; pub struct RegionId; pub struct HandlerId; pub struct ComponentId; }

pub struct State<T> {
    key: StateKey,
    owner: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for State<T> {}
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Only exists during dispatch. Nothing stores this borrow across calls.
pub struct Access<'a, T> {
    runtime: &'a Runtime,
    state: State<T>,
}
impl<T: 'static> Access<'_, T> {
    pub fn handle(&self) -> State<T> {
        self.state
    }
    pub fn read(&self) -> Ref<'_, T> {
        Ref::map(
            self.runtime.states[self.state.key].value.borrow(),
            |value| value.downcast_ref().unwrap(),
        )
    }
    /// Mutation conservatively invalidates all explicit dependents, even if unchanged.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        assert!(
            !self.runtime.rendering.get(),
            "state writes inside watch are forbidden"
        );
        let slot = &self.runtime.states[self.state.key];
        let mut value = slot.value.borrow_mut();
        let result = f(value.downcast_mut().unwrap());
        drop(value);
        let mut queue = self.runtime.queue.borrow_mut();
        for &id in &slot.readers {
            queue.push_back(id);
        }
        result
    }
}
struct Slot {
    value: RefCell<Box<dyn Any>>,
    readers: Vec<RegionId>,
}
#[derive(Clone, Copy)]
struct LocalState {
    key: StateKey,
    owner: usize,
    ty: TypeId,
}
#[derive(Default)]
struct Component {
    states: HashMap<u64, LocalState>,
    roots: Vec<RegionId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    Text(usize),
    Child(u64),
}
#[derive(Clone, Debug)]
pub enum RegionItem {
    Text { key: usize, text: String },
    Child(RegionId),
}
struct ChildRequest {
    key: u64,
    spec: WatchSpec,
}
/// Owned description of a callback and its explicit state handles.
pub struct WatchSpec {
    deps: Vec<(StateKey, usize)>,
    replay: Box<dyn Replay>,
}
#[derive(Default)]
pub struct RegionUi {
    pub text: Vec<String>,
    locals: HashMap<u64, LocalState>,
    children: Vec<ChildRequest>,
    seen_children: HashSet<u64>,
    order: Vec<Content>,
}
impl RegionUi {
    pub fn text(&mut self, value: impl Into<String>) {
        if !self.children.is_empty() {
            self.order.push(Content::Text(self.text.len()));
        }
        self.text.push(value.into());
    }
    /// Resolve a state declared once by the owning component's mount function.
    /// Looking up a handle does not subscribe; watch's explicit tuple does.
    pub fn state<T: 'static>(&self, key: u64) -> State<T> {
        let slot = self.locals.get(&key).expect("component state not declared");
        assert_eq!(slot.ty, TypeId::of::<T>(), "component state type changed");
        State {
            key: slot.key,
            owner: slot.owner,
            marker: PhantomData,
        }
    }
    /// Key is scoped to the current parent. Callback remains a function pointer.
    pub fn watch<D: WatchDependencies>(&mut self, key: u64, deps: D, callback: D::Callback) {
        assert!(self.seen_children.insert(key), "duplicate child region key");
        if self.children.is_empty() {
            self.order.extend((0..self.text.len()).map(Content::Text));
        }
        self.order.push(Content::Child(key));
        self.children.push(ChildRequest {
            key,
            spec: deps.describe(callback),
        });
    }
}
trait Replay {
    fn run(&self, runtime: &Runtime, ui: &mut RegionUi);
}
struct One<T> {
    state: State<T>,
    run: fn(&mut RegionUi, Access<'_, T>),
}
impl<T: 'static> Replay for One<T> {
    fn run(&self, runtime: &Runtime, ui: &mut RegionUi) {
        (self.run)(ui, runtime.access(self.state));
    }
}
struct Two<A, B> {
    a: State<A>,
    b: State<B>,
    run: fn(&mut RegionUi, Access<'_, A>, Access<'_, B>),
}
impl<A: 'static, B: 'static> Replay for Two<A, B> {
    fn run(&self, runtime: &Runtime, ui: &mut RegionUi) {
        (self.run)(ui, runtime.access(self.a), runtime.access(self.b));
    }
}
trait Event {
    fn run(&self, runtime: &Runtime);
}
struct EventTwo<A, B> {
    a: State<A>,
    b: State<B>,
    run: fn(Access<'_, A>, Access<'_, B>),
}
impl<A: 'static, B: 'static> Event for EventTwo<A, B> {
    fn run(&self, runtime: &Runtime) {
        (self.run)(runtime.access(self.a), runtime.access(self.b));
    }
}
struct EventReadWrite<A, B> {
    a: State<A>,
    b: State<B>,
    run: fn(&A, &mut B),
}
impl<A: 'static, B: 'static> Event for EventReadWrite<A, B> {
    fn run(&self, runtime: &Runtime) {
        let a = runtime.access(self.a);
        let b = runtime.access(self.b);
        let value = a.read();
        b.update(|target| (self.run)(&value, target));
    }
}
struct EventOne<T> {
    state: State<T>,
    run: fn(Access<'_, T>),
}
impl<T: 'static> Event for EventOne<T> {
    fn run(&self, runtime: &Runtime) {
        (self.run)(runtime.access(self.state));
    }
}
struct Region {
    replay: Box<dyn Replay>,
    deps: Vec<StateKey>,
    output: Vec<String>,
    parent: Option<RegionId>,
    children: Vec<RegionId>,
    queued: bool,
    component: Option<ComponentId>,
    child_keys: HashMap<u64, RegionId>,
    order: Vec<Content>,
    executions: usize,
}
struct Handler {
    event: Box<dyn Event>,
    deps: Vec<StateKey>,
    owner: RegionId,
}
/// A tuple selects the concrete function-pointer type, preserving closure coercion.
pub trait WatchDependencies {
    type Callback;
    fn register(self, runtime: &mut Runtime, callback: Self::Callback) -> RegionId;
    fn describe(self, callback: Self::Callback) -> WatchSpec;
}
impl<T: 'static> WatchDependencies for (State<T>,) {
    type Callback = for<'a, 'b> fn(&'a mut RegionUi, Access<'b, T>);
    fn register(self, runtime: &mut Runtime, callback: Self::Callback) -> RegionId {
        runtime.watch1(self, callback)
    }
    fn describe(self, run: Self::Callback) -> WatchSpec {
        WatchSpec {
            deps: vec![(self.0.key, self.0.owner)],
            replay: Box::new(One { state: self.0, run }),
        }
    }
}
impl<A: 'static, B: 'static> WatchDependencies for (State<A>, State<B>) {
    type Callback = for<'a, 'b, 'c> fn(&'a mut RegionUi, Access<'b, A>, Access<'c, B>);
    fn register(self, runtime: &mut Runtime, callback: Self::Callback) -> RegionId {
        runtime.watch2(self, callback)
    }
    fn describe(self, run: Self::Callback) -> WatchSpec {
        WatchSpec {
            deps: vec![(self.0.key, self.0.owner), (self.1.key, self.1.owner)],
            replay: Box::new(Two {
                a: self.0,
                b: self.1,
                run,
            }),
        }
    }
}

pub struct Runtime {
    owner: usize,
    states: SlotMap<StateKey, Slot>,
    regions: SlotMap<RegionId, Region>,
    handlers: SlotMap<HandlerId, Handler>,
    queue: RefCell<VecDeque<RegionId>>,
    rendering: Cell<bool>,
    components: SlotMap<ComponentId, Component>,
}
impl Default for Runtime {
    fn default() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let owner = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("runtime IDs exhausted");
        Self {
            owner,
            states: SlotMap::with_key(),
            regions: SlotMap::with_key(),
            handlers: SlotMap::with_key(),
            queue: RefCell::new(VecDeque::new()),
            rendering: Cell::new(false),
            components: SlotMap::with_key(),
        }
    }
}
impl Runtime {
    fn check<T>(&self, state: State<T>) {
        assert_eq!(state.owner, self.owner, "state belongs to another runtime");
    }
    pub fn state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        State {
            owner: self.owner,
            key: self.states.insert(Slot {
                value: RefCell::new(Box::new(init())),
                readers: Vec::new(),
            }),
            marker: PhantomData,
        }
    }
    /// Diagnostic live counts: states, regions, handlers, components, dependency edges.
    pub fn live_counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.states.len(),
            self.regions.len(),
            self.handlers.len(),
            self.components.len(),
            self.states.values().map(|s| s.readers.len()).sum(),
        )
    }
    pub fn component(&mut self) -> ComponentId {
        self.components.insert(Component::default())
    }
    pub fn state_in<T: 'static>(
        &mut self,
        owner: ComponentId,
        key: u64,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        if let Some(slot) = self.components[owner].states.get(&key) {
            assert_eq!(slot.ty, TypeId::of::<T>(), "component state type changed");
            return State {
                key: slot.key,
                owner: slot.owner,
                marker: PhantomData,
            };
        }
        let state = self.state(init);
        self.components[owner].states.insert(
            key,
            LocalState {
                key: state.key,
                owner: self.owner,
                ty: TypeId::of::<T>(),
            },
        );
        state
    }
    pub fn watch_in<D: WatchDependencies>(
        &mut self,
        owner: ComponentId,
        deps: D,
        callback: D::Callback,
    ) -> RegionId {
        assert!(self.components.contains_key(owner));
        let id = self.watch(deps, callback);
        self.regions[id].component = Some(owner);
        self.components[owner].roots.push(id);
        id
    }
    pub fn unmount_component(&mut self, owner: ComponentId) {
        let Some(component) = self.components.remove(owner) else {
            return;
        };
        for root in component.roots {
            self.unmount(root);
        }
        for slot in component.states.into_values() {
            self.dispose_key(slot.key);
        }
    }
    pub fn state_alive<T>(&self, state: State<T>) -> bool {
        state.owner == self.owner && self.states.contains_key(state.key)
    }
    pub fn child(&self, parent: RegionId, key: u64) -> Option<RegionId> {
        self.regions.get(parent)?.child_keys.get(&key).copied()
    }
    pub fn executions(&self, id: RegionId) -> Option<usize> {
        self.regions.get(id).map(|r| r.executions)
    }
    pub fn contents(&self, id: RegionId) -> Option<Vec<RegionItem>> {
        let region = self.regions.get(id)?;
        if region.order.is_empty() {
            return Some(
                region
                    .output
                    .iter()
                    .enumerate()
                    .map(|(key, text)| RegionItem::Text {
                        key,
                        text: text.clone(),
                    })
                    .collect(),
            );
        }
        Some(
            region
                .order
                .iter()
                .map(|item| match *item {
                    Content::Text(key) => RegionItem::Text {
                        key,
                        text: region.output[key].clone(),
                    },
                    Content::Child(key) => RegionItem::Child(region.child_keys[&key]),
                })
                .collect(),
        )
    }
    /// Flatten logical text output, preserving text/region interleaving.
    pub fn snapshot(&self, id: RegionId) -> Vec<String> {
        fn walk(runtime: &Runtime, id: RegionId, out: &mut Vec<String>) {
            let Some(region) = runtime.regions.get(id) else {
                return;
            };
            if region.order.is_empty() {
                out.extend(region.output.iter().cloned());
                return;
            }
            for content in &region.order {
                match *content {
                    Content::Text(i) => out.push(region.output[i].clone()),
                    Content::Child(key) => walk(runtime, region.child_keys[&key], out),
                }
            }
        }
        let mut out = Vec::new();
        walk(self, id, &mut out);
        out
    }
    pub fn access<T: 'static>(&self, state: State<T>) -> Access<'_, T> {
        {
            self.check(state);
            Access {
                runtime: self,
                state,
            }
        }
    }
    /// Stored callbacks cannot capture local variables.
    /// ```compile_fail
    /// use plico_reactivity_compare::Runtime;
    /// let mut rt = Runtime::default();
    /// let count = rt.state(|| 0);
    /// let external = String::from("implicit capture");
    /// rt.watch((count,), |cx, _| cx.text(external.clone()));
    /// ```
    pub fn watch<D: WatchDependencies>(&mut self, deps: D, callback: D::Callback) -> RegionId {
        deps.register(self, callback)
    }
    fn watch1<T: 'static>(
        &mut self,
        deps: (State<T>,),
        run: fn(&mut RegionUi, Access<'_, T>),
    ) -> RegionId {
        self.check(deps.0);
        self.mount(vec![deps.0.key], Box::new(One { state: deps.0, run }))
    }
    pub fn watch2<A: 'static, B: 'static>(
        &mut self,
        deps: (State<A>, State<B>),
        run: fn(&mut RegionUi, Access<'_, A>, Access<'_, B>),
    ) -> RegionId {
        self.check(deps.0);
        self.check(deps.1);
        self.mount(
            vec![deps.0.key, deps.1.key],
            Box::new(Two {
                a: deps.0,
                b: deps.1,
                run,
            }),
        )
    }
    fn mount(&mut self, mut deps: Vec<StateKey>, replay: Box<dyn Replay>) -> RegionId {
        deps.dedup();
        let id = self.regions.insert(Region {
            replay,
            deps,
            output: Vec::new(),
            parent: None,
            children: Vec::new(),
            queued: false,
            component: None,
            child_keys: HashMap::new(),
            order: Vec::new(),
            executions: 0,
        });
        for &key in &self.regions[id].deps {
            self.states[key].readers.push(id);
        }
        self.queue.get_mut().push_back(id);
        id
    }
    /// ```compile_fail
    /// use plico_reactivity_compare::Runtime;
    /// let mut rt = Runtime::default();
    /// let count = rt.state(|| 0);
    /// let region = rt.watch((count,), |_, _| {});
    /// let external = 2;
    /// rt.on_click(region, (count,), |count| count.update(|c| *c += external));
    /// ```
    pub fn on_click<T: 'static>(
        &mut self,
        owner: RegionId,
        deps: (State<T>,),
        run: fn(Access<'_, T>),
    ) -> HandlerId {
        self.check(deps.0);
        assert!(self.regions.contains_key(owner));
        assert!(self.states.contains_key(deps.0.key));
        self.handlers.insert(Handler {
            owner,
            deps: vec![deps.0.key],
            event: Box::new(EventOne { state: deps.0, run }),
        })
    }
    pub fn on_click2<A: 'static, B: 'static>(
        &mut self,
        owner: RegionId,
        deps: (State<A>, State<B>),
        run: fn(Access<'_, A>, Access<'_, B>),
    ) -> HandlerId {
        self.check(deps.0);
        assert!(self.regions.contains_key(owner));
        assert!(self.states.contains_key(deps.0.key));
        self.check(deps.1);
        assert!(self.states.contains_key(deps.1.key));
        self.handlers.insert(Handler {
            owner,
            deps: vec![deps.0.key, deps.1.key],
            event: Box::new(EventTwo {
                a: deps.0,
                b: deps.1,
                run,
            }),
        })
    }
    /// Read/write permissions can also be expressed with ordinary Rust references.
    /// ```compile_fail
    /// use plico_reactivity_compare::Runtime;
    /// let mut rt = Runtime::default();
    /// let a = rt.state(|| 1); let b = rt.state(|| 0);
    /// let owner = rt.watch((a,), |_, _| {});
    /// rt.on_click_rw(owner, (a, b), |read, write| { *read += 1; *write += 1; });
    /// ```
    pub fn on_click_rw<A: 'static, B: 'static>(
        &mut self,
        owner: RegionId,
        deps: (State<A>, State<B>),
        run: fn(&A, &mut B),
    ) -> HandlerId {
        self.check(deps.0);
        self.check(deps.1);
        assert!(self.regions.contains_key(owner));
        assert!(self.states.contains_key(deps.0.key));
        assert!(self.states.contains_key(deps.1.key));
        assert_ne!(
            deps.0.key, deps.1.key,
            "conflicting read/write dependencies"
        );
        self.handlers.insert(Handler {
            owner,
            deps: vec![deps.0.key, deps.1.key],
            event: Box::new(EventReadWrite {
                a: deps.0,
                b: deps.1,
                run,
            }),
        })
    }
    pub fn click(&self, id: HandlerId) -> bool {
        let Some(handler) = self.handlers.get(id) else {
            return false;
        };
        if !self.regions.contains_key(handler.owner)
            || !handler.deps.iter().all(|k| self.states.contains_key(*k))
        {
            return false;
        }
        handler.event.run(self);
        true
    }
    /// Explicit lifetime tree for the PoC. Not inferred from Rust scopes.
    pub fn adopt(&mut self, parent: RegionId, child: RegionId) {
        assert_ne!(parent, child);
        assert!(self.regions[child].parent.is_none());
        let mut cursor = Some(parent);
        while let Some(id) = cursor {
            assert_ne!(id, child, "region ownership cycle");
            cursor = self.regions[id].parent;
        }
        self.regions[child].parent = Some(parent);
        self.regions[parent].children.push(child);
    }
    pub fn unmount(&mut self, id: RegionId) {
        let Some(region) = self.regions.remove(id) else {
            return;
        };
        for child in region.children {
            self.unmount(child);
        }
        if let Some(parent) = region.parent.and_then(|p| self.regions.get_mut(p)) {
            parent.children.retain(|c| *c != id);
            parent.child_keys.retain(|_, child| *child != id);
            parent.order.retain(|content| match content {
                Content::Text(_) => true,
                Content::Child(key) => parent.child_keys.contains_key(key),
            });
        }
        if let Some(component) = region.component.and_then(|c| self.components.get_mut(c)) {
            component.roots.retain(|root| *root != id);
        }
        for key in region.deps {
            if let Some(slot) = self.states.get_mut(key) {
                slot.readers.retain(|r| *r != id);
            }
        }
        self.handlers.retain(|_, h| h.owner != id);
    }
    pub fn dispose_state<T>(&mut self, state: State<T>) {
        self.check(state);
        self.dispose_key(state.key);
    }
    fn dispose_key(&mut self, key: StateKey) {
        for component in self.components.values_mut() {
            component.states.retain(|_, slot| slot.key != key);
        }
        self.handlers.retain(|_, h| !h.deps.contains(&key));
        if let Some(slot) = self.states.remove(key) {
            for id in slot.readers {
                self.unmount(id);
            }
        }
    }
    pub fn output(&self, id: RegionId) -> Option<&[String]> {
        self.regions.get(id).map(|r| r.output.as_slice())
    }
    fn check_spec(&self, spec: &WatchSpec) {
        for &(key, owner) in &spec.deps {
            assert_eq!(owner, self.owner, "state belongs to another runtime");
            assert!(self.states.contains_key(key), "state was disposed");
        }
    }
    fn reconcile_children(&mut self, parent: RegionId, requests: Vec<ChildRequest>) {
        let old_keys = self.regions[parent].child_keys.clone();
        let declared: HashSet<_> = old_keys.values().copied().collect();
        let manual: Vec<_> = self.regions[parent]
            .children
            .iter()
            .copied()
            .filter(|id| !declared.contains(id))
            .collect();
        let component = self.regions[parent].component;
        let mut children = Vec::with_capacity(requests.len());
        let mut keys = HashMap::with_capacity(requests.len());
        for ChildRequest { key, spec } in requests {
            self.check_spec(&spec);
            let mut deps: Vec<_> = spec.deps.iter().map(|d| d.0).collect();
            let mut seen = HashSet::new();
            deps.retain(|d| seen.insert(*d));
            let child = if let Some(&child) = old_keys.get(&key) {
                let old_deps = std::mem::replace(&mut self.regions[child].deps, deps);
                for key in old_deps {
                    self.states[key].readers.retain(|r| *r != child);
                }
                for &key in &self.regions[child].deps {
                    self.states[key].readers.push(child);
                }
                self.regions[child].replay = spec.replay;
                // A parent replay refreshes its retained children as well. It
                // may have changed their callback, arguments or inherited context.
                if !self.regions[child].queued {
                    self.queue.get_mut().push_back(child);
                }
                child
            } else {
                let child = self.mount(deps, spec.replay);
                self.regions[child].parent = Some(parent);
                self.regions[child].component = component;
                child
            };
            keys.insert(key, child);
            children.push(child);
        }
        for (key, child) in old_keys {
            if !keys.contains_key(&key) {
                // Parent lists are replaced below; avoid rescanning them for each removal.
                self.regions[child].parent = None;
                self.unmount(child);
            }
        }
        children.extend(manual);
        self.regions[parent].children = children;
        self.regions[parent].child_keys = keys;
    }
    fn depth(&self, mut id: RegionId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.regions.get(id).and_then(|r| r.parent) {
            depth += 1;
            id = parent;
        }
        depth
    }
    /// Parents replay before children. Reconciliation cancels dead work by key.
    pub fn flush(&mut self) -> Vec<RegionId> {
        let mut changed = Vec::new();
        loop {
            let mut scheduled = Vec::new();
            for id in self.queue.get_mut().drain(..) {
                if let Some(region) = self.regions.get_mut(id)
                    && !region.queued
                {
                    region.queued = true;
                    scheduled.push(id);
                }
            }
            if scheduled.is_empty() {
                break;
            }
            scheduled.sort_by_key(|id| self.depth(*id));
            for id in scheduled {
                let Some(region) = self.regions.get(id) else {
                    continue;
                };
                let locals = region
                    .component
                    .and_then(|c| self.components.get(c))
                    .map(|c| c.states.clone())
                    .unwrap_or_default();
                let mut ui = RegionUi {
                    locals,
                    ..RegionUi::default()
                };
                struct Guard<'a>(&'a Cell<bool>);
                impl Drop for Guard<'_> {
                    fn drop(&mut self) {
                        self.0.set(false);
                    }
                }
                self.rendering.set(true);
                let guard = Guard(&self.rendering);
                region.replay.run(self, &mut ui);
                drop(guard);
                let old_children = self.regions[id].children.clone();
                if !ui.children.is_empty() || !self.regions[id].child_keys.is_empty() {
                    self.reconcile_children(id, ui.children);
                }
                let region = &mut self.regions[id];
                region.queued = false;
                region.executions += 1;
                if region.output != ui.text
                    || region.order != ui.order
                    || region.children != old_children
                {
                    region.output = ui.text;
                    region.order = ui.order;
                    changed.push(id);
                }
            }
        }
        changed.retain(|id| self.regions.contains_key(*id));
        changed
    }
}
/// Explicit cx parameter, explicit state tuple; callbacks must coerce to fn pointers.
#[macro_export]
macro_rules! watch {
    ($rt:expr; $deps:expr, $callback:expr) => {
        $rt.watch($deps, $callback)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[should_panic(expected = "state belongs to another runtime")]
    fn rejects_foreign_state_handles() {
        let mut a = Runtime::default();
        let b = Runtime::default();
        let state = a.state(|| 0);
        let _ = b.access(state);
    }
    #[test]
    #[should_panic(expected = "state writes inside watch are forbidden")]
    fn watch_cannot_write_state() {
        let mut rt = Runtime::default();
        let s = rt.state(|| 0);
        rt.watch((s,), |_, s| s.update(|v| *v += 1));
        rt.flush();
    }
    #[test]
    fn ordinary_watch_infers_one_and_two_dependency_callbacks() {
        let mut rt = Runtime::default();
        let a = rt.state(|| 1);
        let b = rt.state(|| 2);
        let one = rt.watch((a,), |cx, a| cx.text(a.read().to_string()));
        let two = rt.watch((a, b), |cx, a, b| {
            cx.text((*a.read() + *b.read()).to_string())
        });
        assert_eq!(rt.flush().len(), 2);
        assert_eq!(rt.output(one).unwrap(), ["1"]);
        assert_eq!(rt.output(two).unwrap(), ["3"]);
    }
    #[test]
    fn reference_callback_gets_read_and_write_permissions() {
        let mut rt = Runtime::default();
        let step = rt.state(|| 2);
        let count = rt.state(|| 0);
        let owner = rt.watch((count,), |cx, c| cx.text(c.read().to_string()));
        rt.flush();
        let event = rt.on_click_rw(owner, (step, count), |step, count| *count += *step);
        assert!(rt.click(event));
        assert_eq!(rt.flush(), [owner]);
        assert_eq!(rt.output(owner).unwrap(), ["2"]);
    }
    #[test]
    #[should_panic(expected = "conflicting read/write dependencies")]
    fn conflicting_reference_permissions_are_rejected_at_registration() {
        let mut rt = Runtime::default();
        let s = rt.state(|| 0);
        let owner = rt.watch((s,), |_, _| {});
        rt.on_click_rw(owner, (s, s), |a, b| *b += *a);
    }
    #[test]
    fn read_one_state_while_updating_another() {
        let mut rt = Runtime::default();
        let a = rt.state(|| 2);
        let b = rt.state(|| 3);
        let owner = rt.watch((a,), |_, _| {});
        let event = rt.on_click2(owner, (a, b), |a, b| {
            let a = a.read();
            b.update(|v| *v += *a);
        });
        assert!(rt.click(event));
        assert_eq!(*rt.access(b).read(), 5);
    }
    #[test]
    fn two_argument_event_aliases_are_short_lived() {
        let mut rt = Runtime::default();
        let s = rt.state(|| 1);
        let owner = rt.watch((s,), |_, _| {});
        let e = rt.on_click2(owner, (s, s), |a, b| {
            let value = *a.read();
            b.update(|v| *v += value);
        });
        assert!(rt.click(e));
        assert_eq!(*rt.access(s).read(), 2);
    }
    #[test]
    fn batching_conditional_reads_and_static_dependencies() {
        let mut rt = Runtime::default();
        let count = rt.state(|| 0);
        let enabled = rt.state(|| true);
        let id = rt.watch2((count, enabled), |cx, c, enabled| {
            if *enabled.read() {
                cx.text(c.read().to_string());
            }
        });
        assert_eq!(rt.flush(), [id]);
        rt.access(count).update(|v| *v += 1);
        rt.access(count).update(|v| *v += 1);
        assert_eq!(rt.flush(), [id]);
        assert_eq!(rt.output(id).unwrap(), ["2"]);
        rt.access(enabled).update(|v| *v = false);
        assert_eq!(rt.flush(), [id]);
        rt.access(count).update(|v| *v += 1);
        assert!(rt.flush().is_empty()); // Replayed, but output stayed empty.
    }
    #[test]
    fn unmount_cancels_pending_work_and_old_events() {
        let mut rt = Runtime::default();
        let count = rt.state(|| 0);
        let parent = rt.watch((count,), |cx, c| cx.text(c.read().to_string()));
        let child = rt.watch((count,), |cx, c| cx.text(c.read().to_string()));
        rt.adopt(parent, child);
        let event = rt.on_click(child, (count,), |c| c.update(|v| *v += 1));
        assert!(rt.click(event));
        rt.unmount(parent);
        assert!(!rt.click(event));
        assert!(rt.flush().is_empty());
        assert!(rt.states[count.key].readers.is_empty());
        let replacement = rt.watch((count,), |_, _| {});
        assert_ne!(replacement, child);
        assert!(!rt.click(event));
    }
    #[test]
    fn removed_states_cannot_dispatch_into_reused_slots() {
        let mut rt = Runtime::default();
        let s = rt.state(|| 0);
        let id = rt.watch((s,), |_, _| {});
        let event = rt.on_click(id, (s,), |s| s.update(|v| *v += 1));
        rt.dispose_state(s);
        let replacement = rt.state(|| 10);
        assert!(!rt.click(event));
        assert_eq!(*rt.access(replacement).read(), 10);
        assert!(rt.flush().is_empty());
    }
}
