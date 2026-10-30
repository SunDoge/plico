use nsh_core::{
    EllipsizeMode,
    element::{
        AccessibilityRole, Accessible, AdaptiveColor, AnyElement, Element, FontWeight, Interactive,
        IntoElement, ParentElement, Styled, Text, text, view,
    },
    geometry::{px, relative},
};
use plico::{Align, Color, Engine, Entity, FontFamily, Justify, Kind, NodeRef, Theme};
use snafu::Snafu;
use std::{cell::RefCell, rc::Rc};

type Route = Rc<dyn Fn(Entity)>;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("NSH does not implement node kind {kind:?} ({entity:?})"))]
    UnsupportedKind { entity: Entity, kind: Kind },
    #[snafu(display("NSH projection does not implement {property} ({entity:?})"))]
    UnsupportedStyle {
        entity: Entity,
        property: &'static str,
    },
    #[snafu(display("NSH projection does not implement styled text spans ({entity:?})"))]
    TextSpans { entity: Entity },
}

/// Entity-only native event queue and NSH element projection.
///
/// Clones share the event queue. Native callbacks never capture the Plico
/// application or its declaration closures.
#[derive(Clone, Default)]
pub struct Adapter {
    events: Rc<RefCell<Vec<Entity>>>,
}

impl Adapter {
    /// Project the current retained tree. Call [`dispatch`](Self::dispatch)
    /// before the next declaration when using this low-level API directly.
    pub fn project(&self, engine: &Engine) -> Result<AnyElement, Error> {
        let adapter = self.clone();
        self.project_with(
            engine,
            Rc::new(move |entity| adapter.events.borrow_mut().push(entity)),
        )
    }

    pub(crate) fn project_with(&self, engine: &Engine, route: Route) -> Result<AnyElement, Error> {
        validate_tree(engine, engine.root())?;
        Ok(project_node(engine, engine.root(), &route))
    }

    /// Queue a native button activation. This is also useful for headless
    /// round-trip tests; it does not mutate application state directly.
    pub fn send_click(&self, entity: Entity) {
        self.events.borrow_mut().push(entity);
    }

    pub fn pending_events(&self) -> usize {
        self.events.borrow().len()
    }

    /// Route queued native events into the next ordinary declaration.
    /// Removed, remounted, non-button and disabled targets are ignored.
    pub fn dispatch(&self, engine: &mut Engine) -> usize {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        let mut dispatched = 0;
        for entity in events {
            let Some(node) = engine.node(entity) else {
                continue;
            };
            if !node.style.disabled && matches!(node.kind, Kind::Button | Kind::ButtonBase) {
                engine.queue_click(entity);
                dispatched += 1;
            }
        }
        dispatched
    }
}

fn validate_tree(engine: &Engine, entity: Entity) -> Result<(), Error> {
    let node = engine.node(entity).expect("live engine child");
    validate_node(&node)?;
    for child in node.children {
        validate_tree(engine, *child)?;
    }
    Ok(())
}

fn validate_node(node: &NodeRef<'_>) -> Result<(), Error> {
    if !matches!(
        node.kind,
        Kind::Root
            | Kind::Column
            | Kind::Row
            | Kind::Overlay
            | Kind::Dialog
            | Kind::TooltipAnchor
            | Kind::Text
            | Kind::Button
            | Kind::ButtonBase
    ) {
        return Err(Error::UnsupportedKind {
            entity: node.entity,
            kind: node.kind,
        });
    }
    if !node.spans.is_empty() {
        return Err(Error::TextSpans {
            entity: node.entity,
        });
    }
    let style = node.style;
    let unsupported = if node.grid.is_some() {
        Some("grid templates")
    } else if style.layout != plico::LayoutStyle::default() {
        Some("advanced LayoutStyle")
    } else if style.selectable {
        Some("text selection")
    } else if style.backdrop_blur != 0. {
        Some("backdrop blur")
    } else if style.overlay || style.popup_anchor.is_some() || style.modal_open {
        Some("portal and modal behavior")
    } else if style.anchor_id.is_some() || style.role.is_some() {
        Some("portable widget roles")
    } else if style.presence_until_ms.is_some() {
        Some("presence deadlines")
    } else if style.scroll_y || style.smooth_scroll || style.follow_end {
        Some("scrolling")
    } else if style.translate != [0., 0.] || style.scale != [1., 1.] {
        Some("transforms")
    } else if matches!(style.justify, Some(Justify::Stretch)) {
        Some("Justify::Stretch")
    } else if style.checked
        || style.input_revision != 0
        || style.input_navigation
        || style.input_password_visible
        || style.shader_revision != 0
        || style.numeric_revision != 0
        || style.slider.is_some()
    {
        Some("control state without a supported control")
    } else {
        None
    };
    if let Some(property) = unsupported {
        return Err(Error::UnsupportedStyle {
            entity: node.entity,
            property,
        });
    }
    Ok(())
}

fn project_node(engine: &Engine, entity: Entity, route: &Route) -> AnyElement {
    let node = engine.node(entity).expect("validated live node");
    let theme = engine.theme();
    match node.kind {
        Kind::Text => {
            let element = apply_text_ellipsis(
                apply_style(text(node.text.to_owned()), &node, theme),
                node.style.text_ellipsis,
            );
            element.key(entity_key(entity)).into_element()
        }
        Kind::Button | Kind::ButtonBase => {
            let mut element = apply_style(view(), &node, theme)
                .items_center()
                .justify_center()
                .accessible(true)
                .role(AccessibilityRole::Button)
                .label(node.text.to_owned());
            if node.kind == Kind::Button {
                let label =
                    apply_text_ellipsis(text(node.text.to_owned()), node.style.text_ellipsis);
                element = element.child(label);
            } else {
                element = element.children(
                    node.children
                        .iter()
                        .map(|child| project_node(engine, *child, route)),
                );
            }
            let route = route.clone();
            element
                .on_press(move || route(entity))
                .key(entity_key(entity))
                .into_element()
        }
        Kind::Root
        | Kind::Column
        | Kind::Row
        | Kind::Overlay
        | Kind::Dialog
        | Kind::TooltipAnchor => {
            let mut element = apply_style(view(), &node, theme);
            if matches!(node.kind, Kind::Root | Kind::Column) {
                element = element.flex_col();
            } else if node.kind == Kind::Row {
                element = element.flex_row();
            }
            element
                .children(
                    node.children
                        .iter()
                        .map(|child| project_node(engine, *child, route)),
                )
                .key(entity_key(entity))
                .into_element()
        }
        _ => unreachable!("validated NSH kind"),
    }
}

fn entity_key(entity: Entity) -> String {
    entity.raw().to_string()
}

fn color(theme: Theme, color: Color, alpha: f32) -> AdaptiveColor {
    AdaptiveColor::rgb(theme.resolve(color), theme.resolve(color)).opacity(alpha)
}

fn apply_style<E: Styled + Interactive>(mut element: E, node: &NodeRef<'_>, theme: Theme) -> E {
    let style = node.style;
    if let Some(value) = style.width {
        element = element.w(px(value));
    }
    if let Some(value) = style.width_percent {
        element = element.w(relative(value / 100.));
    }
    if let Some(value) = style.height {
        element = element.h(px(value));
    }
    if let Some(value) = style.height_percent {
        element = element.h(relative(value / 100.));
    }
    if let Some(value) = style.min_width {
        element = element.min_w(px(value));
    }
    if let Some(value) = style.min_height {
        element = element.min_h(px(value));
    }
    if let Some(value) = style.max_width {
        element = element.max_w(px(value));
    }
    if let Some(value) = style.max_height {
        element = element.max_h(px(value));
    }
    element = element
        .flex_grow(style.grow)
        .flex_shrink(style.shrink)
        .gap(px(style.gap))
        .px(px(style.horizontal_padding()))
        .py(px(style.vertical_padding()))
        .opacity(style.opacity)
        .rounded(px(style.radius))
        .border(px(style.border_width))
        .border_color(color(theme, style.border_color, 1.))
        .text_size(px(f32::from(style.font_size)))
        .font_weight(FontWeight(style.font_weight))
        .text_color(color(theme, style.foreground, 1.))
        .disabled(style.disabled);
    if let Some(family) = match style.font_family {
        FontFamily::Default => None,
        FontFamily::SansSerif => Some("sans-serif"),
        FontFamily::Serif => Some("serif"),
        FontFamily::Monospace => Some("monospace"),
    } {
        element = element.font_family(family);
    }
    if let Some(background) = style.background {
        element = element.bg(color(theme, background, style.background_alpha));
    }
    if let Some(background) = style.hover_background {
        element = element.hover(move |style| style.bg(color(theme, background, 1.)));
    }
    if let Some(background) = style.pressed_background {
        element = element.active(move |style| style.bg(color(theme, background, 1.)));
    }
    if style.wrap {
        element = element.flex_wrap();
    }
    if style.clip {
        element = element.overflow_hidden();
    }
    if style.transition_ms != 0 {
        element = element
            .transition_all()
            .duration(f32::from(style.transition_ms));
    }
    if let Some([x, y]) = style.absolute {
        element = element.absolute().left(px(x)).top(px(y));
    }
    element = match style.align {
        Some(Align::Start) => element.items_start(),
        Some(Align::Center) => element.items_center(),
        Some(Align::End) => element.items_end(),
        Some(Align::Stretch) => element.items_stretch(),
        Some(Align::Baseline) => element.items_baseline(),
        None => element,
    };
    match style.justify {
        Some(Justify::Start) => element.justify_start(),
        Some(Justify::Center) => element.justify_center(),
        Some(Justify::End) => element.justify_end(),
        Some(Justify::SpaceBetween) => element.justify_between(),
        Some(Justify::SpaceAround) => element.justify_around(),
        Some(Justify::SpaceEvenly) => element.justify_evenly(),
        Some(Justify::Stretch) => unreachable!("validated justify"),
        None => element,
    }
}

fn apply_text_ellipsis(mut element: Text, enabled: bool) -> Text {
    if enabled {
        element = element
            .number_of_lines(1u32)
            .ellipsize_mode(EllipsizeMode::Tail);
    }
    element
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico::{Theme, View, prelude::*};

    #[derive(Default)]
    struct Counter {
        count: u32,
        extra: bool,
        disabled: bool,
        rows: Vec<u64>,
    }

    impl View for Counter {
        fn theme(&self) -> Theme {
            Theme::default()
        }

        #[component]
        #[rustfmt::skip]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).padding(24).gap(14).width_percent(100.) + |cx| {
                text!(cx, "Count: {}", self.count);
                if button(cx).text("Increment").disabled(self.disabled).primary().clicked() {
                    self.count += 1;
                }
                if button(cx).text("Toggle extra").clicked() {
                    self.extra = !self.extra;
                }
                if self.extra && button(cx).text("Extra action").clicked() {
                    self.count += 10;
                }
                if button(cx).text(if self.disabled { "Enable increment" } else { "Disable increment" }).clicked() {
                    self.disabled = !self.disabled;
                }
                row(cx) + |cx| {
                    for id in &self.rows {
                        text!(cx, "Item {id}").key(*id);
                    }
                };
            };
        }
    }

    fn find(engine: &Engine, kind: Kind, label: &str) -> Entity {
        fn visit(engine: &Engine, entity: Entity, kind: Kind, label: &str) -> Option<Entity> {
            let node = engine.node(entity)?;
            if node.kind == kind && node.text == label {
                return Some(entity);
            }
            node.children
                .iter()
                .find_map(|child| visit(engine, *child, kind, label))
        }
        visit(engine, engine.root(), kind, label).expect("declared widget")
    }

    fn frame(engine: &mut Engine, app: &mut Counter, adapter: &Adapter) {
        engine.declare_until_settled(app).unwrap();
        adapter.project(engine).unwrap();
    }

    #[test]
    fn click_round_trip_reorder_stale_events_and_disabled_input() {
        let mut engine = Engine::default();
        let mut app = Counter {
            rows: vec![1, 2],
            ..Counter::default()
        };
        let adapter = Adapter::default();
        frame(&mut engine, &mut app, &adapter);
        let increment = find(&engine, Kind::ButtonBase, "Increment");
        let toggle = find(&engine, Kind::ButtonBase, "Disable increment");
        let toggle_label = engine.node(toggle).unwrap().children[0];
        let item = find(&engine, Kind::Text, "Item 1");

        for expected in 1..=3 {
            adapter.send_click(increment);
            assert_eq!(adapter.dispatch(&mut engine), 1);
            frame(&mut engine, &mut app, &adapter);
            assert_eq!(app.count, expected);
            assert_eq!(find(&engine, Kind::ButtonBase, "Increment"), increment);
        }

        app.extra = true;
        app.rows.reverse();
        frame(&mut engine, &mut app, &adapter);
        assert_eq!(find(&engine, Kind::Text, "Item 1"), item);
        let extra = find(&engine, Kind::ButtonBase, "Extra action");
        adapter.send_click(extra);
        app.extra = false;
        frame(&mut engine, &mut app, &adapter);
        app.extra = true;
        frame(&mut engine, &mut app, &adapter);
        assert_ne!(find(&engine, Kind::ButtonBase, "Extra action"), extra);
        assert_eq!(adapter.dispatch(&mut engine), 0);

        app.disabled = true;
        frame(&mut engine, &mut app, &adapter);
        assert_eq!(find(&engine, Kind::ButtonBase, "Enable increment"), toggle);
        let toggle_node = engine.node(toggle).unwrap();
        let label = toggle_node.children[0];
        assert_eq!(label, toggle_label);
        assert_eq!(engine.node(label).unwrap().text, "Enable increment");
        adapter.send_click(increment);
        assert_eq!(adapter.dispatch(&mut engine), 0);
        assert_eq!(adapter.pending_events(), 0);
    }

    #[test]
    fn unsupported_controls_fail_instead_of_becoming_containers() {
        let mut engine = Engine::default();
        engine
            .frame_declarations(|cx| {
                checkbox(cx, "Unsupported", &mut false);
            })
            .unwrap();
        let entity = find(&engine, Kind::Checkbox, "Unsupported");
        assert!(matches!(
            Adapter::default().project(&engine),
            Err(Error::UnsupportedKind {
                entity: found,
                kind: Kind::Checkbox,
            }) if found == entity
        ));
    }

    #[test]
    fn entity_keys_include_generation() {
        let first = Entity::default();
        assert_eq!(entity_key(first), first.raw().to_string());
    }
}
