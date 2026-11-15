use crate::Entity;
use std::{any::Any, collections::HashMap};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct MemoKey {
    pub parent: Entity,
    pub scope: u64,
}

pub(crate) struct MemoSlot {
    pub dependency: Box<dyn Any>,
    pub roots: Vec<(Entity, Entity)>,
    pub epoch: u64,
    pub context_revision: u64,
    pub animated: bool,
    pub locals: Vec<crate::local::LocalKey>,
    pub wake_at: Option<std::time::Duration>,
}

pub(crate) struct MemoHit {
    pub roots: Vec<(Entity, Entity)>,
    pub locals: Vec<crate::local::LocalKey>,
    pub wake_at: Option<std::time::Duration>,
}

#[derive(Default)]
pub(crate) struct MemoStore {
    slots: HashMap<MemoKey, MemoSlot>,
}

impl MemoStore {
    pub fn hit<D: PartialEq + 'static>(
        &self,
        key: MemoKey,
        dependency: &D,
        context_revision: u64,
    ) -> Option<MemoHit> {
        let slot = self.slots.get(&key)?;
        (!slot.animated
            && slot.context_revision == context_revision
            && slot.dependency.downcast_ref::<D>() == Some(dependency))
        .then(|| MemoHit {
            roots: slot.roots.clone(),
            locals: slot.locals.clone(),
            wake_at: slot.wake_at,
        })
    }

    pub fn insert(&mut self, key: MemoKey, slot: MemoSlot) {
        self.slots.insert(key, slot);
    }

    pub fn touch(&mut self, key: MemoKey, epoch: u64) {
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.epoch = epoch;
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(MemoKey, &MemoSlot) -> bool) {
        self.slots.retain(|key, slot| keep(*key, slot));
    }

    pub fn wake_deadline(&self) -> Option<std::time::Duration> {
        self.slots.values().filter_map(|slot| slot.wake_at).min()
    }
}
