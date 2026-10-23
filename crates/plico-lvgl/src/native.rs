//! Native-widget projection onto LVGL through lightvgl-sys raw bindings.
//!
//! The adapter consumes the same Plico core abstractions as before
//! (`Engine`, `FrameDelta`, `KindFamily`, `Style`); only the widget layer is
//! raw FFI. Unsafe is confined to this module's small helpers.
use crate::ffi::{self, Obj, color, pct, with_cstr};
use lightvgl_sys as sys;
use plico_core::{
    Changes, Engine, Entity, FrameDelta, Kind, KindFamily, NodeRef, Style, StyleProperties, Theme,
};
use snafu::{OptionExt, Snafu};
use std::cell::RefCell;
use std::collections::HashMap;

#[path = "style_diff.rs"]
mod style_diff;

type Queue = RefCell<Vec<Entity>>;
type Route = (Entity, std::rc::Weak<Queue>);
thread_local! {
    // Native callbacks retain no application state. Route directly to a
    // generational identity so a queued click cannot target a reused address.
    static ROUTES: RefCell<HashMap<usize, Route>> = RefCell::new(HashMap::new());
}
unsafe extern "C" fn clicked(event: *mut sys::lv_event_t) {
    unsafe {
        let target = sys::lv_event_get_current_target(event) as *mut sys::lv_obj_t;
        if target.is_null() {
            return;
        }
        let key = target as usize;
        ROUTES.with(|routes| {
            if let Some((entity, queue)) = routes.borrow().get(&key)
                && let Some(queue) = queue.upgrade()
            {
                queue.borrow_mut().push(*entity);
            }
        });
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("LVGL display must be initialized before attaching Plico"))]
    NoScreen,
    #[snafu(display("LVGL widget creation returned NULL"))]
    Widget,
    #[snafu(display("LVGL text cannot contain an embedded NUL"))]
    TextNul,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub created: usize,
    pub removed: usize,
    pub reused: usize,
    pub style_updates: usize,
    pub text_updates: usize,
    /// Incremental native property setter calls (excludes initial binding).
    pub property_writes: usize,
    /// Complete native style resets, needed only at first binding.
    pub style_resets: usize,
}

/// A mounted LVGL object; `label` holds a composite button's child label.
struct Widget {
    obj: *mut Obj,
    /// Child label of a composite button. For a pure Text widget, `obj`
    /// itself is the label and `owns_text` is set instead.
    label: *mut Obj,
    owns_text: bool,
    button: bool,
}
// The adapter is single-threaded; every handle outlives its Mounted entry.
unsafe impl Send for Widget {}
impl Widget {
    fn object(&self) -> *mut Obj {
        self.obj
    }
    /// Family-level class matching: a native widget survives a declared kind
    /// change that stays within the same [`KindFamily`]. Exact-button kinds
    /// keep their identity; degraded controls ride the container family.
    fn class_matches(&self, kind: Kind) -> bool {
        if self.button {
            matches!(kind, Kind::Button | Kind::ButtonBase)
        } else if !self.label.is_null() || self.owns_text {
            matches!(kind.family(), KindFamily::Text)
        } else {
            matches!(kind.family(), KindFamily::Container | KindFamily::Media)
        }
    }
}
struct Mounted {
    widget: Widget,
    kind: Kind,
    style: Option<(Style, Theme)>,
    text: Option<String>,
}

/// Single-threaded native-widget projection. The application owns its Engine
/// and Rust state; no alternate component API or reactive graph is introduced.
pub struct Adapter {
    nodes: HashMap<Entity, Mounted>,
    parents: HashMap<Entity, Entity>,
    screen: *mut Obj,
    events: std::rc::Rc<Queue>,
    stats: Stats,
}
unsafe impl Send for Adapter {}
impl Adapter {
    pub fn new(_driver: &ffi::Driver) -> Result<Self, Error> {
        let screen = unsafe { sys::lv_screen_active() };
        let screen = (!screen.is_null())
            .then_some(screen)
            .context(NoScreenSnafu)?;
        Ok(Self {
            nodes: HashMap::new(),
            parents: HashMap::new(),
            screen,
            events: std::rc::Rc::new(RefCell::new(Vec::new())),
            stats: Stats::default(),
        })
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Project a successful core declaration. Preflight NUL text before
    /// modifying LVGL. Layout and drawing are both performed by LVGL.
    pub fn sync(&mut self, engine: &Engine) -> Result<(), Error> {
        self.sync_delta(engine, None)
    }
    /// Incremental projection: when the frame delta carries no structural
    /// change (no creation, removal, reorder or kind change), only the
    /// dirtied nodes are re-read — the full tree walk is skipped entirely.
    pub fn sync_delta(&mut self, engine: &Engine, delta: Option<FrameDelta>) -> Result<(), Error> {
        let Some(delta) = delta else {
            return self.sync_full(engine, None);
        };
        if !delta.frame.intersects(Changes::STRUCTURE) && delta.removed.is_empty() {
            return self.sync_touched(engine, &delta);
        }
        self.sync_full(engine, Some(delta))
    }
    fn sync_touched(&mut self, engine: &Engine, delta: &FrameDelta) -> Result<(), Error> {
        self.stats = Stats::default();
        for change in &delta.changes {
            let Some(mounted) = self.nodes.get_mut(&change.entity) else {
                continue;
            };
            let Some(node) = engine.node(change.entity) else {
                continue;
            };
            if mounted.style != Some((node.style, engine.theme())) || mounted.kind != node.kind {
                if let Some((old_style, old_theme)) = mounted.style {
                    let writes = style_diff::apply(
                        mounted.widget.object(),
                        (mounted.kind, old_style, old_theme),
                        (node.kind, node.style, engine.theme()),
                        change.properties,
                    );
                    self.stats.property_writes += writes;
                    self.stats.style_updates += usize::from(writes != 0);
                } else {
                    apply_style(mounted.widget.object(), &node, engine.theme());
                    self.stats.style_resets += 1;
                    self.stats.style_updates += 1;
                }
                mounted.style = Some((node.style, engine.theme()));
                mounted.kind = node.kind;
            }
            if mounted.text.as_deref() != Some(node.text)
                && change.changes.intersects(Changes::MEASURE | Changes::PAINT)
            {
                set_text(text_target(&mounted.widget), node.text)?;
                node.text
                    .clone_into(mounted.text.get_or_insert_with(String::new));
                self.stats.text_updates += 1;
            }
        }
        Ok(())
    }
    fn sync_full(&mut self, engine: &Engine, delta: Option<FrameDelta>) -> Result<(), Error> {
        fn walk(engine: &Engine, entity: Entity, out: &mut Vec<Entity>) -> Result<(), Error> {
            let node = engine.node(entity).expect("live engine child");
            snafu::ensure!(!node.text.contains('\0'), TextNulSnafu);
            out.push(entity);
            for child in node.children {
                walk(engine, *child, out)?;
            }
            Ok(())
        }
        let mut order = Vec::new();
        walk(engine, engine.root(), &mut order)?;
        self.stats = Stats::default();
        let dead: Vec<_> = match &delta {
            Some(delta) => delta.removed.clone(),
            None => self
                .nodes
                .iter()
                .filter(|(id, mounted)| {
                    engine
                        .node(**id)
                        .is_none_or(|n| !mounted.widget.class_matches(n.kind))
                })
                .map(|(id, _)| *id)
                .collect(),
        };
        for id in self.children_first(dead) {
            self.unmount(id);
            self.stats.removed += 1;
        }

        // Record parent links from the engine's child lists as we mount.
        self.parents.clear();
        self.parents.insert(engine.root(), engine.root());
        for id in &order {
            let node = engine.node(*id).expect("preflighted node");
            for child in node.children {
                self.parents.insert(*child, *id);
            }
        }
        for &id in &order {
            let node = engine.node(id).expect("preflighted node");
            if !self.nodes.contains_key(&id) {
                let parent = self
                    .parents
                    .get(&id)
                    .copied()
                    .filter(|p| *p != id)
                    .and_then(|p| self.nodes.get(&p))
                    .map(|m| m.widget.object());
                let widget = self.create(node.kind, parent)?;
                if widget.button {
                    let obj = widget.object();
                    ROUTES.with(|routes| {
                        routes
                            .borrow_mut()
                            .insert(obj as usize, (id, std::rc::Rc::downgrade(&self.events)));
                    });
                    unsafe {
                        sys::lv_obj_add_event_cb(
                            obj,
                            Some(clicked),
                            sys::lv_event_code_t_LV_EVENT_CLICKED,
                            std::ptr::null_mut(),
                        );
                    };
                }
                self.nodes.insert(
                    id,
                    Mounted {
                        widget,
                        kind: node.kind,
                        style: None,
                        text: None,
                    },
                );
                self.stats.created += 1;
            } else {
                self.stats.reused += 1;
            }
            let mounted = self.nodes.get_mut(&id).expect("mounted node");
            if mounted.style != Some((node.style, engine.theme())) || mounted.kind != node.kind {
                if let Some((old_style, old_theme)) = mounted.style {
                    let mut fields = node.style.diff(old_style).properties;
                    if mounted.kind != node.kind {
                        fields = StyleProperties::all();
                    }
                    if old_theme != engine.theme() {
                        fields |= StyleProperties::BACKGROUND
                            | StyleProperties::FOREGROUND
                            | StyleProperties::BORDER_COLOR;
                    }
                    let writes = style_diff::apply(
                        mounted.widget.object(),
                        (mounted.kind, old_style, old_theme),
                        (node.kind, node.style, engine.theme()),
                        fields,
                    );
                    self.stats.property_writes += writes;
                    self.stats.style_updates += usize::from(writes != 0);
                } else {
                    apply_style(mounted.widget.object(), &node, engine.theme());
                    self.stats.style_resets += 1;
                    self.stats.style_updates += 1;
                }
                mounted.style = Some((node.style, engine.theme()));
                mounted.kind = node.kind;
            }
            if mounted.text.as_deref() != Some(node.text) {
                set_text(text_target(&mounted.widget), node.text)?;
                node.text
                    .clone_into(mounted.text.get_or_insert_with(String::new));
                self.stats.text_updates += 1;
            }
        }
        for &id in &order {
            let node = engine.node(id).unwrap();
            for (index, child) in node.children.iter().enumerate() {
                let obj = self.nodes[child].widget.object();
                let current = unsafe { sys::lv_obj_get_index(obj) };
                if current != index as i32 {
                    unsafe { sys::lv_obj_move_to_index(obj, index as i32) };
                }
            }
        }
        unsafe { sys::lv_obj_update_layout(self.nodes[&engine.root()].widget.object()) };
        Ok(())
    }

    fn create(&self, kind: Kind, parent: Option<*mut Obj>) -> Result<Widget, Error> {
        let parent = parent.unwrap_or(self.screen);
        let (obj, label, owns_text, button) = match kind.family() {
            KindFamily::Text => {
                let obj = unsafe { sys::lv_label_create(parent) };
                (obj, std::ptr::null_mut(), true, false)
            }
            // Exact-button kinds get native click routing; degraded controls
            // (Switch/Checkbox/Slider) render as containers carrying their
            // label and lose interaction.
            KindFamily::Control => match kind {
                Kind::Button => {
                    let button = unsafe { sys::lv_button_create(parent) };
                    let label = unsafe { sys::lv_label_create(button) };
                    unsafe { sys::lv_obj_center(label) };
                    (button, label, false, true)
                }
                Kind::ButtonBase => {
                    let button = unsafe { sys::lv_button_create(parent) };
                    (button, std::ptr::null_mut(), false, true)
                }
                _ => {
                    let obj = unsafe { sys::lv_obj_create(parent) };
                    (obj, std::ptr::null_mut(), false, false)
                }
            },
            // Containers nest children; media renders as an empty box.
            KindFamily::Container | KindFamily::Media => {
                let obj = unsafe { sys::lv_obj_create(parent) };
                (obj, std::ptr::null_mut(), false, false)
            }
        };
        snafu::ensure!(!obj.is_null(), WidgetSnafu);
        Ok(Widget {
            obj,
            label,
            owns_text,
            button,
        })
    }

    /// Drain native callbacks at the event boundary, before declaring again.
    /// Deleted/disabled entities are ignored; callbacks never borrow app state.
    pub fn dispatch(&mut self, engine: &mut Engine) -> usize {
        let mut count = 0;
        for entity in self.events.borrow_mut().drain(..) {
            if engine.node(entity).is_some_and(|n| {
                matches!(n.kind, Kind::Button | Kind::ButtonBase) && !n.style.disabled
            }) {
                engine.queue_click(entity);
                count += 1;
            }
        }
        count
    }
    /// Programmatic native event for headless verification, not a direct core click.
    pub fn send_click(&self, entity: Entity) -> bool {
        if let Some(mounted) = self.nodes.get(&entity).filter(|m| m.widget.button) {
            unsafe {
                sys::lv_obj_send_event(
                    mounted.widget.object(),
                    sys::lv_event_code_t_LV_EVENT_CLICKED,
                    std::ptr::null_mut(),
                )
            };
            true
        } else {
            false
        }
    }
    /// Diagnostic identity for reuse assertions. Never dereference this value.
    pub fn object_id(&self, entity: Entity) -> Option<usize> {
        self.nodes.get(&entity).map(|n| n.widget.object() as usize)
    }
    pub fn bounds(&self, entity: Entity) -> Option<[i32; 4]> {
        self.nodes.get(&entity).map(|n| {
            let mut area = sys::lv_area_t {
                x1: 0,
                y1: 0,
                x2: 0,
                y2: 0,
            };
            unsafe { sys::lv_obj_get_coords(n.widget.object(), &mut area) };
            [area.x1, area.y1, area.x2, area.y2]
        })
    }
    fn unmount(&mut self, entity: Entity) {
        if let Some(node) = self.nodes.remove(&entity) {
            ROUTES.with(|routes| routes.borrow_mut().remove(&(node.widget.object() as usize)));
            unsafe {
                let obj = node.widget.object();
                // LVGL stores this pointer; unlink it before freeing it.
                let style = style_of(obj);
                if !style.is_null() {
                    sys::lv_obj_remove_style(obj, style, 0);
                    sys::lv_obj_set_user_data(obj, std::ptr::null_mut());
                    sys::lv_style_reset(style);
                    drop(Box::from_raw(style));
                }
                sys::lv_obj_delete(obj);
            };
        }
    }
    fn children_first(&self, mut ids: Vec<Entity>) -> Vec<Entity> {
        ids.sort_by_cached_key(|id| {
            let mut depth = 0;
            let mut current = *id;
            while let Some(parent) = self.parents.get(&current) {
                if *parent == current {
                    break;
                }
                depth += 1;
                current = *parent;
            }
            std::cmp::Reverse(depth)
        });
        ids
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        let ids: Vec<Entity> = self.nodes.keys().copied().collect();
        // Deleting a parent also deletes native children. HashMap iteration
        // order would therefore occasionally delete a dangling child pointer.
        for id in self.children_first(ids) {
            self.unmount(id);
        }
    }
}

/// The per-object heap style, or null before the first bind.
fn style_of(obj: *mut Obj) -> *mut sys::lv_style_t {
    unsafe { sys::lv_obj_get_user_data(obj) as *mut sys::lv_style_t }
}
/// The label API applies only to text-bearing widgets.
fn text_target(widget: &Widget) -> Option<*mut Obj> {
    if !widget.label.is_null() {
        Some(widget.label)
    } else if widget.owns_text {
        Some(widget.obj)
    } else {
        None
    }
}
fn set_text(target: Option<*mut Obj>, text: &str) -> Result<(), Error> {
    snafu::ensure!(!text.contains('\0'), TextNulSnafu);
    // Containers ignore text: calling the label API on them corrupts their
    // user data through translation-tag cleanup.
    let Some(target) = target else {
        return Ok(());
    };
    with_cstr(text, |p| unsafe { sys::lv_label_set_text(target, p) });
    Ok(())
}

fn apply_style(obj: *mut Obj, node: &NodeRef<'_>, theme: Theme) {
    let s = node.style;
    let container_like = matches!(
        node.kind.family(),
        KindFamily::Container | KindFamily::Control
    );
    unsafe {
        sys::lv_obj_remove_style_all(obj);
        // LVGL links the style by pointer, not copy: it must outlive the
        // object. One leaked box per mounted widget; rebinding replaces the
        // properties in place through the same leaked allocation.
        if style_of(obj).is_null() {
            let style = Box::into_raw(Box::new(std::mem::zeroed::<sys::lv_style_t>()));
            sys::lv_obj_set_user_data(obj, style as *mut std::ffi::c_void);
            sys::lv_style_init(style);
        }
        let style = style_of(obj);
        let width = s
            .width
            .map(|v| v as i32)
            .or_else(|| s.width_percent.map(|v| pct(v as i32)))
            .unwrap_or(if node.kind == Kind::Root {
                pct(100)
            } else {
                ffi::SIZE_CONTENT
            });
        let height = s
            .height
            .map(|v| v as i32)
            .or_else(|| s.height_percent.map(|v| pct(v as i32)))
            .unwrap_or(if node.kind == Kind::Root {
                pct(100)
            } else {
                ffi::SIZE_CONTENT
            });
        sys::lv_style_set_width(style, width);
        sys::lv_style_set_height(style, height);
        sys::lv_style_set_pad_top(style, s.vertical_padding() as i32);
        sys::lv_style_set_pad_bottom(style, s.vertical_padding() as i32);
        sys::lv_style_set_pad_left(style, s.horizontal_padding() as i32);
        sys::lv_style_set_pad_right(style, s.horizontal_padding() as i32);
        sys::lv_style_set_pad_row(style, s.gap as i32);
        sys::lv_style_set_pad_column(style, s.gap as i32);
        sys::lv_style_set_radius(style, s.radius as i32);
        sys::lv_style_set_border_width(style, s.border_width as i32);
        sys::lv_style_set_border_color(style, color(theme.resolve(s.border_color)));
        sys::lv_style_set_text_color(style, color(theme.resolve(s.foreground)));
        let font = if s.font_size >= 24 {
            &sys::lv_font_montserrat_24
        } else {
            &sys::lv_font_montserrat_16
        };
        sys::lv_style_set_text_font(style, font as *const sys::lv_font_t);
        if let Some(bg) = s.background {
            sys::lv_style_set_bg_color(style, color(theme.resolve(bg)));
            sys::lv_style_set_bg_opa(style, style_diff::opacity(s.background_alpha));
        } else {
            sys::lv_style_set_bg_opa(style, 0);
        }
        sys::lv_style_set_opa(style, style_diff::opacity(s.opacity));
        if container_like {
            sys::lv_style_set_layout(style, sys::lv_layout_t_LV_LAYOUT_FLEX as u16);
        }
        // add_style copies the style's property storage into the object's own
        // style list; the stack struct is not retained.
        sys::lv_obj_add_style(obj, style, 0);
        if container_like {
            let flow = if matches!(node.kind, Kind::Row | Kind::ButtonBase) {
                sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW
            } else {
                sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN
            };
            sys::lv_obj_set_flex_flow(obj, flow);
            let main = match s.justify.unwrap_or(plico_core::Justify::Start) {
                plico_core::Justify::Center => sys::lv_flex_align_t_LV_FLEX_ALIGN_CENTER,
                plico_core::Justify::End => sys::lv_flex_align_t_LV_FLEX_ALIGN_END,
                plico_core::Justify::SpaceBetween => {
                    sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_BETWEEN
                }
                plico_core::Justify::SpaceAround => sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_AROUND,
                plico_core::Justify::SpaceEvenly => sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_EVENLY,
                plico_core::Justify::Start | plico_core::Justify::Stretch => {
                    sys::lv_flex_align_t_LV_FLEX_ALIGN_START
                }
            };
            // LVGL has no flex baseline/stretch alignment; retain its start fallback.
            let cross = match s.align.unwrap_or(plico_core::Align::Start) {
                plico_core::Align::Center => sys::lv_flex_align_t_LV_FLEX_ALIGN_CENTER,
                plico_core::Align::End => sys::lv_flex_align_t_LV_FLEX_ALIGN_END,
                plico_core::Align::Start
                | plico_core::Align::Stretch
                | plico_core::Align::Baseline => sys::lv_flex_align_t_LV_FLEX_ALIGN_START,
            };
            // LVGL aligns each flex track separately from the items inside it.
            sys::lv_obj_set_flex_align(obj, main, cross, cross);
        }
        if s.disabled {
            sys::lv_obj_add_state(obj, sys::lv_state_t_LV_STATE_DISABLED);
        } else {
            sys::lv_obj_remove_state(obj, sys::lv_state_t_LV_STATE_DISABLED);
        }
        sys::lv_obj_remove_flag(obj, sys::lv_obj_flag_t_LV_OBJ_FLAG_SCROLLABLE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_core::{DeclareView, Length, prelude::*};

    struct View {
        height: Length,
        fill: Option<plico_core::Color>,
        alpha: f32,
        opacity: f32,
        theme: Theme,
    }
    impl DeclareView for View {
        fn theme(&self) -> Theme {
            self.theme
        }
        fn show(&mut self, cx: &mut UiContext) {
            let mut panel = column(cx)
                .width(120)
                .height(self.height)
                .padding(7)
                .radius(6.)
                .background_alpha(self.alpha)
                .opacity(self.opacity);
            if let Some(fill) = self.fill {
                panel = panel.background(fill);
            }
            panel.children(|cx| {
                text(cx, "Retained child");
            });
        }
    }
    fn frame(engine: &mut Engine, view: &mut View, adapter: &mut Adapter) {
        engine.declare_until_settled(view).unwrap();
        let delta = engine.take_frame_delta();
        adapter.sync_delta(engine, Some(delta)).unwrap();
    }
    unsafe fn number(obj: *mut Obj, prop: sys::lv_style_prop_t) -> i32 {
        unsafe { sys::lv_obj_get_style_prop(obj, 0, prop).num }
    }
    #[test]
    fn property_patches_preserve_native_state_and_restore_defaults() {
        let driver = ffi::Driver::init(480, 800);
        let mut engine = Engine::default();
        let mut adapter = Adapter::new(&driver).unwrap();
        let mut view = View {
            height: Length::Px(40.),
            fill: Some(plico_core::Color::Accent),
            alpha: 1.,
            opacity: 1.,
            theme: Theme::default(),
        };
        frame(&mut engine, &mut view, &mut adapter);
        let panel = engine.node(engine.root()).unwrap().children[0];
        let obj = adapter.nodes[&panel].widget.object();
        let owned_style = style_of(obj);

        view.height = Length::Px(80.);
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(adapter.stats().property_writes, 1);
        assert_eq!(adapter.stats().style_resets, 0);
        assert_eq!(adapter.stats().created, 0);
        assert_eq!(style_of(obj), owned_style);
        unsafe {
            assert_eq!(number(obj, sys::_lv_style_id_t_LV_STYLE_HEIGHT as _), 80);
            assert_eq!(number(obj, sys::_lv_style_id_t_LV_STYLE_PAD_TOP as _), 7);
            assert_eq!(number(obj, sys::_lv_style_id_t_LV_STYLE_RADIUS as _), 6);
        }
        view.height = Length::Px(80.4);
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(
            adapter.stats().property_writes,
            0,
            "same native pixel value"
        );

        view.height = Length::Percent(50.);
        frame(&mut engine, &mut view, &mut adapter);
        unsafe {
            assert_eq!(
                number(obj, sys::_lv_style_id_t_LV_STYLE_HEIGHT as _),
                pct(50)
            );
        }
        view.height = Length::Auto;
        frame(&mut engine, &mut view, &mut adapter);
        unsafe {
            assert_eq!(
                number(obj, sys::_lv_style_id_t_LV_STYLE_HEIGHT as _),
                ffi::SIZE_CONTENT
            );
        }

        view.theme.accent = 0x123456;
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(
            adapter.stats().property_writes,
            1,
            "resolve the changed theme token only"
        );
        unsafe {
            let actual =
                sys::lv_obj_get_style_prop(obj, 0, sys::_lv_style_id_t_LV_STYLE_BG_COLOR as _)
                    .color;
            let expected = color(0x123456);
            assert_eq!(
                (actual.red, actual.green, actual.blue),
                (expected.red, expected.green, expected.blue)
            );
        }
        view.alpha = 0.5;
        view.opacity = 0.5;
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(adapter.stats().property_writes, 2);
        view.alpha = 1.;
        view.opacity = 1.;
        view.fill = None;
        frame(&mut engine, &mut view, &mut adapter);
        unsafe {
            assert_eq!(number(obj, sys::_lv_style_id_t_LV_STYLE_OPA as _), 255);
            assert_eq!(number(obj, sys::_lv_style_id_t_LV_STYLE_BG_OPA as _), 0);
        }
        assert_eq!(adapter.stats().style_resets, 0);
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(adapter.stats().property_writes, 0);
        assert_eq!(adapter.stats().style_updates, 0);
        // Drop also exercises child-before-parent destruction of the native tree.
    }
}
