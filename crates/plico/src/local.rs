use crate::Entity;
use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
    collections::HashMap,
    panic::Location,
    rc::Rc,
};

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct LocalKey {
    parent: Entity,
    scope: u64,
    namespace: u8,
    file: &'static str,
    line: u32,
    column: u32,
    name: Option<&'static str>,
}
struct Slot {
    value: Box<dyn Any>,
    epoch: u64,
}
#[derive(Default)]
pub(crate) struct LocalStore {
    slots: HashMap<LocalKey, Slot>,
    changed: Rc<Cell<bool>>,
    accessed: RefCell<Vec<LocalKey>>,
}
impl LocalStore {
    fn channel_key(parent: Entity, scope: u64, namespace: u8, name: &'static str) -> LocalKey {
        LocalKey {
            parent,
            scope,
            namespace,
            file: "",
            line: 0,
            column: 0,
            name: Some(name),
        }
    }
    pub fn get<T: 'static>(
        &mut self,
        parent: Entity,
        scope: u64,
        site: &'static Location<'static>,
        epoch: u64,
        init: impl FnOnce() -> T,
    ) -> LocalState<T> {
        let key = LocalKey {
            parent,
            scope,
            namespace: 0,
            file: site.file(),
            line: site.line(),
            column: site.column(),
            name: None,
        };
        self.get_slot(key, epoch, init)
    }
    pub fn get_channel<T: 'static>(
        &mut self,
        parent: Entity,
        scope: u64,
        namespace: u8,
        name: &'static str,
        epoch: u64,
        init: impl FnOnce() -> T,
    ) -> LocalState<T> {
        let key = Self::channel_key(parent, scope, namespace, name);
        self.get_slot(key, epoch, init)
    }
    pub fn channel<T: 'static>(
        &self,
        parent: Entity,
        scope: u64,
        namespace: u8,
        name: &'static str,
    ) -> Option<LocalState<T>> {
        let key = Self::channel_key(parent, scope, namespace, name);
        self.accessed.borrow_mut().push(key.clone());
        let value = self
            .slots
            .get(&key)?
            .value
            .downcast_ref::<Rc<RefCell<T>>>()?;
        Some(LocalState {
            value: Rc::clone(value),
            changed: Rc::clone(&self.changed),
        })
    }
    fn get_slot<T: 'static>(
        &mut self,
        key: LocalKey,
        epoch: u64,
        init: impl FnOnce() -> T,
    ) -> LocalState<T> {
        self.accessed.borrow_mut().push(key.clone());
        let slot = self.slots.entry(key).or_insert_with(|| Slot {
            value: Box::new(Rc::new(RefCell::new(init()))),
            epoch,
        });
        slot.epoch = epoch;
        let value = slot
            .value
            .downcast_ref::<Rc<RefCell<T>>>()
            .expect("local state type changed at the same call site");
        LocalState {
            value: Rc::clone(value),
            changed: Rc::clone(&self.changed),
        }
    }
    pub fn take_changed(&self) -> bool {
        self.changed.replace(false)
    }
    pub fn access_mark(&self) -> usize {
        self.accessed.borrow().len()
    }
    pub fn accesses_since(&self, mark: usize) -> Vec<LocalKey> {
        self.accessed.borrow()[mark..].to_vec()
    }
    pub fn touch(&mut self, keys: &[LocalKey], epoch: u64) {
        for key in keys {
            if let Some(slot) = self.slots.get_mut(key) {
                slot.epoch = epoch;
                self.accessed.borrow_mut().push(key.clone());
            }
        }
    }
    pub fn clear_accesses(&mut self) {
        self.accessed.get_mut().clear();
    }
    pub fn retain(&mut self, mut keep: impl FnMut(Entity, u64) -> bool) {
        self.slots.retain(|key, slot| keep(key.parent, slot.epoch));
    }
}

/// A UI-thread-local handle, not a subscription or a borrow of `UiContext`.
/// Keep handles within the declaration. Storage is released when its component
/// stops being declared; outstanding handles extend the value's lifetime only.
pub struct LocalState<T> {
    pub(crate) value: Rc<RefCell<T>>,
    pub(crate) changed: Rc<Cell<bool>>,
}
impl<T> Clone for LocalState<T> {
    fn clone(&self) -> Self {
        Self {
            value: Rc::clone(&self.value),
            changed: Rc::clone(&self.changed),
        }
    }
}
impl<T> LocalState<T> {
    /// Borrow without cloning the value. Drop the guard before calling `set`.
    pub fn read(&self) -> Ref<'_, T> {
        self.value.borrow()
    }
    /// Replace only when different, requesting a settle pass automatically.
    pub fn set(&self, value: T)
    where
        T: PartialEq,
    {
        let mut current = self.value.borrow_mut();
        if *current != value {
            *current = value;
            self.changed.set(true);
        }
    }
    /// Retain bookkeeping that has already been incorporated into this
    /// declaration's output, without requesting another settle pass.
    ///
    /// This is deliberately crate-private: widgets may use it for values such
    /// as the scroll observation or anchor they just consumed, while ordinary
    /// component state must use [`Self::set`] so changed output is rebuilt.
    pub(crate) fn commit(&self, value: T)
    where
        T: PartialEq,
    {
        let mut current = self.value.borrow_mut();
        if *current != value {
            *current = value;
        }
    }
    /// Mutate a temporary copy and commit only a changed result. No clone on reads.
    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: Clone + PartialEq,
    {
        let mut next = self.value.borrow().clone();
        f(&mut next);
        self.set(next);
    }
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.borrow().clone()
    }
}
