use slotmap::{Key, SlotMap};
slotmap::new_key_type! {
    /// Opaque generational handle. It is local to this arena, not a persistent state key.
    pub struct Entity;
}
impl Entity {
    /// Opaque encoding for host semantic IDs; never use it as a cross-reload identity.
    pub fn raw(self) -> u64 {
        self.data().as_ffi()
    }
}
/// Thin typed facade over slotmap's tested generational storage.
pub struct Arena<T> {
    slots: SlotMap<Entity, T>,
}
impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            slots: SlotMap::with_key(),
        }
    }
}
impl<T> Arena<T> {
    pub fn insert(&mut self, value: T) -> Entity {
        self.slots.insert(value)
    }
    pub fn get(&self, id: Entity) -> Option<&T> {
        self.slots.get(id)
    }
    pub fn get_mut(&mut self, id: Entity) -> Option<&mut T> {
        self.slots.get_mut(id)
    }
    pub fn remove(&mut self, id: Entity) -> Option<T> {
        self.slots.remove(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.slots.iter()
    }
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.slots.iter_mut()
    }
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}
