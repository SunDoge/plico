//! Native-widget projection onto LVGL through lightvgl-sys raw bindings.
//!
//! The adapter consumes the same Plico core abstractions as before
//! (`Engine`, `FrameDelta`, `Kind`, `Style`); only the widget layer is
//! raw FFI. Unsafe is confined to this module's small helpers.
use crate::ffi::{self, Obj, color, pct, with_cstr};
use crate::projection::{self, NativeClass};
use lightvgl_sys as sys;
use plico_core::{
    Changes, Engine, Entity, FrameDelta, Kind, NodeRef, Style, StyleProperties, Theme,
};
use snafu::{OptionExt, Snafu};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[path = "style_diff.rs"]
mod style_diff;
#[path = "svg.rs"]
mod svg;

type Queue = RefCell<Vec<(Entity, sys::lv_event_code_t)>>;
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
                queue
                    .borrow_mut()
                    .push((*entity, sys::lv_event_get_code(event)));
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
    #[snafu(display("LVGL SVG raster allocation or size is invalid"))]
    SvgRaster,
    #[snafu(display("LVGL does not implement node kind {kind:?}"))]
    UnsupportedKind { kind: Kind },
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
    /// SVGs rasterized this sync; unchanged icons retain their pixels.
    pub svg_rasterizations: usize,
}

/// A mounted LVGL object; `label` holds a composite button's child label.
struct Widget {
    obj: *mut Obj,
    /// Child label of a composite button. For a pure Text widget, `obj`
    /// itself is the label and `owns_text` is set instead.
    label: *mut Obj,
    owns_text: bool,
    button: bool,
    editor: bool,
    svg: Option<svg::Raster>,
    is_svg: bool,
}
// The adapter is single-threaded; every handle outlives its Mounted entry.
unsafe impl Send for Widget {}
impl Widget {
    fn object(&self) -> *mut Obj {
        self.obj
    }
    /// Reuse only objects with the same native representation. A change in
    /// interaction or core classification cannot silently change the object class.
    fn class_matches(&self, kind: Kind) -> bool {
        let mounted = if self.is_svg {
            NativeClass::Image
        } else if self.editor {
            NativeClass::Editor
        } else if self.button && !self.label.is_null() {
            NativeClass::LabeledButton
        } else if self.button {
            NativeClass::Button
        } else if self.owns_text {
            NativeClass::Label
        } else {
            NativeClass::Container
        };
        projection::class(kind) == Some(mounted)
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
    keyboard: *mut Obj,
    editing: Option<Entity>,
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
            keyboard: std::ptr::null_mut(),
            editing: None,
        })
    }
    /// Pixels retained by the currently mounted SVG nodes, excluding LVGL object overhead.
    pub fn svg_cache_bytes(&self) -> usize {
        self.nodes
            .values()
            .filter_map(|n| n.widget.svg.as_ref())
            .map(svg::Raster::bytes)
            .sum()
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
        // Validate the complete delta before changing any native property.
        for change in &delta.changes {
            if let Some(node) = engine.node(change.entity) {
                preflight(&node)?;
            }
        }
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
            self.stats.svg_rasterizations += sync_svg(&mut mounted.widget, &node, engine.theme())?;
            sync_editor(&mounted.widget, engine, change.entity)?;
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
            preflight(&node)?;
            out.push(entity);
            for child in node.children {
                walk(engine, *child, out)?;
            }
            Ok(())
        }
        let mut order = Vec::new();
        walk(engine, engine.root(), &mut order)?;
        self.stats = Stats::default();
        let mut dead = HashSet::new();
        let mut remount = Vec::new();
        if let Some(delta) = &delta {
            dead.extend(delta.removed.iter().copied());
            for change in &delta.changes {
                if let (Some(mounted), Some(node)) =
                    (self.nodes.get(&change.entity), engine.node(change.entity))
                    && !mounted.widget.class_matches(node.kind)
                {
                    remount.push(change.entity);
                }
            }
        } else {
            for (&id, mounted) in &self.nodes {
                match engine.node(id) {
                    None => {
                        dead.insert(id);
                    }
                    Some(node) if !mounted.widget.class_matches(node.kind) => remount.push(id),
                    _ => {}
                }
            }
        }
        // Deleting an LVGL parent also deletes its native children. Retained
        // core descendants must be unmounted/remounted too, even when their
        // own kind has not changed. Other removed descendants are in delta.
        while let Some(id) = remount.pop() {
            if !self.nodes.contains_key(&id) || !dead.insert(id) {
                continue;
            }
            if let Some(node) = engine.node(id) {
                remount.extend_from_slice(node.children);
            }
        }
        if !dead.is_empty() {
            // Entity can survive a kind change. An event from the old native
            // instance must not activate its replacement with the same Entity.
            self.events
                .borrow_mut()
                .retain(|(id, _)| !dead.contains(id));
        }
        for id in self.children_first(dead.into_iter().collect()) {
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
                if widget.button || widget.editor {
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
                            if widget.editor {
                                sys::lv_event_code_t_LV_EVENT_ALL
                            } else {
                                sys::lv_event_code_t_LV_EVENT_CLICKED
                            },
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
            self.stats.svg_rasterizations += sync_svg(&mut mounted.widget, &node, engine.theme())?;
            sync_editor(&mounted.widget, engine, id)?;
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
        let native_class = projection::class(kind).ok_or(Error::UnsupportedKind { kind })?;
        let (obj, label, owns_text, button) = match native_class {
            NativeClass::Image => (
                unsafe { sys::lv_image_create(parent) },
                std::ptr::null_mut(),
                false,
                false,
            ),
            NativeClass::Editor => {
                let obj = unsafe { sys::lv_textarea_create(parent) };
                if !obj.is_null() {
                    unsafe { sys::lv_textarea_set_one_line(obj, true) };
                }
                (obj, std::ptr::null_mut(), false, false)
            }
            NativeClass::Label => (
                unsafe { sys::lv_label_create(parent) },
                std::ptr::null_mut(),
                true,
                false,
            ),
            NativeClass::LabeledButton => {
                let button = unsafe { sys::lv_button_create(parent) };
                snafu::ensure!(!button.is_null(), WidgetSnafu);
                let label = unsafe { sys::lv_label_create(button) };
                if label.is_null() {
                    unsafe { sys::lv_obj_delete(button) };
                    return Err(Error::Widget);
                }
                unsafe { sys::lv_obj_center(label) };
                (button, label, false, true)
            }
            NativeClass::Button => (
                unsafe { sys::lv_button_create(parent) },
                std::ptr::null_mut(),
                false,
                true,
            ),
            NativeClass::Container => (
                unsafe { sys::lv_obj_create(parent) },
                std::ptr::null_mut(),
                false,
                false,
            ),
        };
        snafu::ensure!(!obj.is_null(), WidgetSnafu);
        Ok(Widget {
            obj,
            label,
            owns_text,
            button,
            editor: kind == Kind::TextInput,
            is_svg: kind == Kind::Svg,
            svg: None,
        })
    }

    /// Drain native callbacks at the event boundary, before declaring again.
    /// Deleted/disabled entities are ignored; callbacks never borrow app state.
    pub fn dispatch(&mut self, engine: &mut Engine) -> usize {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        let mut count = 0;
        for (entity, code) in events {
            let Some(node) = engine.node(entity).filter(|n| !n.style.disabled) else {
                continue;
            };
            if matches!(node.kind, Kind::Button | Kind::ButtonBase)
                && code == sys::lv_event_code_t_LV_EVENT_CLICKED
            {
                engine.queue_click(entity);
                count += 1;
            } else if node.kind == Kind::TextInput {
                let Some((buffer, _)) = engine.input(entity) else {
                    continue;
                };
                if code == sys::lv_event_code_t_LV_EVENT_VALUE_CHANGED {
                    let obj = self.nodes[&entity].widget.object();
                    let text = unsafe { std::ffi::CStr::from_ptr(sys::lv_textarea_get_text(obj)) }
                        .to_string_lossy();
                    if !buffer.with_value(|old| old == text) {
                        buffer.set(text.as_ref().to_owned().into());
                        count += 1;
                    }
                } else if code == sys::lv_event_code_t_LV_EVENT_FOCUSED
                    || code == sys::lv_event_code_t_LV_EVENT_CLICKED
                {
                    self.open_keyboard(entity);
                    count += 1;
                } else if code == sys::lv_event_code_t_LV_EVENT_READY
                    || code == sys::lv_event_code_t_LV_EVENT_CANCEL
                {
                    if code == sys::lv_event_code_t_LV_EVENT_READY {
                        buffer.edit(plico_core::InputEdit::Submit);
                    }
                    self.close_keyboard();
                    count += 1;
                } else if code == sys::lv_event_code_t_LV_EVENT_DEFOCUSED
                    && self.editing == Some(entity)
                {
                    self.close_keyboard();
                    count += 1;
                }
            }
        }
        if let Some(entity) = engine.take_focus_request()
            && self.nodes.get(&entity).is_some_and(|n| n.widget.editor)
        {
            self.open_keyboard(entity);
            count += 1;
        }
        count
    }
    fn open_keyboard(&mut self, entity: Entity) {
        unsafe {
            if self.keyboard.is_null() {
                self.keyboard = sys::lv_keyboard_create(self.screen);
                if self.keyboard.is_null() {
                    return;
                }
                sys::lv_obj_set_size(self.keyboard, pct(100), pct(42));
                sys::lv_obj_set_style_text_font(
                    self.keyboard,
                    &sys::lv_font_montserrat_24,
                    sys::lv_part_t_LV_PART_ITEMS,
                );
                sys::lv_obj_align(self.keyboard, sys::lv_align_t_LV_ALIGN_BOTTOM_MID, 0, 0);
            }
            sys::lv_keyboard_set_textarea(self.keyboard, self.nodes[&entity].widget.object());
            sys::lv_obj_remove_flag(self.keyboard, sys::lv_obj_flag_t_LV_OBJ_FLAG_HIDDEN);
            sys::lv_obj_move_to_index(self.keyboard, -1);
            self.editing = Some(entity);
        }
    }
    fn close_keyboard(&mut self) {
        if !self.keyboard.is_null() {
            unsafe {
                sys::lv_keyboard_set_textarea(self.keyboard, std::ptr::null_mut());
                sys::lv_obj_add_flag(self.keyboard, sys::lv_obj_flag_t_LV_OBJ_FLAG_HIDDEN);
            }
        }
        self.editing = None;
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
        if self.editing == Some(entity) {
            self.close_keyboard();
        }
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
        self.close_keyboard();
        if !self.keyboard.is_null() {
            ROUTES.with(|r| r.borrow_mut().remove(&(self.keyboard as usize)));
            unsafe {
                sys::lv_obj_delete(self.keyboard);
            }
        }
        let ids: Vec<Entity> = self.nodes.keys().copied().collect();
        // Deleting a parent also deletes native children. HashMap iteration
        // order would therefore occasionally delete a dangling child pointer.
        for id in self.children_first(ids) {
            self.unmount(id);
        }
    }
}

fn sync_svg(widget: &mut Widget, node: &NodeRef<'_>, theme: Theme) -> Result<usize, Error> {
    if !widget.is_svg || widget.svg.as_ref().is_some_and(|r| r.matches(node, theme)) {
        return Ok(0);
    }
    let raster = svg::Raster::new(node, theme)?;
    unsafe { sys::lv_image_set_src(widget.obj, raster.image() as *const _) };
    widget.svg = Some(raster);
    Ok(1)
}

// Native editors own their LVGL text; snapshots retain only the accessible label.
fn sync_editor(widget: &Widget, engine: &Engine, entity: Entity) -> Result<(), Error> {
    if !widget.editor {
        return Ok(());
    }
    let Some((buffer, hint)) = engine.input(entity) else {
        return Ok(());
    };
    snafu::ensure!(
        !hint.contains('\0') && buffer.with_value(|v| !v.contains('\0')),
        TextNulSnafu
    );
    unsafe {
        with_cstr(hint, |p| {
            sys::lv_textarea_set_placeholder_text(widget.obj, p)
        });
        let visible = engine
            .node(entity)
            .is_some_and(|n| n.style.input_password_visible);
        sys::lv_textarea_set_password_mode(widget.obj, buffer.is_secret() && !visible);
        sys::lv_textarea_set_password_show_time(widget.obj, 0);
        buffer.with_value(|value| {
            let current =
                std::ffi::CStr::from_ptr(sys::lv_textarea_get_text(widget.obj)).to_bytes();
            if current != value.as_bytes() {
                with_cstr(value, |p| sys::lv_textarea_set_text(widget.obj, p));
            }
        });
    }
    Ok(())
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

fn apply_scroll(obj: *mut Obj, enabled: bool) {
    unsafe {
        if enabled {
            sys::lv_obj_add_flag(
                obj,
                sys::lv_obj_flag_t_LV_OBJ_FLAG_SCROLLABLE
                    | sys::lv_obj_flag_t_LV_OBJ_FLAG_CLICKABLE,
            );
            sys::lv_obj_set_scroll_dir(obj, sys::lv_dir_t_LV_DIR_VER as _);
            sys::lv_obj_set_scrollbar_mode(obj, sys::lv_scrollbar_mode_t_LV_SCROLLBAR_MODE_AUTO);
        } else {
            // Generic layout boxes must not steal presses from their enclosing
            // button. Scroll containers remain clickable to start gestures on
            // empty space; buttons and editors retain their native click flags.
            let mut flags = sys::lv_obj_flag_t_LV_OBJ_FLAG_SCROLLABLE;
            if sys::lv_obj_check_type(obj, &sys::lv_obj_class) {
                flags |= sys::lv_obj_flag_t_LV_OBJ_FLAG_CLICKABLE;
            }
            sys::lv_obj_remove_flag(obj, flags);
            sys::lv_obj_scroll_to_y(obj, 0, false);
        }
    }
}

fn preflight(node: &NodeRef<'_>) -> Result<(), Error> {
    snafu::ensure!(
        projection::class(node.kind).is_some(),
        UnsupportedKindSnafu { kind: node.kind }
    );
    snafu::ensure!(!node.text.contains('\0'), TextNulSnafu);
    Ok(())
}

fn apply_style(obj: *mut Obj, node: &NodeRef<'_>, theme: Theme) {
    let s = node.style;
    let container_like = projection::uses_flex(node.kind);
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
        let pad_y = style_diff::vertical_padding(node.kind, s);
        sys::lv_style_set_pad_top(style, pad_y);
        sys::lv_style_set_pad_bottom(style, pad_y);
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
        sys::lv_style_set_translate_x(style, s.translate[0].round() as i32);
        sys::lv_style_set_translate_y(style, s.translate[1].round() as i32);
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
        apply_scroll(obj, s.scroll_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_core::{DeclareView, Length, prelude::*};

    struct View {
        height: Length,
        scroll_y: bool,
        fill: Option<plico_core::Color>,
        alpha: f32,
        opacity: f32,
        translate: [f32; 2],
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
                .scroll_y(self.scroll_y)
                .padding(7)
                .radius(6.)
                .background_alpha(self.alpha)
                .opacity(self.opacity)
                .translate(self.translate[0], self.translate[1]);
            if let Some(fill) = self.fill {
                panel = panel.background(fill);
            }
            panel.children(|cx| {
                text(cx, "Retained child");
            });
        }
    }
    fn frame(engine: &mut Engine, view: &mut impl DeclareView, adapter: &mut Adapter) {
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
            scroll_y: false,
            fill: Some(plico_core::Color::Accent),
            alpha: 1.,
            opacity: 1.,
            translate: [0., 0.],
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
        view.scroll_y = true;
        frame(&mut engine, &mut view, &mut adapter);
        assert!(unsafe { sys::lv_obj_has_flag(obj, sys::lv_obj_flag_t_LV_OBJ_FLAG_SCROLLABLE) });
        assert_eq!(adapter.stats().style_resets, 0);
        view.scroll_y = false;
        frame(&mut engine, &mut view, &mut adapter);
        assert!(!unsafe { sys::lv_obj_has_flag(obj, sys::lv_obj_flag_t_LV_OBJ_FLAG_SCROLLABLE) });
        view.translate = [24., -8.];
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(
            unsafe { number(obj, sys::_lv_style_id_t_LV_STYLE_TRANSLATE_X as _) },
            24
        );
        assert_eq!(
            unsafe { number(obj, sys::_lv_style_id_t_LV_STYLE_TRANSLATE_Y as _) },
            -8
        );
        assert_eq!(adapter.stats().style_resets, 0);
        view.translate = [0., 0.];
        frame(&mut engine, &mut view, &mut adapter);
        assert_eq!(
            unsafe { number(obj, sys::_lv_style_id_t_LV_STYLE_TRANSLATE_X as _) },
            0
        );
        assert_eq!(
            unsafe { number(obj, sys::_lv_style_id_t_LV_STYLE_TRANSLATE_Y as _) },
            0
        );
        // Exercise the real native textarea/keyboard path in the same LVGL instance.
        struct Editors {
            height: i32,
            font_size: u16,
            name: plico_core::TextBuffer,
            password: plico_core::TextBuffer,
            reveal: bool,
            visible: bool,
        }
        impl DeclareView for Editors {
            fn theme(&self) -> Theme {
                Theme::default()
            }
            fn show(&mut self, cx: &mut UiContext) {
                if self.visible {
                    column(cx).width(400).children(|cx| {
                        text_input(cx, "Network", &self.name)
                            .width(350)
                            .height(self.height)
                            .font_size(self.font_size)
                            .placeholder("SSID");
                        text_input(cx, "Password", &self.password)
                            .width(350)
                            .height(self.height)
                            .font_size(self.font_size)
                            .password_visible(self.reveal);
                    });
                }
            }
        }
        let mut editors = Editors {
            height: 54,
            font_size: 16,
            name: Default::default(),
            password: plico_core::TextBuffer::secret(),
            reveal: false,
            visible: true,
        };
        engine.declare_until_settled(&mut editors).unwrap();
        adapter.sync(&engine).unwrap();
        adapter.dispatch(&mut engine);
        let root = engine.node(engine.root()).unwrap().children[0];
        let ids = engine.node(root).unwrap().children.to_vec();
        let name_obj = adapter.nodes[&ids[0]].widget.object();
        let password_obj = adapter.nodes[&ids[1]].widget.object();
        let assert_centered = |obj| unsafe {
            sys::lv_obj_update_layout(obj);
            let mut outer = sys::lv_area_t {
                x1: 0,
                y1: 0,
                x2: 0,
                y2: 0,
            };
            let mut label = sys::lv_area_t {
                x1: 0,
                y1: 0,
                x2: 0,
                y2: 0,
            };
            sys::lv_obj_get_coords(obj, &mut outer);
            sys::lv_obj_get_coords(sys::lv_textarea_get_label(obj), &mut label);
            assert!(
                ((label.y1 + label.y2) - (outer.y1 + outer.y2)).abs() <= 1,
                "editor line must be centered: {outer:?} {label:?}"
            );
        };
        // Empty input exercises the placeholder's shared content origin.
        assert_centered(name_obj);
        assert_centered(password_obj);
        editors.height = 62;
        editors.font_size = 24;
        engine.declare_until_settled(&mut editors).unwrap();
        let delta = engine.take_frame_delta();
        adapter.sync_delta(&engine, Some(delta)).unwrap();
        assert_centered(name_obj);
        assert_centered(password_obj);
        assert_eq!(adapter.stats().style_resets, 0);
        unsafe {
            with_cstr("café", |p| sys::lv_textarea_set_text(name_obj, p));
            with_cstr("s3cret!", |p| sys::lv_textarea_set_text(password_obj, p));
        }
        assert_eq!(adapter.dispatch(&mut engine), 2);
        assert_eq!(editors.name.text().as_deref(), Some("café"));
        assert!(editors.password.text().is_none());
        assert_eq!(editors.password.presentation().text, "•••••••");
        assert!(unsafe { sys::lv_textarea_get_password_mode(password_obj) });
        assert_centered(name_obj);
        assert_centered(password_obj);
        editors.reveal = true;
        engine.declare_until_settled(&mut editors).unwrap();
        let delta = engine.take_frame_delta();
        adapter.sync_delta(&engine, Some(delta)).unwrap();
        assert!(!unsafe { sys::lv_textarea_get_password_mode(password_obj) });
        assert!(
            editors.password.text().is_none(),
            "reveal does not change snapshot secrecy"
        );
        unsafe {
            sys::lv_obj_send_event(
                password_obj,
                sys::lv_event_code_t_LV_EVENT_CLICKED,
                std::ptr::null_mut(),
            );
        }
        adapter.dispatch(&mut engine);
        assert_eq!(adapter.editing, Some(ids[1]));
        assert_eq!(
            unsafe { sys::lv_keyboard_get_textarea(adapter.keyboard) },
            password_obj
        );
        unsafe {
            sys::lv_obj_send_event(
                password_obj,
                sys::lv_event_code_t_LV_EVENT_READY,
                std::ptr::null_mut(),
            );
        }
        adapter.dispatch(&mut engine);
        assert_eq!(editors.password.submitted_count(), 1);
        assert!(adapter.editing.is_none());
        adapter.open_keyboard(ids[1]);
        editors.visible = false;
        engine.declare_until_settled(&mut editors).unwrap();
        let delta = engine.take_frame_delta();
        adapter.sync_delta(&engine, Some(delta)).unwrap();
        assert!(adapter.editing.is_none());
        assert!(unsafe { sys::lv_keyboard_get_textarea(adapter.keyboard) }.is_null());
        // Stale events must not dereference a deleted input.
        adapter
            .events
            .borrow_mut()
            .push((ids[1], sys::lv_event_code_t_LV_EVENT_VALUE_CHANGED));
        assert_eq!(adapter.dispatch(&mut engine), 0);
        struct IconView {
            color: plico_core::Color,
            size: f32,
            source: String,
            visible: bool,
        }
        impl DeclareView for IconView {
            fn theme(&self) -> Theme {
                Theme::default()
            }
            fn show(&mut self, cx: &mut UiContext) {
                if self.visible {
                    svg_source(cx, self.source.clone())
                        .size(self.size, self.size)
                        .color(self.color);
                }
            }
        }
        let mut icons = IconView {
            color: plico_core::Color::Literal(0xff0000), size: 24., visible: true,
            source: "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path d=\"M4 4H20V20H4Z\" fill=\"currentColor\"/></svg>".into(),
        };
        frame(&mut engine, &mut icons, &mut adapter);
        assert_eq!(adapter.stats().svg_rasterizations, 1);
        let id = engine.node(engine.root()).unwrap().children[0];
        let object = adapter.object_id(id);
        let image_pixel = |adapter: &Adapter| unsafe {
            let source = sys::lv_image_get_src(adapter.nodes[&id].widget.object())
                as *const sys::lv_image_dsc_t;
            let image = &*source;
            let offset =
                (image.header.h() / 2 * image.header.stride() + image.header.w() / 2 * 4) as usize;
            let bytes = std::slice::from_raw_parts(image.data.add(offset), 4);
            [bytes[0], bytes[1], bytes[2], bytes[3]]
        };
        let assert_pixel = |actual: [u8; 4], expected: [u8; 4]| {
            for (a, e) in actual.into_iter().zip(expected) {
                assert!(a.abs_diff(e) <= 1, "pixel: {actual:?} != {expected:?}");
            }
        };
        assert_pixel(image_pixel(&adapter), [0, 0, 255, 255]);
        assert_eq!(adapter.svg_cache_bytes(), 24 * 24 * 4);
        unsafe {
            let image = &*(sys::lv_image_get_src(adapter.nodes[&id].widget.object())
                as *const sys::lv_image_dsc_t);
            assert_eq!((image.header.w(), image.header.h()), (24, 24));
            assert_eq!(*image.data.add(3), 0, "outside SVG path stays transparent");
        }
        let source = unsafe { sys::lv_image_get_src(adapter.nodes[&id].widget.object()) };
        frame(&mut engine, &mut icons, &mut adapter);
        assert_eq!(adapter.stats().svg_rasterizations, 0);
        assert_eq!(source, unsafe {
            sys::lv_image_get_src(adapter.nodes[&id].widget.object())
        });
        icons.color = plico_core::Color::Literal(0x00ff00);
        frame(&mut engine, &mut icons, &mut adapter);
        assert_eq!(adapter.stats().svg_rasterizations, 1);
        assert_eq!(adapter.object_id(id), object);
        assert_eq!(adapter.stats().style_resets, 0);
        assert_pixel(image_pixel(&adapter), [0, 255, 0, 255]);
        icons.size = 32.;
        frame(&mut engine, &mut icons, &mut adapter);
        assert_eq!(adapter.stats().svg_rasterizations, 1);
        assert_pixel(image_pixel(&adapter), [0, 255, 0, 255]);
        assert_eq!(adapter.svg_cache_bytes(), 32 * 32 * 4);
        icons.source = icons.source.replace("currentColor", "#0000ff");
        frame(&mut engine, &mut icons, &mut adapter);
        assert_pixel(image_pixel(&adapter), [255, 0, 0, 255]);
        icons.visible = false;
        frame(&mut engine, &mut icons, &mut adapter);
        assert!(adapter.object_id(id).is_none());
        assert_eq!(adapter.svg_cache_bytes(), 0);
        struct Navigation {
            clicked: bool,
        }
        impl DeclareView for Navigation {
            fn theme(&self) -> Theme {
                Theme::default()
            }
            fn show(&mut self, cx: &mut UiContext) {
                self.clicked |= button_base(cx, "Home")
                    .width(104)
                    .height(56)
                    .padding(0)
                    .children(|cx| {
                        column(cx)
                            .width(104)
                            .height(56)
                            .align(Align::Center)
                            .justify(Justify::Center)
                            .children(|cx| {
                                icon(cx, &plico_core::icons::CHECK).size(24., 24.);
                                text(cx, "Home");
                            });
                    })
                    .response
                    .clicked();
            }
        }
        let mut navigation = Navigation { clicked: false };
        frame(&mut engine, &mut navigation, &mut adapter);
        let button = engine.node(engine.root()).unwrap().children[0];
        let column = engine.node(button).unwrap().children[0];
        let button_obj = adapter.nodes[&button].widget.object();
        let child_ids = engine.node(column).unwrap().children.to_vec();
        for child in child_ids {
            let area = adapter.bounds(child).unwrap();
            let mut point = sys::lv_point_t {
                x: (area[0] + area[2]) / 2,
                y: (area[1] + area[3]) / 2,
            };
            let hit = unsafe { sys::lv_indev_search_obj(adapter.screen, &mut point) };
            assert_eq!(hit, button_obj, "icon/text must hit the surrounding button");
        }
        unsafe {
            sys::lv_obj_send_event(
                button_obj,
                sys::lv_event_code_t_LV_EVENT_CLICKED,
                std::ptr::null_mut(),
            );
        }
        adapter.dispatch(&mut engine);
        frame(&mut engine, &mut navigation, &mut adapter);
        assert!(navigation.clicked);
        // A kind change is structural even if the core Entity and its text
        // child survive. Remount both native objects, children before parents.
        let mut kinds = Engine::default();
        let mut kind_adapter = Adapter::new(&driver).unwrap();
        struct KindView {
            kind: Kind,
            child: bool,
        }
        impl DeclareView for KindView {
            fn theme(&self) -> Theme {
                Theme::default()
            }
            fn show(&mut self, cx: &mut UiContext<'_>) {
                cx.element(
                    plico_core::Key::Static("changing"),
                    self.kind,
                    Style::default(),
                    "Action",
                    |cx| {
                        if self.child {
                            text(cx, "Kept child").key("child");
                        }
                    },
                );
            }
        }
        let declare_kind = |engine: &mut Engine, kind: Kind, child: bool| {
            engine
                .declare_until_settled(&mut KindView { kind, child })
                .unwrap();
        };
        declare_kind(&mut kinds, Kind::Row, true);
        kind_adapter.sync(&kinds).unwrap();
        kinds.take_frame_delta();
        let parent = kinds.node(kinds.root()).unwrap().children[0];
        let child = kinds.node(parent).unwrap().children[0];
        declare_kind(&mut kinds, Kind::ButtonBase, true);
        let delta = kinds.take_frame_delta();
        assert!(delta.frame.contains(Changes::STRUCTURE));
        kind_adapter.sync_delta(&kinds, Some(delta)).unwrap();
        assert_eq!(kinds.node(parent).unwrap().children, &[child]);
        assert_eq!(kind_adapter.stats().removed, 2);
        assert_eq!(kind_adapter.stats().created, 2);
        assert!(unsafe {
            sys::lv_obj_check_type(
                kind_adapter.nodes[&parent].widget.object(),
                &sys::lv_button_class,
            )
        });
        assert!(unsafe {
            sys::lv_obj_check_type(
                kind_adapter.nodes[&child].widget.object(),
                &sys::lv_label_class,
            )
        });
        kind_adapter.send_click(parent);
        declare_kind(&mut kinds, Kind::Button, false);
        let delta = kinds.take_frame_delta();
        kind_adapter.sync_delta(&kinds, Some(delta)).unwrap();
        assert_eq!(
            kind_adapter.dispatch(&mut kinds),
            0,
            "drop queued clicks from old native instance"
        );
        kind_adapter.send_click(parent);
        assert_eq!(kind_adapter.dispatch(&mut kinds), 1);

        // Preflight covers the whole tree/delta before touching retained widgets.
        let mut preflight_engine = Engine::default();
        let mut preflight_adapter = Adapter::new(&driver).unwrap();
        preflight_engine
            .frame_declarations(|cx| {
                text(cx, "first").key("first");
                text(cx, "safe").key("second");
            })
            .unwrap();
        preflight_adapter.sync(&preflight_engine).unwrap();
        preflight_engine.take_frame_delta();
        let root_object = preflight_adapter.object_id(preflight_engine.root());
        let first = preflight_engine
            .node(preflight_engine.root())
            .unwrap()
            .children[0];
        let label = preflight_adapter.nodes[&first].widget.object();
        preflight_engine
            .frame_declarations(|cx| {
                text(cx, "must not be applied").key("first");
                text(cx, "invalid\0label").key("second");
            })
            .unwrap();
        let delta = preflight_engine.take_frame_delta();
        assert!(!delta.frame.contains(Changes::STRUCTURE));
        assert!(matches!(
            preflight_adapter.sync_delta(&preflight_engine, Some(delta)),
            Err(Error::TextNul)
        ));
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(sys::lv_label_get_text(label)) }
                .to_str()
                .unwrap(),
            "first"
        );
        preflight_engine
            .frame_declarations(|cx| {
                text(cx, "must not be applied").key("first");
                plico_core::widgets::checkbox(cx, "Unsupported", &mut false);
            })
            .unwrap();
        assert!(matches!(
            preflight_adapter.sync(&preflight_engine),
            Err(Error::UnsupportedKind {
                kind: Kind::Checkbox
            })
        ));
        assert_eq!(
            preflight_adapter.object_id(preflight_engine.root()),
            root_object
        );
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(sys::lv_label_get_text(label)) }
                .to_str()
                .unwrap(),
            "first"
        );
        // Drop also exercises child-before-parent destruction of the native tree.
    }
}
