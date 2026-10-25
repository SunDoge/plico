use super::*;
use std::sync::Weak;
struct Data<T> {
    value: RefCell<T>,
    listeners: RefCell<Vec<Weak<ScopeWake>>>,
}
/// Explicit parent-to-component input. Ordinary component locals need no wrapper.
/// Reading does not track dependencies; subscribe once to receive change wakes.
pub struct Input<T> {
    data: Rc<Data<T>>,
}
impl<T> Clone for Input<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
        }
    }
}
impl<T> Input<T> {
    pub fn new(value: T) -> Self {
        Self {
            data: Rc::new(Data {
                value: RefCell::new(value),
                listeners: RefCell::default(),
            }),
        }
    }
    pub fn subscribe(&self, ui: &AsyncUi) {
        let wake = Arc::downgrade(&ui.state.wake);
        let mut listeners = self.data.listeners.borrow_mut();
        if !listeners.iter().any(|old| old.ptr_eq(&wake)) {
            listeners.push(wake);
        }
    }
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.data.value.borrow())
    }
}
impl<T: Clone> Input<T> {
    pub fn get(&self) -> T {
        self.data.value.borrow().clone()
    }
}
impl<T: PartialEq> Input<T> {
    pub fn set(&self, value: T) -> bool {
        let mut current = self.data.value.borrow_mut();
        if *current == value {
            return false;
        }
        *current = value;
        drop(current);
        self.data.listeners.borrow_mut().retain(|weak| {
            if let Some(wake) = weak.upgrade() {
                wake.schedule();
                true
            } else {
                false
            }
        });
        true
    }
}
