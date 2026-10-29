//! A forced full declaration is the oracle, independent of dirty subscriptions.
use plico::prelude::*;
use plico::{Engine, Entity, FrameError, VirtualClock};
use plico_renderer::{FrameSource, ImeEvent, SemanticAction, SemanticRole, TextContext, UiEvent};
use plico_runtime::{
    Source, View,
    regions::{
        Access, BindState, OnClick, OnClickWith, OnSubmit, Reactive, Regions, State, StateSender,
        RegionUi,
    },
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::Duration,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct World {
    visible: bool,
    choose_b: bool,
    rows: Vec<u64>,
    font: u16,
    shade: u32,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Info {
    id: u64,
    label: String,
}
#[derive(Clone, Copy)]
struct Local {
    count: State<i32>,
    draft: State<TextBuffer>,
    enabled: State<bool>,
}
#[derive(Default)]
struct Diagnostics {
    live: BTreeMap<u64, Local>,
    drops: BTreeMap<u64, usize>,
}
type Records = Rc<RefCell<Diagnostics>>;
struct Probe {
    id: u64,
    records: Weak<RefCell<Diagnostics>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(records) = self.records.upgrade() {
            let mut records = records.borrow_mut();
            records.live.remove(&self.id);
            *records.drops.entry(self.id).or_default() += 1;
        }
    }
}
struct Bindings {
    a: State<i32>,
    b: State<i32>,
    level: State<f64>,
    theme: State<Theme>,
    infos: Vec<State<Info>>,
}
#[derive(Clone)]
struct Handles {
    world: State<World>,
    a: State<i32>,
    b: State<i32>,
    theme: State<Theme>,
    infos: Vec<State<Info>>,
}
struct Driven {
    regions: Regions,
    full: bool,
}
impl View for Driven {
    fn theme(&self) -> Theme {
        self.regions.theme()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.regions.show(cx);
    }
    fn snapshot_changed(&self) -> bool {
        self.regions.snapshot_changed()
    }
    fn poll_reload(&mut self) -> bool {
        self.regions.poll_reload()
    }
    fn replay_regions(
        &mut self,
        engine: &mut Engine,
    ) -> Option<Result<plico_runtime::regions::ReplayBatch, FrameError>> {
        if self.full {
            None
        } else {
            self.regions.replay_regions(engine)
        }
    }
    fn scene_cache_roots(&self) -> Vec<Entity> {
        self.regions.scene_cache_roots()
    }
}
fn application(records: Records, full: bool) -> (Driven, Handles) {
    let mut handles = None;
    let regions = Regions::new(|app| {
        let world = app.persisted_state(1, || World {
            visible: true,
            choose_b: false,
            rows: vec![0, 1, 2],
            font: 14,
            shade: 0x234567,
        });
        let a = app.persisted_state(2, || 3_i32);
        let b = app.persisted_state(3, || 7_i32);
        let theme = app.persisted_state(4, Theme::light);
        let level = app.persisted_state(5, || 40_f64);
        app.theme_state(theme);
        let infos: Vec<_> = (0..8)
            .map(|id| {
                app.persisted_state(100 + id, || Info {
                    id,
                    label: format!("Item {id}"),
                })
            })
            .collect();
        handles = Some(Handles {
            world,
            a,
            b,
            theme,
            infos: infos.clone(),
        });
        let bindings = app.state(|| Bindings {
            a,
            b,
            level,
            theme,
            infos,
        });
        let records = app.state(|| records);
        app.root((world, bindings, records), root);
    });
    (Driven { regions, full }, handles.unwrap())
}
#[component]
fn root(
    cx: &mut UiContext<'_>,
    (world, bindings, records): (Access<'_, World>, Access<'_, Bindings>, Access<'_, Records>),
) {
    column(cx)
        .width_percent(100.)
        .height_percent(100.)
        .gap(8)
        .children(|cx| {
            text(cx, "Fixture");
            text_empty(cx).key("property-label").rx(
                (bindings.read().a, bindings.read().b),
                |node, (a, b)| {
                    let a = *a.read();
                    node.text(format!("Bound: {a}/{}", *b.read()))
                        .font_size(12 + (a.unsigned_abs() % 12) as u16)
                        .weight(if a % 2 == 0 { 700 } else { 400 })
                        .font_family(if *b.read() % 2 == 0 {
                            FontFamily::Monospace
                        } else {
                            FontFamily::SansSerif
                        })
                        .opacity(if a < 0 { 0.4 } else { 1. })
                        .color(if a % 2 == 0 {
                            Color::Accent
                        } else {
                            Color::Muted
                        });
                },
            );
            button(cx)
                .text("Visibility")
                .on_click((world.handle(),), |(world,)| {
                    world.update(|w| w.visible = !w.visible)
                });
            button(cx)
                .text("Theme")
                .on_click((bindings.read().theme,), |(theme,)| {
                    let dark = *theme.read() == Theme::dark();
                    theme.set(if dark { Theme::light() } else { Theme::dark() });
                });
            cx.region((bindings.read().level,), |cx, (level,)| {
                let mut value = *level.read();
                slider(cx, "Level", &mut value, 0. ..=100.).bind(level.handle());
            });
            cx.region(
                (world.handle(), bindings.handle(), records.handle()),
                branch,
            );
        });
}
#[component]
fn branch(
    cx: &mut UiContext<'_>,
    (world, bindings, records): (Access<'_, World>, Access<'_, Bindings>, Access<'_, Records>),
) {
    let world_handle = world.handle();
    let bindings_handle = bindings.handle();
    let records_handle = records.handle();
    let world = world.read();
    let bindings = bindings.read();
    column(cx)
        .background(Color::Literal(world.shade))
        .gap(6)
        .children(|cx| {
            text(cx, "Branch")
                .font_size(world.font)
                .color(Color::Literal(world.shade));
            if world.visible {
                cx.region_keyed(
                    50,
                    (if world.choose_b {
                        bindings.b
                    } else {
                        bindings.a
                    },),
                    |cx, (selected,)| {
                        text!(cx, "Selected {}", *selected.read());
                    },
                );
                cx.each(
                    (world_handle, bindings_handle, records_handle),
                    |(world, _, _)| world.read().rows.clone(),
                    each_row,
                );
            } else {
                text(cx, "Hidden");
            }
        });
}
fn each_row(
    cx: &mut UiContext<'_>,
    (world, bindings, records): (Access<'_, World>, Access<'_, Bindings>, Access<'_, Records>),
    id: u64,
) {
    let info = bindings.read().infos[id as usize];
    let count = cx.local_persisted_state(1, || 0_i32);
    let enabled = cx.local_persisted_state(2, || true);
    let draft = cx.local_state(TextBuffer::new);
    let diagnostic = Rc::clone(&records.read());
    let _probe = cx.local_state(|| Probe {
        id,
        records: Rc::downgrade(&diagnostic),
    });
    diagnostic.borrow_mut().live.insert(
        id,
        Local {
            count,
            draft,
            enabled,
        },
    );
    cx.region((count, draft, enabled, info, world.handle()), row_view);
}
#[component]
#[allow(clippy::type_complexity)] // Exercise heterogeneous explicit dependencies in one fixture.
fn row_view(
    cx: &mut UiContext<'_>,
    (count, draft, enabled, info, world): (
        Access<'_, i32>,
        Access<'_, TextBuffer>,
        Access<'_, bool>,
        Access<'_, Info>,
        Access<'_, World>,
    ),
) {
    let info = info.read();
    let id = info.id;
    column(cx).gap(4).children(|cx| {
        text!(cx, "{}: {}", info.label, *count.read());
        let mut checked = *enabled.read();
        checkbox(cx, format!("Enabled {id}"), &mut checked).bind(enabled.handle());
        column(cx).disabled(!checked).children(|cx| {
            text_input(cx, format!("Draft {id}"), &draft.read())
                .width(240.)
                .height(36.)
                .bind(draft.handle())
                .on_submit((count.handle(),), |(count,)| count.update(|v| *v += 10));
            text!(
                cx,
                "Draft {id}: {}",
                draft.read().text().unwrap_or_default()
            );
            button(cx)
                .text(format!("Increment {id}"))
                .on_click((count.handle(),), |(count,)| count.update(|v| *v += 1));
        });
        button(cx).text(format!("Remove {id}")).on_click_with(
            (world.handle(),),
            id,
            |(world,), id| world.update(|w| w.rows.retain(|row| *row != id)),
        );
    });
}
struct Harness {
    source: Source<Driven>,
    scene: plico_renderer::Scene,
    text: TextContext,
    handles: Handles,
    records: Records,
    clock: VirtualClock,
    size: (u32, u32),
    senders: Vec<(State<i32>, StateSender<i32>)>,
}
impl Harness {
    fn new(full: bool) -> Self {
        let records = Records::default();
        let (view, handles) = application(Rc::clone(&records), full);
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(view, clock.clone());
        source.adapter.experiment_force_full_geometry = full;
        source.experiment_force_full_scene = full;
        source.set_reduced_motion(true);
        let mut h = Self {
            source,
            scene: plico_renderer::Scene::new(),
            text: TextContext::new(),
            handles,
            records,
            clock,
            size: (700, 1000),
            senders: vec![],
        };
        h.frame();
        h
    }
    fn frame(&mut self) {
        if self.source.view.full {
            self.source.request_rebuild();
        }
        self.source
            .render_retained_frame(&mut self.scene, &mut self.text, self.size, 1.);
        assert!(
            self.source.last_error.is_none(),
            "{:?}",
            self.source.last_error
        );
        let mut reference = plico_renderer::Scene::new();
        let surface = self.source.engine.theme().surface;
        let base =
            peniko::Color::from_rgb8((surface >> 16) as u8, (surface >> 8) as u8, surface as u8);
        plico_renderer::scene::build_scene_scaled_layers(
            &mut reference,
            &[self.source.adapter.placed.as_slice(), &[]],
            &mut self.text,
            self.size.0,
            self.size.1,
            base,
            1.,
        );
        assert_eq!(self.scene.flattened().commands, reference.commands);
    }
    fn target(&self, role: SemanticRole, label: &str) -> Option<u64> {
        self.source
            .adapter
            .semantics(&self.source.engine, 0)
            .node_by_role(role, label, None)
            .map(|n| n.id)
    }
    fn click(&mut self, role: SemanticRole, label: &str) -> bool {
        self.target(role, label).is_some_and(|target| {
            self.source
                .handle_semantic_action(SemanticAction::Click { target })
        })
    }
    fn local(&self, id: u64) -> Option<Local> {
        self.records.borrow().live.get(&id).copied()
    }
    fn apply(&mut self, op: u64, id: u64, value: i32) -> bool {
        let app = &self.source.view.regions;
        match op {
            0 => app.access(self.handles.a).set(value),
            1 => app.access(self.handles.b).set(value),
            2 => app
                .access(self.handles.world)
                .update(|w| w.choose_b = !w.choose_b),
            3 => app
                .access(self.handles.world)
                .update(|w| w.visible = !w.visible),
            4 => app.access(self.handles.world).update(|w| w.rows.reverse()),
            5 => app.access(self.handles.world).update(|w| {
                if !w.rows.contains(&id) {
                    w.rows.insert(0, id);
                }
            }),
            6 => app
                .access(self.handles.world)
                .update(|w| w.rows.retain(|row| *row != id)),
            7 => {
                app.access(self.handles.world).update(|w| {
                    w.font = 12 + (value.unsigned_abs() % 12) as u16;
                    w.shade = 0x123456 ^ (value.unsigned_abs() & 0x777777);
                    w.choose_b = !w.choose_b;
                });
                app.access(self.handles.a).set(value);
                app.access(self.handles.b).set(-value);
            }
            8 => return self.click(SemanticRole::Button, &format!("Increment {id}")),
            9 => return self.click(SemanticRole::Button, &format!("Remove {id}")),
            10 => return self.click(SemanticRole::CheckBox, &format!("Enabled {id}")),
            11 | 12 => {
                if let Some(target) = self.target(SemanticRole::TextInput, &format!("Draft {id}")) {
                    if !self
                        .source
                        .handle_semantic_action(SemanticAction::Focus { target })
                    {
                        return false;
                    }
                    return self
                        .source
                        .handle_event(if op == 11 {
                            UiEvent::TextInput(format!("x{value}"))
                        } else {
                            UiEvent::Ime(ImeEvent::Preedit {
                                text: "候補".into(),
                                cursor: None,
                            })
                        })
                        .handled;
                }
                return false;
            }
            13 => {
                if let Some(local) = self.local(id) {
                    app.access(local.count).set(value);
                }
            }
            14 => {
                if let Some(local) = self.local(id) {
                    self.senders.push((local.count, app.sender(local.count)));
                }
            }
            15 => {
                if let Some((state, sender)) = self.senders.last() {
                    let live = app.try_access(*state).is_some();
                    let accepted = sender.send(value).is_ok();
                    assert_eq!(live, accepted);
                    return accepted;
                }
                return false;
            }
            16 => {
                self.size = if value % 2 == 0 {
                    (480, 680)
                } else {
                    (900, 1200)
                }
            }
            17 => app.access(self.handles.theme).set(if value % 2 == 0 {
                Theme::dark()
            } else {
                Theme::light()
            }),
            18 => app
                .access(self.handles.infos[id as usize])
                .update(|i| i.label = format!("Item {id} v{value}")),
            19 => {
                if let Some(target) = self.target(SemanticRole::Slider, "Level") {
                    return self
                        .source
                        .handle_semantic_action(SemanticAction::SetNumericValue {
                            target,
                            value: value as f64,
                        });
                }
                return false;
            }
            20 => return self.click(SemanticRole::Button, "Visibility"),
            21 => return self.click(SemanticRole::Button, "Theme"),
            _ => {
                self.clock
                    .advance(Duration::from_millis(value.unsigned_abs() as u64))
                    .unwrap();
            }
        }
        true
    }
}
fn equivalent(a: &Harness, b: &Harness, context: &str) {
    fn compare(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
        match (a, b) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                assert_eq!(a.len(), b.len(), "keys {path}");
                for (k, v) in a {
                    // TextBuffer revision includes a process-global identity. The two
                    // runtimes own different buffers; compare contents/IME below.
                    if k == "input_revision" {
                        continue;
                    }
                    compare(v, &b[k], &format!("{path}/{k}"));
                }
            }
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "length {path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    compare(a, b, &format!("{path}/{i}"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }
    compare(
        &serde_json::to_value(a.source.engine.snapshot()).unwrap(),
        &serde_json::to_value(b.source.engine.snapshot()).unwrap(),
        context,
    );
    assert_eq!(
        a.source.engine.theme(),
        b.source.engine.theme(),
        "theme: {context}"
    );
    let semantics = |h: &Harness| {
        h.source
            .adapter
            .semantics(&h.source.engine, 0)
            .exposed_nodes()
            .iter()
            .map(|n| {
                (
                    n.role,
                    n.label.clone(),
                    n.value.clone(),
                    n.numeric_value,
                    n.disabled,
                    n.states,
                    n.bounds,
                )
            })
            .collect::<Vec<_>>()
    };
    let left = semantics(a);
    let right = semantics(b);
    assert_eq!(left.len(), right.len(), "semantics: {context}");
    for (left, right) in left.iter().zip(&right) {
        assert_eq!(
            (left.0, &left.1, &left.2, left.3, left.4, left.5),
            (right.0, &right.1, &right.2, right.3, right.4, right.5),
            "semantic values: {context}"
        );
        assert!(
            left.6
                .iter()
                .zip(right.6)
                .all(|(a, b)| (*a - b).abs() < 0.1),
            "semantic bounds: {context}: {left:?} != {right:?}"
        );
    }
    assert_eq!(
        a.source.adapter.placed.len(),
        b.source.adapter.placed.len(),
        "placed: {context}"
    );
    for (left, right) in a.source.adapter.placed.iter().zip(&b.source.adapter.placed) {
        for (a, b) in left
            .rect
            .iter()
            .chain(&left.content_origin)
            .zip(right.rect.iter().chain(&right.content_origin))
        {
            assert!(
                (*a - *b).abs() < 0.1,
                "placed geometry: {context}: {:?} != {:?}",
                left.rect,
                right.rect
            );
        }
        assert_eq!(left.paint.text, right.paint.text, "paint text: {context}");
        assert_eq!(
            left.paint.text_color, right.paint.text_color,
            "paint color: {context}"
        );
        assert_eq!(
            left.paint.font_size, right.paint.font_size,
            "paint font: {context}"
        );
    }
    let locals = |h: &Harness| {
        h.records
            .borrow()
            .live
            .iter()
            .map(|(&id, local)| {
                (
                    id,
                    *h.source.view.regions.access(local.count).read(),
                    h.source.view.regions.access(local.draft).read().text(),
                    *h.source.view.regions.access(local.enabled).read(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(locals(a), locals(b), "local state: {context}");
    assert_eq!(
        a.records.borrow().drops,
        b.records.borrow().drops,
        "owner drops: {context}"
    );
    let ime = |h: &Harness| {
        h.source.ime_state().map(|ime| {
            (
                ime.surrounding_text,
                ime.surrounding_cursor,
                ime.surrounding_anchor,
                ime.cursor_area,
            )
        })
    };
    assert_eq!(ime(a), ime(b), "IME: {context}");
}
fn pair_step(a: &mut Harness, b: &mut Harness, ops: &[(u64, u64, i32)], context: &str) {
    for &(op, id, value) in ops {
        assert_eq!(
            a.apply(op, id, value),
            b.apply(op, id, value),
            "event acceptance {op}: {context}"
        );
    }
    a.frame();
    b.frame();
    equivalent(a, b, context);
}
#[test]
fn nested_regions_match_forced_full_declaration_under_seeded_event_batches() {
    for seed in [1_u64, 7, 42, 151, 731, 2026, 0xdeadbeef, 0xabcdef01] {
        let (mut a, mut b) = (Harness::new(false), Harness::new(true));
        equivalent(&a, &b, "initial");
        let mut random = seed;
        for step in 0..128 {
            let mut ops = vec![];
            for _ in 0..1 + (step % 3) {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                ops.push((
                    random % 23,
                    (random >> 8) % 8,
                    ((random >> 20) % 101) as i32 - 25,
                ));
            }
            pair_step(
                &mut a,
                &mut b,
                &ops,
                &format!("seed={seed}, step={step}, ops={ops:?}"),
            );
        }
    }
}
#[test]
fn parent_child_batch_rebind_and_reorder_preserve_or_dispose_local_owners() {
    let (mut a, mut b) = (Harness::new(false), Harness::new(true));
    for (step, ops) in [
        vec![(8, 0, 0), (8, 1, 0), (14, 0, 0)],
        vec![(7, 0, 20), (13, 0, 17), (4, 0, 0)],
        vec![(1, 0, 81), (2, 0, 0), (0, 0, 99)],
        vec![(6, 0, 0), (13, 0, 55), (15, 0, 71)],
        vec![(5, 0, 0), (15, 0, 88)],
        vec![(12, 1, 0), (3, 0, 0)],
        vec![(3, 0, 0), (17, 0, 0)],
    ]
    .iter()
    .enumerate()
    {
        pair_step(&mut a, &mut b, ops, &format!("lifecycle step {step}"));
    }
}

#[test]
fn repeated_activation_in_one_batch_is_one_boolean_click_on_both_paths() {
    let (mut local, mut full) = (Harness::new(false), Harness::new(true));
    pair_step(
        &mut local,
        &mut full,
        &[(20, 0, 0), (20, 0, 0)],
        "duplicate click",
    );
    assert!(
        !local
            .source
            .view
            .regions
            .access(local.handles.world)
            .read()
            .visible
    );
}

#[test]
fn failed_local_declaration_reports_error_and_recovers_with_full_refresh() {
    let mut state = None;
    let view = Regions::new(|app| {
        let invalid = app.state(|| false);
        state = Some(invalid);
        app.root((invalid,), |cx, (invalid,)| {
            column(cx)
                .width(if *invalid.read() { -1. } else { 200. })
                .children(|cx| {
                    text(cx, "Recovered");
                });
        });
    });
    let mut source = Source::new(view);
    source.try_declare().unwrap();
    source.view.access(state.unwrap()).set(true);
    assert!(source.try_declare().is_err());
    source.view.access(state.unwrap()).set(false);
    source.try_declare().unwrap();
    assert!(
        source
            .engine
            .snapshot()
            .iter()
            .any(|node| !node.children.is_empty())
    );
    assert!(!source.view.snapshot_changed());
}

#[test]
fn nonsettling_replay_is_explicit_and_can_recover_after_cause_is_removed() {
    let mut state = None;
    let view = Regions::new(|app| {
        let repeat = app.state(|| false);
        state = Some(repeat);
        app.root((repeat,), |cx, (repeat,)| {
            text(cx, "Settle");
            if *repeat.read() {
                cx.request_rebuild();
            }
        });
    });
    let mut source = Source::new(view);
    source.try_declare().unwrap();
    source.view.access(state.unwrap()).set(true);
    assert!(matches!(
        source.try_declare(),
        Err(plico_runtime::SourceError::DidNotSettle)
    ));
    source.view.access(state.unwrap()).set(false);
    source.try_declare().unwrap();
    assert!(!source.view.snapshot_changed());
}

#[test]
fn panics_are_not_swallowed_and_declaration_tls_is_unwound() {
    let mut state = None;
    let view = Regions::new(|app| {
        let fail = app.state(|| false);
        state = Some(fail);
        app.root((fail,), |cx, (fail,)| {
            text(cx, "Panic boundary");
            assert!(!*fail.read(), "intentional region panic");
        });
    });
    let mut source = Source::new(view);
    source.try_declare().unwrap();
    source.view.access(state.unwrap()).set(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.try_declare()));
    let payload = result.expect_err("user panic must propagate");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"intentional region panic")
    );
    // Panics are fail-fast, not transactional. Discard the failed source rather
    // than continuing with partially applied tree edits. A new source must work.
    drop(source);
    let mut fresh = Harness::new(false);
    fresh.frame();
}
