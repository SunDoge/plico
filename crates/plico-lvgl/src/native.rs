use oxivgl::{
    driver::LvglDriver,
    enums::{EventCode, ObjState},
    event::Event,
    layout::FlexFlow,
    style::{LV_SIZE_CONTENT, Selector, Style as NativeStyle, lv_pct},
    widgets::{AsLvHandle, Button, Label, Obj, Screen, WidgetError},
};
use plico_core::{Engine, Entity, Kind, NodeRef, Style, Theme};
use snafu::{OptionExt, Snafu};
use std::{
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    rc::{Rc, Weak},
};

type Queue = RefCell<Vec<Entity>>;
type Route = (Entity, Weak<Queue>);
thread_local! {
    // Native callbacks retain no application state. Route directly to a
    // generational identity so a queued click cannot target a reused address.
    static ROUTES: RefCell<HashMap<usize, Route>> = RefCell::new(HashMap::new());
}
fn clicked(event: &Event) {
    ROUTES.with(|routes| {
        if let Some((entity, queue)) = routes
            .borrow()
            .get(&(event.current_target_handle() as usize))
            && let Some(queue) = queue.upgrade()
        {
            queue.borrow_mut().push(*entity);
        }
    });
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("LVGL display must be initialized before attaching Plico"))]
    NoScreen,
    #[snafu(display("the LVGL experiment does not implement {kind:?}"))]
    Unsupported { kind: Kind },
    #[snafu(display("LVGL text cannot contain an embedded NUL"))]
    TextNul,
    #[snafu(display("LVGL widget creation failed: {error}"))]
    Widget { error: WidgetError },
}
impl From<WidgetError> for Error {
    fn from(error: WidgetError) -> Self {
        Self::Widget { error }
    }
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub created: usize,
    pub removed: usize,
    pub reused: usize,
    pub style_updates: usize,
    pub text_updates: usize,
}
enum Widget {
    Container(Obj<'static>),
    Text(Label<'static>),
    ButtonBase(Button<'static>),
    // Delete the owned label before its parent button.
    Button {
        label: Label<'static>,
        button: Button<'static>,
    },
}
impl Widget {
    fn object(&self) -> &Obj<'static> {
        match self {
            Self::Container(obj) => obj,
            Self::Text(obj) => obj,
            Self::ButtonBase(obj) => obj,
            Self::Button { button, .. } => button,
        }
    }
    fn class_matches(&self, kind: Kind) -> bool {
        matches!(
            (self, kind),
            (Self::Container(_), Kind::Root | Kind::Column | Kind::Row)
                | (Self::Text(_), Kind::Text)
                | (Self::Button { .. }, Kind::Button)
                | (Self::ButtonBase(_), Kind::ButtonBase)
        )
    }
    fn button(&self) -> Option<&Button<'static>> {
        match self {
            Self::Button { button, .. } | Self::ButtonBase(button) => Some(button),
            _ => None,
        }
    }
}
struct Mounted {
    widget: Widget,
    depth: usize,
    kind: Kind,
    style: Option<(Style, Theme)>,
    text: Option<String>,
}

/// Single-threaded native-widget projection. The application owns its Engine
/// and Rust state; no alternate component API or reactive graph is introduced.
pub struct Adapter<'driver> {
    nodes: HashMap<Entity, Mounted>,
    screen: Screen,
    events: Rc<Queue>,
    stats: Stats,
    _driver: PhantomData<&'driver LvglDriver>,
}
impl<'driver> Adapter<'driver> {
    pub fn new(_driver: &'driver LvglDriver) -> Result<Self, Error> {
        Ok(Self {
            nodes: HashMap::new(),
            screen: Screen::active().context(NoScreenSnafu)?,
            events: Rc::new(RefCell::new(Vec::new())),
            stats: Stats::default(),
            _driver: PhantomData,
        })
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Project a successful core declaration. Preflight unsupported widgets
    /// before modifying LVGL. Layout and drawing are both performed by LVGL.
    pub fn sync(&mut self, engine: &Engine) -> Result<(), Error> {
        fn walk(
            engine: &Engine,
            entity: Entity,
            parent: Option<Entity>,
            depth: usize,
            out: &mut Vec<(Entity, Option<Entity>, usize)>,
        ) -> Result<(), Error> {
            let node = engine.node(entity).expect("live engine child");
            snafu::ensure!(
                matches!(
                    node.kind,
                    Kind::Root
                        | Kind::Column
                        | Kind::Row
                        | Kind::Text
                        | Kind::Button
                        | Kind::ButtonBase
                ),
                UnsupportedSnafu { kind: node.kind }
            );
            snafu::ensure!(!node.text.contains('\0'), TextNulSnafu);
            out.push((entity, parent, depth));
            for child in node.children {
                walk(engine, *child, Some(entity), depth + 1, out)?;
            }
            Ok(())
        }
        let mut order = Vec::new();
        walk(engine, engine.root(), None, 0, &mut order)?;
        self.stats = Stats::default();
        let mut dead: Vec<_> = self
            .nodes
            .iter()
            .filter(|(id, mounted)| {
                engine
                    .node(**id)
                    .is_none_or(|n| !mounted.widget.class_matches(n.kind))
            })
            .map(|(id, mounted)| (*id, mounted.depth))
            .collect();
        dead.sort_unstable_by_key(|(_, depth)| std::cmp::Reverse(*depth));
        for (id, _) in dead {
            self.unmount(id);
            self.stats.removed += 1;
        }

        for (id, parent, depth) in &order {
            let node = engine.node(*id).expect("preflighted node");
            if !self.nodes.contains_key(id) {
                let widget = self.create(node.kind, *parent)?;
                if let Some(button) = widget.button() {
                    ROUTES.with(|routes| {
                        routes
                            .borrow_mut()
                            .insert(button.handle() as usize, (*id, Rc::downgrade(&self.events)))
                    });
                    button.on(EventCode::CLICKED, clicked);
                }
                self.nodes.insert(
                    *id,
                    Mounted {
                        widget,
                        depth: *depth,
                        kind: node.kind,
                        style: None,
                        text: None,
                    },
                );
                self.stats.created += 1;
            } else {
                self.stats.reused += 1;
            }
            let mounted = self.nodes.get_mut(id).expect("mounted node");
            if mounted.style != Some((node.style, engine.theme())) || mounted.kind != node.kind {
                apply_style(&mounted.widget, &node, engine.theme());
                mounted.style = Some((node.style, engine.theme()));
                mounted.kind = node.kind;
                self.stats.style_updates += 1;
            }
            if mounted.text.as_deref() != Some(node.text) {
                match &mounted.widget {
                    Widget::Text(label) | Widget::Button { label, .. } => {
                        label.text(node.text);
                    }
                    Widget::Container(_) | Widget::ButtonBase(_) => {}
                }
                node.text
                    .clone_into(mounted.text.get_or_insert_with(String::new));
                self.stats.text_updates += 1;
            }
        }
        for (id, _, _) in &order {
            for (index, child) in engine.node(*id).unwrap().children.iter().enumerate() {
                let obj = self.nodes[child].widget.object();
                if obj.get_index() != index as i32 {
                    obj.move_to_index(index as i32);
                }
            }
        }
        self.nodes[&engine.root()].widget.object().update_layout();
        Ok(())
    }

    fn create(&self, kind: Kind, parent: Option<Entity>) -> Result<Widget, Error> {
        fn create_at(kind: Kind, parent: &impl AsLvHandle) -> Result<Widget, Error> {
            Ok(match kind {
                Kind::Text => Widget::Text(Label::new(parent)?),
                Kind::ButtonBase => Widget::ButtonBase(Button::new(parent)?),
                Kind::Button => {
                    let button = Button::new(parent)?;
                    let label = Label::new(&button)?;
                    label.center();
                    Widget::Button { label, button }
                }
                _ => Widget::Container(Obj::new(parent)?),
            })
        }
        match parent {
            Some(parent) => create_at(kind, self.nodes[&parent].widget.object()),
            None => create_at(kind, &self.screen),
        }
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
        if let Some(button) = self
            .nodes
            .get(&entity)
            .and_then(|node| node.widget.button())
        {
            button.send_event(EventCode::CLICKED);
            true
        } else {
            false
        }
    }
    /// Diagnostic identity for reuse assertions. Never dereference this value.
    pub fn object_id(&self, entity: Entity) -> Option<usize> {
        self.nodes
            .get(&entity)
            .map(|n| n.widget.object().handle() as usize)
    }
    pub fn bounds(&self, entity: Entity) -> Option<[i32; 4]> {
        self.nodes.get(&entity).map(|n| {
            let a = n.widget.object().get_coords();
            [a.x1, a.y1, a.x2, a.y2]
        })
    }
    fn unmount(&mut self, entity: Entity) {
        if let Some(node) = self.nodes.remove(&entity) {
            ROUTES.with(|routes| {
                routes
                    .borrow_mut()
                    .remove(&(node.widget.object().handle() as usize))
            });
            drop(node);
        }
    }
}
impl Drop for Adapter<'_> {
    fn drop(&mut self) {
        let mut order: Vec<_> = self.nodes.iter().map(|(id, n)| (*id, n.depth)).collect();
        order.sort_unstable_by_key(|(_, depth)| std::cmp::Reverse(*depth));
        for (id, _) in order {
            self.unmount(id);
        }
    }
}

fn apply_style(widget: &Widget, node: &NodeRef<'_>, theme: Theme) {
    let obj = widget.object();
    let s = node.style;
    obj.remove_style_all();
    let style = NativeStyle::new(|style| {
        let width = s
            .width
            .map(|v| v as i32)
            .or_else(|| s.width_percent.map(|v| lv_pct(v as i32)))
            .unwrap_or(if node.kind == Kind::Root {
                lv_pct(100)
            } else {
                LV_SIZE_CONTENT
            });
        let height = s
            .height
            .map(|v| v as i32)
            .or_else(|| s.height_percent.map(|v| lv_pct(v as i32)))
            .unwrap_or(if node.kind == Kind::Root {
                lv_pct(100)
            } else {
                LV_SIZE_CONTENT
            });
        style
            .width(width)
            .height(height)
            .pad_hor(s.horizontal_padding() as i32)
            .pad_ver(s.vertical_padding() as i32)
            .pad_gap(s.gap as i32)
            .radius(s.radius as i16)
            .border_width(s.border_width as i16)
            .border_color_hex(theme.resolve(s.border_color))
            .text_color_hex(theme.resolve(s.foreground))
            .text_font(if s.font_size >= 24 {
                oxivgl::fonts::MONTSERRAT_24
            } else {
                oxivgl::fonts::MONTSERRAT_16
            });
        if let Some(color) = s.background {
            style.bg_color_hex(theme.resolve(color)).bg_opa(255);
        } else {
            style.bg_opa(0);
        }
        if matches!(
            node.kind,
            Kind::Root | Kind::Column | Kind::Row | Kind::ButtonBase
        ) {
            style.layout(oxivgl::layout::Layout::Flex).flex_flow(
                if matches!(node.kind, Kind::Row | Kind::ButtonBase) {
                    FlexFlow::Row
                } else {
                    FlexFlow::Column
                },
            );
        }
    });
    obj.add_style(&style, Selector::DEFAULT).remove_scrollable();
    if s.disabled {
        obj.add_state(ObjState::DISABLED);
    } else {
        obj.remove_state(ObjState::DISABLED);
    }
}
