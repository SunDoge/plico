//! Real Engine integration for text/column regions. No window or GPU required.
use crate::{RegionId, RegionItem, Runtime};
use plico::{Color, Entity, ExperimentContext, FrameStats, Key, UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{Source, View};
use slotmap::Key as _;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

pub fn region_key(id: RegionId) -> Key {
    Key::Number(id.data().as_ffi())
}
fn snapshot(runtime: &Runtime, id: RegionId, parts: &mut HashMap<RegionId, Vec<RegionItem>>) {
    let Some(items) = runtime.contents(id) else {
        return;
    };
    for item in &items {
        if let RegionItem::Child(child) = item {
            snapshot(runtime, *child, parts);
        }
    }
    parts.insert(id, items);
}
struct Binding {
    context: ExperimentContext,
    children: Vec<RegionId>,
}
pub struct BridgeView {
    root: Option<RegionId>,
    parts: HashMap<RegionId, Vec<RegionItem>>,
    bindings: HashMap<RegionId, Binding>,
    holder: Option<ExperimentContext>,
    font_size: u16,
    color: Color,
}
fn full(
    id: RegionId,
    parts: &HashMap<RegionId, Vec<RegionItem>>,
    cx: &mut UiContext<'_>,
    bindings: &mut HashMap<RegionId, Binding>,
) {
    cx.column().key(region_key(id)).gap(3).children(|cx| {
        let context = cx.experiment_context();
        let mut children = Vec::new();
        for item in &parts[&id] {
            match item {
                RegionItem::Text { key, text } => {
                    cx.text(text.as_str()).key(Key::Generated(*key as u64));
                }
                RegionItem::Child(child) => {
                    children.push(*child);
                    full(*child, parts, cx, bindings);
                }
            }
        }
        bindings.insert(id, Binding { context, children });
    });
}
impl View for BridgeView {
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.bindings.clear();
        cx.element(
            "shell".into(),
            plico::Kind::Column,
            plico::Style {
                padding: 12.,
                gap: 6.,
                font_size: self.font_size,
                foreground: self.color,
                ..Default::default()
            },
            "",
            |cx| {
                cx.text("Outside before").key("before");
                cx.column().key("holder").children(|cx| {
                    self.holder = Some(cx.experiment_context());
                    if let Some(root) = self.root {
                        full(root, &self.parts, cx, &mut self.bindings);
                    }
                });
                cx.text("Outside after").key("after");
            },
        );
    }
}
pub struct EngineBridge {
    pub source: Source<BridgeView>,
    pub scene: Scene,
    root: RegionId,
    clock: VirtualClock,
    size: (u32, u32),
    pub last_passes: Vec<(RegionId, FrameStats)>,
}
impl EngineBridge {
    pub fn new(runtime: &Runtime, root: RegionId, text: &mut TextContext) -> Self {
        let clock = VirtualClock::default();
        let mut parts = HashMap::new();
        snapshot(runtime, root, &mut parts);
        let mut source = Source::with_clock(
            BridgeView {
                root: runtime.executions(root).map(|_| root),
                parts,
                bindings: HashMap::new(),
                holder: None,
                font_size: 19,
                color: Color::Literal(0x254059),
            },
            clock.clone(),
        );
        source.set_reduced_motion(true);
        let mut scene = Scene::new();
        let size = (800, 1200);
        source
            .render_retained_frame(&mut scene, text, size, 1.)
            .unwrap();
        assert!(source.last_error.is_none());
        Self {
            source,
            scene,
            root,
            clock,
            size,
            last_passes: Vec::new(),
        }
    }
    pub fn entity(&self, id: RegionId) -> Option<Entity> {
        self.source.view.bindings.get(&id).map(|b| b.context.parent)
    }
    pub fn context(&self, id: RegionId) -> Option<ExperimentContext> {
        self.source.view.bindings.get(&id).map(|b| b.context)
    }
    pub fn bindings_len(&self) -> usize {
        self.source.view.bindings.len()
    }
    fn remove_binding(
        bindings: &mut HashMap<RegionId, Binding>,
        parts: &mut HashMap<RegionId, Vec<RegionItem>>,
        id: RegionId,
    ) {
        parts.remove(&id);
        if let Some(binding) = bindings.remove(&id) {
            for child in binding.children {
                Self::remove_binding(bindings, parts, child);
            }
        }
    }
    /// Drive only output-changed regions, retaining opaque children in each pass.
    pub fn apply(&mut self, runtime: &mut Runtime, text: &mut TextContext) {
        self.last_passes.clear();
        let changed = runtime.flush();
        if runtime.executions(self.root).is_none() {
            self.source
                .experiment_redeclare(self.source.view.holder.unwrap(), |_| {})
                .unwrap();
            self.source.view.bindings.clear();
            self.source.view.parts.clear();
            self.source.view.root = None;
        } else {
            for id in changed {
                let items = runtime.contents(id).unwrap();
                let context = self.source.view.bindings[&id].context;
                let children: Vec<_> = items
                    .iter()
                    .filter_map(|item| {
                        if let RegionItem::Child(id) = item {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect();
                for &child in &children {
                    self.source
                        .view
                        .parts
                        .entry(child)
                        .or_insert_with(|| runtime.contents(child).unwrap());
                }
                let wanted: HashSet<_> = children.iter().copied().collect();
                let mut bindings = std::mem::take(&mut self.source.view.bindings);
                for child in bindings[&id].children.clone() {
                    if !wanted.contains(&child) {
                        Self::remove_binding(&mut bindings, &mut self.source.view.parts, child);
                    }
                }
                let stats = self
                    .source
                    .experiment_redeclare(context, |cx| {
                        for item in &items {
                            match item {
                                RegionItem::Text { key, text } => {
                                    cx.text(text.as_str()).key(Key::Generated(*key as u64));
                                }
                                RegionItem::Child(child) => {
                                    cx.column().key(region_key(*child)).gap(3).children(|cx| {
                                        cx.experiment_keep_children();
                                        let context = cx.experiment_context();
                                        bindings
                                            .entry(*child)
                                            .and_modify(|binding| binding.context = context)
                                            .or_insert(Binding {
                                                context,
                                                children: Vec::new(),
                                            });
                                    });
                                }
                            }
                        }
                    })
                    .unwrap();
                bindings.get_mut(&id).unwrap().children = children;
                self.source.view.bindings = bindings;
                self.source.view.parts.insert(id, items);
                self.last_passes.push((id, stats));
            }
        }
        self.prepare(text);
    }
    /// Global inherited-style changes refresh all construction contexts by a full pass.
    pub fn set_shell_style(
        &mut self,
        runtime: &Runtime,
        font_size: u16,
        color: Color,
        text: &mut TextContext,
    ) {
        self.source.view.font_size = font_size;
        self.source.view.color = color;
        self.rebuild(runtime, text);
    }
    /// Independent reference: reconstruct all declarations from the logical tree.
    pub fn rebuild(&mut self, runtime: &Runtime, text: &mut TextContext) {
        self.source.view.root = runtime.executions(self.root).map(|_| self.root);
        self.source.view.parts.clear();
        snapshot(runtime, self.root, &mut self.source.view.parts);
        self.source.request_rebuild();
        self.prepare(text);
    }
    /// Exercise a host-driven full declaration without asking the graph for a snapshot.
    pub fn force_host_rebuild(&mut self, text: &mut TextContext) {
        self.source.request_rebuild();
        self.prepare(text);
    }
    fn prepare(&mut self, text: &mut TextContext) {
        self.clock.advance(Duration::from_millis(16)).unwrap();
        self.source
            .experiment_prepare_without_scene(text, self.size.0, self.size.1);
        assert!(self.source.last_error.is_none());
    }
    pub fn encode(&mut self, text: &mut TextContext) {
        self.source
            .render_retained_frame(&mut self.scene, text, self.size, 1.)
            .unwrap();
        assert!(self.source.last_error.is_none());
    }
}
