//! Only named, serialized values migrate. Owners and callbacks are rebuilt.
use super::*;
use plico_dev::StateSnapshot;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, snafu::Snafu)]
#[snafu(display("region checkpoint failed: {message}"))]
pub struct CheckpointError {
    message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct SlotKey {
    path: Vec<plico::PathKey>,
    key: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    key: SlotKey,
    type_name: String,
    value: StateSnapshot,
}
/// Contains no Rust values, handles or callback pointers. Sensitive values are
/// included if explicitly marked persistent; treat these bytes as app secrets.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    records: Vec<Record>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RestoreStats {
    pub restored: usize,
    pub reset: usize,
    pub unmatched: usize,
}
#[derive(Default)]
pub(super) struct Restore {
    pending: RefCell<HashMap<SlotKey, Record>>,
    stats: Cell<RestoreStats>,
}
pub(super) struct Codec {
    key: u64,
    name: &'static str,
    encode: fn(&dyn Any) -> Result<StateSnapshot, CheckpointError>,
    decode: fn(&StateSnapshot) -> Option<Box<dyn Any>>,
}
impl Codec {
    pub(super) fn of<T: Serialize + DeserializeOwned + 'static>(key: u64) -> Self {
        Self {
            key,
            name: std::any::type_name::<T>(),
            encode: |v| {
                StateSnapshot::encode(v.downcast_ref::<T>().unwrap()).map_err(|e| CheckpointError {
                    message: e.to_string(),
                })
            },
            decode: |v| v.decode::<T>().ok().map(|v| Box::new(v) as Box<dyn Any>),
        }
    }
}
impl Storage {
    pub(super) fn local<T: 'static>(
        &self,
        key: u64,
        init: impl FnOnce() -> T,
        codec: Option<Codec>,
    ) -> usize {
        let mut locals = self.locals.borrow_mut();
        if let Some(&index) = locals.get(&key) {
            let slot = &self.slots[index];
            assert!(
                slot.value.borrow().is::<T>(),
                "local state key reused with another type"
            );
            assert_eq!(
                slot.persistent.is_some(),
                codec.is_some(),
                "state persistence changed for an existing slot"
            );
            return index;
        }
        let value = codec
            .as_ref()
            .and_then(|c| self.restored_value(c))
            .unwrap_or_else(|| Box::new(init()));
        let index = self.slots.len();
        self.slots.push(Box::new(Slot {
            value: RefCell::new(value),
            readers: RefCell::new(vec![]),
            persistent: codec,
        }));
        locals.insert(key, index);
        index
    }
    fn restored_value(&self, codec: &Codec) -> Option<Box<dyn Any>> {
        let record = self.restore.pending.borrow_mut().remove(&SlotKey {
            path: self.path.clone(),
            key: codec.key,
        })?;
        let value = (record.type_name == codec.name)
            .then(|| (codec.decode)(&record.value))
            .flatten();
        let mut stats = self.restore.stats.get();
        if value.is_some() {
            stats.restored += 1;
        } else {
            stats.reset += 1;
        }
        self.restore.stats.set(stats);
        value
    }
}
impl Regions {
    /// Root-owned persisted slot. An explicit key avoids depending on slot order.
    pub fn persisted_state<T: Serialize + DeserializeOwned + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        assert!(!self.initialized, "create root state during construction");
        let index = self.components[self.root_component].storage.local(
            key,
            init,
            Some(Codec::of::<T>(key)),
        );
        State {
            owner: self.owner,
            component: self.root_component,
            index,
            marker: PhantomData,
        }
    }
    pub fn checkpoint(&self) -> Result<Checkpoint, CheckpointError> {
        // Preserve deferred local records when serialized before the first show.
        let mut records: Vec<_> = self.components[self.root_component]
            .storage
            .restore
            .pending
            .borrow()
            .values()
            .cloned()
            .collect();
        for component in self.components.values() {
            let storage = &component.storage;
            for slot in storage.slots.iter() {
                if let Some(codec) = &slot.persistent {
                    records.push(Record {
                        key: SlotKey {
                            path: storage.path.clone(),
                            key: codec.key,
                        },
                        type_name: codec.name.to_owned(),
                        value: (codec.encode)(&**slot.value.borrow())?,
                    });
                }
            }
        }
        Ok(Checkpoint { records })
    }
    /// Restore into a newly built runtime before its first declaration.
    /// Compatible slots survive; incompatible ones keep their own defaults.
    pub fn restore_checkpoint(&mut self, checkpoint: Checkpoint) {
        assert!(!self.initialized, "restore before the first declaration");
        let storage = &self.components[self.root_component].storage;
        *storage.restore.pending.borrow_mut() = checkpoint
            .records
            .into_iter()
            .map(|r| (r.key.clone(), r))
            .collect();
        storage.restore.stats.set(RestoreStats::default());
        for slot in storage.slots.iter() {
            if let Some(codec) = &slot.persistent
                && let Some(value) = storage.restored_value(codec)
            {
                *slot.value.borrow_mut() = value;
            }
        }
    }
    pub fn restore_stats(&self) -> RestoreStats {
        self.components[self.root_component]
            .storage
            .restore
            .stats
            .get()
    }
    pub(super) fn finish_restore(&self) {
        let restore = &self.components[self.root_component].storage.restore;
        let mut stats = restore.stats.get();
        stats.unmatched += restore.pending.borrow().len();
        restore.pending.borrow_mut().clear();
        restore.stats.set(stats);
    }
}
/// A factory is code, not checkpoint data. Every reload rebuilds the graph.
pub trait RegionProgram: 'static {
    fn build() -> Regions;
}
/// Optional adapter for the existing `plico_dev::App` MessagePack protocol.
/// Native `Source<RegionApp<P>>` also forwards region replay and host wakes.
pub struct RegionApp<P: RegionProgram> {
    pub regions: Regions,
    marker: PhantomData<fn() -> P>,
}
impl<P: RegionProgram> Default for RegionApp<P> {
    fn default() -> Self {
        Self {
            regions: P::build(),
            marker: PhantomData,
        }
    }
}
impl<P: RegionProgram> Serialize for RegionApp<P> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.regions
            .checkpoint()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<'de, P: RegionProgram> Deserialize<'de> for RegionApp<P> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let checkpoint = Checkpoint::deserialize(deserializer)?;
        let mut app = Self::default();
        app.regions.restore_checkpoint(checkpoint);
        Ok(app)
    }
}
impl<P: RegionProgram> plico_dev::App for RegionApp<P> {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        View::show(&mut self.regions, cx);
    }
}
impl<P: RegionProgram> View for RegionApp<P> {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        View::show(&mut self.regions, cx);
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        self.regions.replay_regions(engine)
    }
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        self.regions.set_wake_callback(wake);
    }
    fn poll_reload(&mut self) -> bool {
        self.regions.poll_reload()
    }
    fn snapshot_changed(&self) -> bool {
        self.regions.snapshot_changed()
    }
}
