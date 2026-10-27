//! FLTK owns controls and interaction. Taffy owns geometry; Plico owns identity.
use fltk::{
    app,
    button::{Button, CheckButton},
    draw,
    enums::{Align as FAlign, CallbackTrigger, Color as FColor, Font, FrameType},
    frame::Frame,
    group::{Group, Scroll},
    image::SvgImage,
    input::Input,
    prelude::*,
    widget::Widget,
    window::Window,
};
use plico_core::taffy_layout::style as layout_style;
use plico_core::{Clock, DeclareView, Engine, Entity, FrameTime, Kind, Style, Theme};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use taffy::prelude::*;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Plico declarations failed: {source}"))]
    Frame { source: plico_core::FrameError },
    #[snafu(display("Taffy layout failed: {source}"))]
    Layout { source: taffy::TaffyError },
    #[snafu(display("FLTK failed: {source}"))]
    Fltk { source: fltk::prelude::FltkError },
    #[snafu(display("FLTK experiment does not support {kind:?}"))]
    Unsupported { kind: Kind },
    #[snafu(display("secret inputs are not supported by this experiment"))]
    SecretInput,
    #[snafu(display("FLTK button content currently supports text and one SVG icon only"))]
    ButtonContent,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Callbacks carry identities and values, never application state or UI borrows.
#[derive(Clone, Debug)]
pub enum Event {
    Activate(Entity),
    Edit(Entity, String),
}
#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub created: usize,
    pub removed: usize,
    pub layout_calls: usize,
    pub geometry_writes: usize,
    pub property_writes: usize,
}
#[derive(Clone, PartialEq)]
struct Measure {
    text: String,
    font: Font,
    size: i32,
    kind: Kind,
}
struct Retained {
    layout: NodeId,
    parent: Option<Entity>,
    kind: Kind,
    widget: Option<Widget>,
    check: Option<CheckButton>,
    input: Option<Input>,
    scroll: Option<Scroll>,
    style: Style,
    text: String,
    theme: Theme,
    image: Option<SvgImage>,
    icon_source: Option<String>,
    children: Vec<Entity>,
    measure: Measure,
}
/// Construct after initializing FLTK, on the UI thread. Native child widgets are
/// owned by `root`; dropping the adapter alone does not destroy the parent group.
pub struct Adapter {
    root: Group,
    tree: TaffyTree<Measure>,
    nodes: HashMap<Entity, Retained>,
    sender: app::Sender<Event>,
    pub stats: Stats,
}
fn color(token: plico_core::Color, theme: Theme) -> FColor {
    let rgb = theme.resolve(token);
    FColor::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
fn font(style: Style) -> Font {
    if style.font_weight >= 600 {
        Font::HelveticaBold
    } else {
        Font::Helvetica
    }
}
impl Adapter {
    pub fn new(root: Group, sender: app::Sender<Event>) -> Self {
        Self {
            root,
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            sender,
            stats: Stats::default(),
        }
    }
    pub fn native_widget(&self, entity: Entity) -> Option<Widget> {
        self.nodes.get(&entity)?.widget.clone()
    }
    /// Native editor handle for embedding/testing; application UI stays backend-free.
    pub fn native_input(&self, entity: Entity) -> Option<Input> {
        self.nodes.get(&entity)?.input.clone()
    }
    pub fn native_scroll(&self, entity: Entity) -> Option<Scroll> {
        self.nodes.get(&entity)?.scroll.clone()
    }
    pub fn bounds(&self, entity: Entity) -> Option<[i32; 4]> {
        self.native_widget(entity)
            .map(|w| [w.x(), w.y(), w.w(), w.h()])
    }
    /// Route a native message only if the originating node is still live/enabled.
    pub fn dispatch(engine: &mut Engine, event: Event) -> bool {
        let entity = match &event {
            Event::Activate(e) | Event::Edit(e, _) => *e,
        };
        let Some(node) = engine.node(entity) else {
            return false;
        };
        if node.style.disabled {
            return false;
        }
        match event {
            Event::Activate(e)
                if matches!(
                    node.kind,
                    Kind::Button | Kind::ButtonBase | Kind::Switch | Kind::Checkbox
                ) =>
            {
                engine.queue_click(e)
            }
            Event::Edit(e, value) if node.kind == Kind::TextInput => {
                let Some((buffer, _)) = engine.input(e) else {
                    return false;
                };
                if buffer.is_secret() {
                    return false;
                }
                buffer.set(value.into());
            }
            _ => return false,
        }
        true
    }
    pub fn sync(&mut self, engine: &Engine) -> Result<()> {
        // Validate first: no partially mounted unsupported scene.
        Self::validate(engine, engine.root())?;
        self.stats = Stats::default();
        // Kind changes at an explicitly reused key require a native remount.
        if self.nodes.iter().any(|(e, old)| {
            engine
                .node(*e)
                .is_some_and(|n| n.kind != old.kind || n.style.scroll_y != old.style.scroll_y)
        }) {
            self.stats.removed = self.nodes.len();
            self.root.clear();
            self.nodes.clear();
            self.tree.clear();
        }
        let mut live = HashSet::new();
        let root = self.sync_node(engine, engine.root(), None, false, &mut live)?;
        // Delete children before parents. FLTK groups own their descendants;
        // removing random handles from a HashMap would double-delete them.
        let mut stale: Vec<_> = self
            .nodes
            .keys()
            .filter(|e| !live.contains(e))
            .copied()
            .collect();
        stale.sort_by_key(|e| std::cmp::Reverse(self.depth(*e)));
        for entity in stale {
            let n = self.nodes.remove(&entity).unwrap();
            if let Some(w) = n.widget
                && entity != engine.root()
            {
                app::delete_widget(w);
            }
            self.tree.remove(n.layout).context(LayoutSnafu)?;
            self.stats.removed += 1;
        }
        self.tree
            .compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(self.root.w() as f32),
                    height: AvailableSpace::Definite(self.root.h() as f32),
                },
                |input, _, context, style| {
                    taffy::compute_leaf_layout(
                        input,
                        style,
                        |_, _| 0.,
                        |known, available| {
                            let Some(m) = context else {
                                return Size::ZERO;
                            };
                            draw::set_font(m.font, m.size);
                            let limit = known.width.or(match available.width {
                                AvailableSpace::Definite(v) => Some(v),
                                _ => None,
                            });
                            let (w, h) = if m.kind == Kind::Text {
                                draw::wrap_measure(
                                    &m.text,
                                    limit.unwrap_or(0.).max(0.) as i32,
                                    false,
                                )
                            } else {
                                draw::measure(&m.text, false)
                            };
                            let extra = if matches!(m.kind, Kind::Switch | Kind::Checkbox) {
                                24.
                            } else {
                                0.
                            };
                            Size {
                                width: known.width.unwrap_or(
                                    w as f32 + extra + if m.kind == Kind::Text { 6. } else { 0. },
                                ),
                                height: known.height.unwrap_or((h.max(draw::height())) as f32),
                            }
                        },
                    )
                },
            )
            .context(LayoutSnafu)?;
        self.stats.layout_calls = 1;
        self.place(engine, engine.root(), self.root.x(), self.root.y())?;
        Ok(())
    }
    fn depth(&self, entity: Entity) -> usize {
        // A removed branch can no longer be reached through the new live tree.
        // Count all retained incoming ancestors instead.
        let mut depth = 0;
        let mut current = entity;
        while let Some(parent) = self.nodes.get(&current).and_then(|n| n.parent) {
            depth += 1;
            current = parent;
        }
        depth
    }
    fn validate(engine: &Engine, entity: Entity) -> Result<()> {
        let n = engine.node(entity).unwrap();
        if !matches!(
            n.kind,
            Kind::Root
                | Kind::Column
                | Kind::Row
                | Kind::Text
                | Kind::Button
                | Kind::ButtonBase
                | Kind::Switch
                | Kind::Checkbox
                | Kind::TextInput
                | Kind::Svg
        ) {
            return Err(Error::Unsupported { kind: n.kind });
        }
        if engine.input(entity).is_some_and(|(b, _)| b.is_secret()) {
            return Err(Error::SecretInput);
        }
        if n.kind == Kind::ButtonBase {
            let mut icons = 0;
            for &child in n.children {
                let child = engine.node(child).unwrap();
                match child.kind {
                    Kind::Text => {}
                    Kind::Svg => icons += 1,
                    _ => return Err(Error::ButtonContent),
                }
            }
            if icons > 1 {
                return Err(Error::ButtonContent);
            }
        }
        for &child in n.children {
            Self::validate(engine, child)?;
        }
        Ok(())
    }
    fn sync_node(
        &mut self,
        engine: &Engine,
        entity: Entity,
        parent: Option<Entity>,
        in_button: bool,
        live: &mut HashSet<Entity>,
    ) -> Result<NodeId> {
        live.insert(entity);
        let n = engine.node(entity).unwrap();
        let theme = engine.theme();
        let measure = Measure {
            text: n.text.into(),
            font: font(n.style),
            size: n.style.font_size as i32,
            kind: n.kind,
        };
        let mut layout_style = layout_style(n.kind, n.style, n.grid);
        if entity == engine.root() {
            layout_style.size = Size {
                width: length(self.root.w() as f32),
                height: length(self.root.h() as f32),
            };
        }
        let is_new = !self.nodes.contains_key(&entity);
        if is_new {
            let id = self
                .tree
                .new_leaf_with_context(layout_style.clone(), measure.clone())
                .context(LayoutSnafu)?;
            let mut check = None;
            let mut input = None;
            let mut native_scroll = None;
            let widget = if entity == engine.root() {
                Some(self.root.as_base_widget())
            } else if in_button {
                None
            } else {
                let group = parent
                    .and_then(|p| self.nodes.get(&p))
                    .and_then(|n| n.widget.as_ref())
                    .and_then(|w| w.as_group())
                    .unwrap_or(self.root.clone());
                group.begin();
                let widget = match n.kind {
                    Kind::Row | Kind::Column if n.style.scroll_y => {
                        let mut scroll = Scroll::default();
                        scroll.end();
                        scroll.set_type(fltk::group::ScrollType::Vertical);
                        let base = scroll.as_base_widget();
                        native_scroll = Some(scroll);
                        base
                    }
                    Kind::Row | Kind::Column => {
                        let g = Group::default();
                        g.end();
                        g.as_base_widget()
                    }
                    Kind::Button | Kind::ButtonBase => {
                        let mut w = Button::default();
                        let tx = self.sender;
                        w.set_callback(move |_| tx.send(Event::Activate(entity)));
                        w.as_base_widget()
                    }
                    Kind::Switch | Kind::Checkbox => {
                        let mut w = CheckButton::default();
                        let tx = self.sender;
                        w.set_callback(move |_| tx.send(Event::Activate(entity)));
                        let base = w.as_base_widget();
                        check = Some(w);
                        base
                    }
                    Kind::TextInput => {
                        let mut w = Input::default();
                        let tx = self.sender;
                        w.set_trigger(CallbackTrigger::Changed);
                        w.set_callback(move |w| tx.send(Event::Edit(entity, w.value())));
                        let base = w.as_base_widget();
                        input = Some(w);
                        base
                    }
                    _ => Frame::default().as_base_widget(),
                };
                group.end();
                Some(widget)
            };
            self.nodes.insert(
                entity,
                Retained {
                    layout: id,
                    parent,
                    kind: n.kind,
                    widget,
                    check,
                    input,
                    scroll: native_scroll,
                    style: n.style,
                    text: String::new(),
                    theme,
                    image: None,
                    icon_source: None,
                    children: Vec::new(),
                    measure: measure.clone(),
                },
            );
            self.stats.created += 1;
        }
        // An explicit accessible name is metadata, not visible button text.
        // In particular, icon-only Back/Forward controls must stay icon-only.
        let label = if n.kind == Kind::ButtonBase {
            n.children
                .iter()
                .filter_map(|e| engine.node(*e))
                .filter(|child| child.kind == Kind::Text)
                .map(|child| child.text)
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            n.text.to_owned()
        };
        let icon = if n.kind == Kind::ButtonBase {
            n.children
                .iter()
                .filter_map(|e| engine.node(*e))
                .find(|n| n.kind == Kind::Svg)
        } else if n.kind == Kind::Svg {
            engine.node(entity)
        } else {
            None
        };
        let icon_source = icon.map(|icon| {
            icon.text.replace(
                "currentColor",
                &format!("#{:06x}", theme.resolve(icon.style.foreground)),
            )
        });
        let native = self.nodes.get_mut(&entity).unwrap();
        if self.tree.style(native.layout).context(LayoutSnafu)? != &layout_style {
            self.tree
                .set_style(native.layout, layout_style)
                .context(LayoutSnafu)?;
        }
        if native.measure != measure {
            self.tree
                .set_node_context(native.layout, Some(measure.clone()))
                .context(LayoutSnafu)?;
            self.tree.mark_dirty(native.layout).context(LayoutSnafu)?;
            native.measure = measure;
        }
        if native.text != label
            || native.icon_source != icon_source
            || native.style != n.style
            || native.theme != theme
            || is_new
        {
            if let Some(w) = &mut native.widget {
                w.set_label(&label);
                w.set_label_size(n.style.font_size as i32);
                w.set_label_font(font(n.style));
                w.set_label_color(color(n.style.foreground, theme));
                w.set_color(color(
                    n.style.background.unwrap_or(plico_core::Color::Surface),
                    theme,
                ));
                w.set_selection_color(color(plico_core::Color::Accent, theme));
                w.set_frame(
                    if matches!(n.kind, Kind::Button | Kind::ButtonBase | Kind::TextInput) {
                        FrameType::BorderBox
                    } else if n.style.background.is_some() {
                        FrameType::FlatBox
                    } else {
                        FrameType::NoBox
                    },
                );
                w.set_align(
                    FAlign::Inside
                        | FAlign::Left
                        | FAlign::Clip
                        | if n.kind == Kind::Text {
                            FAlign::Wrap
                        } else {
                            FAlign::empty()
                        },
                );
                if let Some(mut group) = w.as_group() {
                    group.set_clip_children(n.style.clip || n.style.scroll_y);
                }
                if n.style.disabled {
                    w.deactivate();
                } else {
                    w.activate();
                }
                if n.style.layout.display == plico_core::Display::None {
                    w.hide();
                } else {
                    w.show();
                }
                if matches!(n.kind, Kind::Button | Kind::ButtonBase) {
                    w.set_tooltip(engine.accessible_name(entity).as_deref().unwrap_or(""));
                    w.set_align(
                        FAlign::Inside | FAlign::Center | FAlign::Clip | FAlign::ImageNextToText,
                    );
                }
                if let Some(check) = &mut native.check {
                    check.set_value(n.style.checked);
                }
                if let Some(input) = &mut native.input {
                    input.set_label("");
                    input.set_text_size(n.style.font_size as i32);
                    input.set_text_font(font(n.style));
                    input.set_text_color(color(n.style.foreground, theme));
                }
                if let Some(source) = &icon_source {
                    let mut image = SvgImage::from_data(source).context(FltkSnafu)?;
                    if n.kind == Kind::ButtonBase {
                        let side = i32::from(n.style.font_size) + 2;
                        image.scale(side, side, true, true);
                        w.set_image(Some(image.clone()));
                    } else {
                        w.set_label("");
                        w.set_align(
                            FAlign::Inside
                                | FAlign::Center
                                | FAlign::Clip
                                | FAlign::ImageNextToText,
                        );
                    }
                    native.image = Some(image);
                } else if native.icon_source.is_some() {
                    w.set_image(None::<SvgImage>);
                    native.image = None;
                }
            }
            native.text = label;
            native.icon_source = icon_source;
            native.style = n.style;
            native.theme = theme;
            self.stats.property_writes += 1;
        }
        if let (Some(input), Some((buffer, hint))) = (&mut native.input, engine.input(entity)) {
            buffer.with_value(|value| {
                if input.value() != value {
                    input.set_value(value);
                }
            });
            input.set_tooltip(hint);
        }
        let id = native.layout;
        let mut children = Vec::new();
        for &child in n.children {
            children.push(self.sync_node(
                engine,
                child,
                Some(entity),
                in_button || n.kind == Kind::ButtonBase,
                live,
            )?);
        }
        if self.tree.children(id).context(LayoutSnafu)? != children {
            self.tree.set_children(id, &children).context(LayoutSnafu)?;
        }
        let retained = &self.nodes[&entity];
        if retained.children != n.children
            && let Some(mut group) = retained.widget.as_ref().and_then(|w| w.as_group())
        {
            // Native child order also controls keyboard traversal and paint order.
            for (index, child) in n
                .children
                .iter()
                .filter_map(|e| self.nodes[e].widget.as_ref())
                .enumerate()
            {
                group.insert(child, index as i32);
            }
        }
        self.nodes.get_mut(&entity).unwrap().children = n.children.to_vec();
        Ok(id)
    }
    fn place(
        &mut self,
        engine: &Engine,
        entity: Entity,
        parent_x: i32,
        parent_y: i32,
    ) -> Result<()> {
        let n = engine.node(entity).unwrap();
        let retained = self.nodes.get_mut(&entity).unwrap();
        let layout = self.tree.layout(retained.layout).context(LayoutSnafu)?;
        let (x, y, w, h) = (
            parent_x + layout.location.x.round() as i32,
            parent_y + layout.location.y.round() as i32,
            layout.size.width.round() as i32,
            layout.size.height.round() as i32,
        );
        if let Some(widget) = &mut retained.widget {
            if [widget.x(), widget.y(), widget.w(), widget.h()] != [x, y, w, h] {
                widget.resize(x, y, w, h);
                self.stats.geometry_writes += 1;
            }
            if n.kind == Kind::Svg
                && let Some(image) = &mut retained.image
            {
                image.scale(w, h, true, true);
                widget.set_image(Some(image.clone()));
            }
        }
        let offset = retained
            .scroll
            .as_ref()
            .map_or((0, 0), |s| (s.xposition(), s.yposition()));
        for &child in n.children {
            self.place(engine, child, x - offset.0, y - offset.1)?;
        }
        Ok(())
    }
}
/// Event-driven host. Active application animations are sampled at most 60 Hz;
/// once their ANIMATE request clears, the host returns to blocking native waits.
pub fn run<V: DeclareView>(mut view: V, title: &str, width: i32, height: i32) -> Result<()> {
    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    let mut window = Window::new(100, 100, width, height, title);
    let root = Group::new(0, 0, width, height, "");
    root.end();
    window.end();
    window.make_resizable(true);
    let (tx, rx) = app::channel::<Event>();
    let mut adapter = Adapter::new(root, tx);
    let mut engine = Engine::default();
    let clock = plico_core::SystemClock::default();
    let mut previous = Duration::ZERO;
    let mut dirty = true;
    let mut pacer = super::pacing::FramePacer::default();
    window.show();
    while window.shown() {
        if dirty {
            let now = clock.now();
            engine.set_time(FrameTime {
                now,
                delta: now.saturating_sub(previous),
            });
            previous = now;
            engine
                .set_viewport(window.w() as f32, window.h() as f32)
                .context(FrameSnafu)?;
            engine
                .declare_until_settled(&mut view)
                .context(FrameSnafu)?;
            adapter.root.resize(0, 0, window.w(), window.h());
            adapter.sync(&engine)?;
            if let Some(entity) = engine.take_focus_request()
                && let Some(mut widget) = adapter.native_widget(entity)
            {
                widget.take_focus().context(FltkSnafu)?;
            }
            window.redraw();
            pacer.after_frame(now, engine.changes().contains(plico_core::Changes::ANIMATE));
            dirty = false;
        }
        let size = (window.w(), window.h());
        if let Some(timeout) = pacer.timeout(clock.now()) {
            // A timeout is not a closed-window signal: wait_for(false) simply
            // means that no native event arrived before this animation deadline.
            app::wait_for(timeout.as_secs_f64()).context(FltkSnafu)?;
        } else if !app.wait() {
            break;
        }
        while let Some(event) = rx.recv() {
            dirty |= Adapter::dispatch(&mut engine, event);
        }
        dirty |= size != (window.w(), window.h()) || pacer.due(clock.now());
    }
    Ok(())
}
