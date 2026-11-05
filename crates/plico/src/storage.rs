//! Value-only persistent component storage. Disk policy belongs to the host.
use crate::{LocalState, PathKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Debug, snafu::Snafu)]
pub enum StorageError {
    #[snafu(display("state storage encoding failed: {source}"))]
    Encode { source: rmp_serde::encode::Error },
    #[snafu(display("state storage decoding failed: {source}"))]
    Decode { source: rmp_serde::decode::Error },
    #[snafu(display("unsupported state storage version {version}"))]
    Version { version: u32 },
    #[snafu(display("duplicate state storage key: {name}"))]
    Duplicate { name: String },
}
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StorageKey {
    pub path: Vec<PathKey>,
    pub scope: u64,
    pub name: String,
}
/// An owned, serializable checkpoint. Arena IDs and Rust objects never escape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateStorageSnapshot {
    version: u32,
    entries: Vec<(StorageKey, Vec<u8>)>,
}
impl Default for StateStorageSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}
impl StateStorageSnapshot {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
trait Stored {
    fn as_any(&self) -> &dyn Any;
    fn encode(&self) -> Result<Vec<u8>, StorageError>;
}
struct Value<T>(Rc<RefCell<T>>);
impl<T: Serialize + 'static> Stored for Value<T> {
    fn as_any(&self) -> &dyn Any {
        &self.0
    }
    fn encode(&self) -> Result<Vec<u8>, StorageError> {
        rmp_serde::to_vec_named(&*self.0.borrow()).map_err(|source| StorageError::Encode { source })
    }
}
enum Slot {
    Saved(Vec<u8>),
    Live(Box<dyn Stored>),
    /// Never encoded into snapshots: transient frame-to-frame host state
    /// (toasts, drag gestures) that checkpoint/restore must not resurrect.
    Transient(Box<dyn Any>),
}
#[derive(Default)]
pub(crate) struct StateStorage {
    slots: HashMap<StorageKey, Slot>,
    changed: Rc<Cell<bool>>,
    resets: Vec<String>,
}
impl StateStorage {
    pub fn get<T: Serialize + DeserializeOwned + 'static>(
        &mut self,
        key: StorageKey,
        init: impl FnOnce() -> T,
    ) -> LocalState<T> {
        if let Some(Slot::Live(value)) = self.slots.get(&key)
            && let Some(value) = value.as_any().downcast_ref::<Rc<RefCell<T>>>()
        {
            return LocalState {
                value: Rc::clone(value),
                changed: Rc::clone(&self.changed),
            };
        }
        let value = match self.slots.remove(&key) {
            Some(Slot::Saved(bytes)) => match rmp_serde::from_slice(&bytes) {
                Ok(value) => value,
                Err(error) => {
                    self.resets.push(format!("{}: {error}", key.name));
                    init()
                }
            },
            Some(Slot::Live(_)) => {
                self.resets.push(format!("{}: Rust type changed", key.name));
                init()
            }
            // A serialized get under a transient key starts fresh; transient
            // values never restore.
            Some(Slot::Transient(_)) | None => init(),
        };
        let value = Rc::new(RefCell::new(value));
        self.slots
            .insert(key, Slot::Live(Box::new(Value(Rc::clone(&value)))));
        LocalState {
            value,
            changed: Rc::clone(&self.changed),
        }
    }
    /// A frame-to-frame value outside snapshots entirely. Toasts and drag
    /// gestures live here: they die with the process and never restore.
    pub fn transient<T: 'static>(
        &mut self,
        key: StorageKey,
        init: impl FnOnce() -> T,
    ) -> LocalState<T> {
        if let Some(Slot::Transient(value)) = self.slots.get(&key)
            && let Some(value) = value.downcast_ref::<Rc<RefCell<T>>>()
        {
            return LocalState {
                value: Rc::clone(value),
                changed: Rc::clone(&self.changed),
            };
        }
        let value = Rc::new(RefCell::new(init()));
        self.slots
            .insert(key, Slot::Transient(Box::new(Rc::clone(&value))));
        LocalState {
            value,
            changed: Rc::clone(&self.changed),
        }
    }
    pub fn take_changed(&self) -> bool {
        self.changed.replace(false)
    }
    pub fn snapshot(&self) -> Result<StateStorageSnapshot, StorageError> {
        let entries = self
            .slots
            .iter()
            .filter_map(|(key, slot)| {
                let bytes = match slot {
                    Slot::Saved(bytes) => bytes.clone(),
                    Slot::Live(value) => value.encode().ok()?,
                    Slot::Transient(_) => return None,
                };
                Some((key.clone(), bytes))
            })
            .collect::<Vec<_>>();
        Ok(StateStorageSnapshot {
            version: 1,
            entries,
        })
    }
    pub fn restore(&mut self, snapshot: StateStorageSnapshot) -> Result<(), StorageError> {
        if snapshot.version != 1 {
            return Err(StorageError::Version {
                version: snapshot.version,
            });
        }
        let mut slots = HashMap::with_capacity(snapshot.entries.len());
        for (key, bytes) in snapshot.entries {
            if slots.contains_key(&key) {
                return Err(StorageError::Duplicate { name: key.name });
            }
            slots.insert(key, Slot::Saved(bytes));
        }
        self.slots = slots;
        self.resets.clear();
        self.changed.set(true);
        Ok(())
    }
    pub fn remove(&mut self, key: &StorageKey) -> bool {
        let removed = self.slots.remove(key).is_some();
        if removed {
            self.changed.set(true);
        }
        removed
    }
    pub fn clear_scope(&mut self, path: &[PathKey], scope: u64) {
        let old = self.slots.len();
        self.slots
            .retain(|key, _| key.path != path || key.scope != scope);
        if self.slots.len() != old {
            self.changed.set(true);
        }
    }
    pub fn clear(&mut self) {
        if !self.slots.is_empty() {
            self.slots.clear();
            self.changed.set(true);
        }
    }
    pub fn take_resets(&mut self) -> Vec<String> {
        std::mem::take(&mut self.resets)
    }
}
