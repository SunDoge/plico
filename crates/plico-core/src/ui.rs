use crate::{Entity, Theme, engine::Engine, model::*};
use std::borrow::Cow;

/// Context borrowed while declaring one UI tree. Widget constructors live in `widgets`.
pub struct UiContext<'e> {
    pub(crate) engine: &'e mut Engine,
    pub(crate) parent: Entity,
    pub(crate) scope: u64,
    pub(crate) inherited: Style,
    pub(crate) in_button: bool,
}
/// Compatibility name for the UI build context.
pub type Ui<'e> = UiContext<'e>;
impl<'e> Ui<'e> {
    pub fn element<R>(
        &mut self,
        key: Key,
        kind: Kind,
        style: Style,
        text: &str,
        children: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, bool) {
        self.element_with_grid(key, kind, style, text, None, children)
    }
    /// Backend/protocol entry point; normal applications use typed builders.
    pub fn element_with_grid<R>(
        &mut self,
        key: Key,
        kind: Kind,
        mut style: Style,
        text: &str,
        grid: Option<&crate::GridTemplate>,
        children: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, bool) {
        style.disabled |= self.inherited.disabled;
        if kind.interactive() && self.in_button {
            self.engine.error.get_or_insert(FrameError::NestedControl);
        }
        let entity = self.engine.declare(
            self.parent,
            key,
            self.scope,
            kind,
            style,
            Cow::Borrowed(text),
        );
        self.engine.set_spans(entity, &[]);
        self.engine.set_grid(entity, grid);
        let clicked = kind.interactive() && !style.disabled && self.engine.clicked(entity);
        let result = self.with_parent(entity, style, kind == Kind::ButtonBase, children);
        (result, clicked)
    }

    /// Portable rich paragraph entry point; applications normally use `text(...).spans(...)`.
    pub fn rich_text_element(
        &mut self,
        key: Key,
        style: Style,
        text: &str,
        spans: &[crate::TextSpan],
    ) {
        let entity = self.engine.declare(
            self.parent,
            key,
            self.scope,
            Kind::Text,
            style,
            Cow::Borrowed(text),
        );
        self.engine.set_spans(entity, spans);
        self.engine.set_grid(entity, None);
    }

    /// Statically dispatch a built-in or application-defined widget.
    pub fn add<W: crate::Widget>(&mut self, widget: W) -> W::Response {
        let key = self.engine.auto_key(self.parent);
        self.add_keyed(key, widget)
    }
    pub fn add_keyed<W: crate::Widget>(&mut self, key: impl Into<Key>, widget: W) -> W::Response {
        self.keyed(key, |ui| widget.show(ui))
    }
    /// Bind a native editor to the current node (host replay path). The
    /// adapter edits this binding; the reload transport syncs its content
    /// with the module-side application buffer by revision.
    pub fn bind_input(&mut self, buffer: &crate::TextBuffer, placeholder: &str) {
        let counters = buffer.counters();
        self.engine.inputs.insert(
            self.parent,
            (buffer.clone(), placeholder.to_string(), counters),
        );
    }
    /// Host scroll feedback for the current container, in logical pixels.
    /// Reading it makes subsequent scroll changes invalidate declarations;
    /// containers that never read it can scroll entirely in the host.
    pub fn scroll_viewport(&self) -> Option<[f32; 2]> {
        self.engine.scroll_viewport(self.parent)
    }
    /// Request a logical vertical offset for the current scroll container.
    pub fn scroll_to_y(&mut self, offset: f32) {
        if offset.is_finite() && offset >= 0. {
            self.engine.scroll_requests.insert(self.parent, offset);
        }
    }
    /// Copy the current theme without keeping a borrow of Ui alive.
    pub fn theme(&self) -> Theme {
        self.engine.theme()
    }
    fn inherited_style(&self) -> Style {
        Style {
            font_size: self.inherited.font_size,
            font_weight: self.inherited.font_weight,
            foreground: self.inherited.foreground,
            disabled: self.inherited.disabled,
            ..Default::default()
        }
    }
    pub(crate) fn with_parent<R>(
        &mut self,
        parent: Entity,
        style: Style,
        in_button: bool,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        let old = (self.parent, self.inherited, self.in_button);
        self.parent = parent;
        self.inherited = style;
        self.in_button |= in_button;
        let result = build(self);
        (self.parent, self.inherited, self.in_button) = old;
        result
    }
    /// Unstyled button behavior with a mandatory accessible name and composable content.
    pub fn button_base<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
    ) -> ButtonBase<'u, 'e, 't> {
        ButtonBase {
            inner: self.leaf(Kind::ButtonBase, label.into()),
            styled: false,
        }
    }
    pub fn column(&mut self) -> Layout<'_, 'e> {
        self.layout(Kind::Column)
    }
    pub fn grid(&mut self) -> Layout<'_, 'e> {
        self.column().display(crate::Display::Grid)
    }
    pub fn row(&mut self) -> Layout<'_, 'e> {
        self.layout(Kind::Row)
    }
    fn layout(&mut self, kind: Kind) -> Layout<'_, 'e> {
        let key = self.engine.auto_key(self.parent);
        let style = self.inherited_style();
        Layout {
            ui: self,
            key,
            kind,
            style,
            grid: None,
        }
    }
    pub fn text_input<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        buffer: &crate::TextBuffer,
    ) -> Input<'u, 'e, 't> {
        let m = self.theme().metrics;
        let mut inner = self.leaf(Kind::TextInput, label.into());
        inner.style.width = Some(240.);
        inner.style.height = Some(m.control_height as f32 + 4.);
        inner.style.padding_x = Some(m.space(2) as f32);
        inner.style.padding_y = Some(m.space(1) as f32);
        inner.style.radius = m.radius as f32;
        inner.style.border_width = 1.;
        inner.style.background = Some(Color::Panel);
        inner.style.clip = true;
        Input {
            inner,
            buffer: buffer.clone(),
            placeholder: Cow::Borrowed(""),
            autofocus: false,
            focus: false,
        }
    }
    pub fn text<'u, 't>(&'u mut self, text: impl Into<Cow<'t, str>>) -> Text<'u, 'e, 't> {
        Text {
            inner: self.leaf(Kind::Text, text.into()),
        }
    }
    /// Format synchronously into an engine-owned reusable scratch buffer.
    pub fn text_fmt(&mut self, args: std::fmt::Arguments<'_>) -> Text<'_, 'e, 'static> {
        use std::fmt::Write;
        if let Some(literal) = args.as_str() {
            return self.text(literal);
        }
        let mut buffer = std::mem::take(&mut self.engine.format_buffer);
        buffer.clear();
        buffer
            .write_fmt(args)
            .expect("a text Display implementation returned an error");
        let mut inner = self.leaf(Kind::Text, Cow::Owned(buffer));
        inner.recycle_text = true;
        Text { inner }
    }
    pub fn svg(&mut self, asset: &crate::SvgAsset) -> Svg<'_, 'e> {
        let mut inner = self.leaf(Kind::Svg, Cow::Borrowed(asset.source()));
        inner.style.width = Some(24.);
        inner.style.height = Some(24.);
        inner.style.shrink = 0.;
        Svg { inner }
    }
    /// Declare generated SVG markup without requiring a leaked static asset.
    pub fn svg_source(&mut self, source: String) -> Svg<'_, 'e> {
        let mut inner = self.leaf(Kind::Svg, Cow::Owned(source));
        inner.style.width = Some(24.);
        inner.style.height = Some(24.);
        inner.style.shrink = 0.;
        Svg { inner }
    }
    pub fn button<'u, 't>(&'u mut self, text: impl Into<Cow<'t, str>>) -> Button<'u, 'e, 't> {
        let metrics = self.theme().metrics;
        let mut inner = self.leaf(Kind::Button, text.into());
        inner.style.transition_ms = metrics.interaction_ms;
        inner.style.padding = 0.;
        inner.style.padding_x = Some(metrics.spacing as f32 * 3.5);
        inner.style.padding_y = Some(metrics.spacing as f32 * 1.5);
        inner.style.border_width = 1.;
        inner.style.radius = metrics.radius as f32;
        inner.style.min_height = Some(metrics.control_height as f32);
        inner.style.background = Some(Color::Control);
        Button {
            inner,
            icon: None,
            accessible_name: None,
            composable: false,
        }
    }
    pub fn viewport_size(&self) -> [f32; 2] {
        self.engine.viewport_size()
    }
    /// Logical width of the root viewport, for responsive application layouts.
    pub fn viewport_width(&self) -> f32 {
        self.engine.viewport_width()
    }
    pub fn time(&self) -> crate::FrameTime {
        self.engine.time
    }
    pub fn now(&self) -> std::time::Duration {
        self.engine.time.now
    }
    pub fn delta(&self) -> std::time::Duration {
        self.engine.time.delta
    }
    /// Schedule a future frame without repeating this frame's declarations.
    pub fn request_animation_frame(&mut self) {
        self.engine.changes.insert(Changes::ANIMATE);
    }
    /// Request another declaration for a non-event update performed during show.
    pub fn request_rebuild(&mut self) {
        self.engine.changes.insert(Changes::VIEW);
    }
    #[doc(hidden)]
    pub fn __scope<R>(&mut self, site: u64, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let scope = crate::__private::site_id("plico.scope", 0) ^ self.scope.rotate_left(17) ^ site;
        let parent = self.parent;
        let previous_auto = self.engine.nodes.get(parent).unwrap().auto;
        self.engine.nodes.get_mut(parent).unwrap().auto = 0;
        let mut ui = Ui {
            engine: &mut *self.engine,
            parent,
            scope,
            inherited: self.inherited,
            in_button: self.in_button,
        };
        let result = build(&mut ui);
        self.engine.nodes.get_mut(parent).unwrap().auto = previous_auto;
        result
    }

    /// Stable logical group for repeated components in a dynamic list; no layout node.
    pub fn keyed<R>(&mut self, key: impl Into<Key>, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let scope = crate::__private::key_seed(key.into());
        self.__scope(scope, build)
    }
    pub fn switch(&mut self, label: &str, value: &mut bool) -> bool {
        let key = self.engine.auto_key(self.parent);
        self.toggle(Kind::Switch, key, label, value)
    }
    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> bool {
        let key = self.engine.auto_key(self.parent);
        self.toggle(Kind::Checkbox, key, label, value)
    }
    /// Synchronous controlled toggle. The borrowed value is never retained.
    pub fn switch_keyed(&mut self, key: impl Into<Key>, label: &str, value: &mut bool) -> bool {
        self.toggle(Kind::Switch, key.into(), label, value)
    }
    pub fn checkbox_keyed(&mut self, key: impl Into<Key>, label: &str, value: &mut bool) -> bool {
        self.toggle(Kind::Checkbox, key.into(), label, value)
    }
    fn toggle(&mut self, kind: Kind, key: Key, label: &str, value: &mut bool) -> bool {
        self.toggle_control(kind, label, value).key(key).changed()
    }
    pub(crate) fn toggle_control<'u, 't>(
        &'u mut self,
        kind: Kind,
        label: impl Into<Cow<'t, str>>,
        value: &'t mut bool,
    ) -> Toggle<'u, 'e, 't> {
        let metrics = self.theme().metrics;
        let checked = *value;
        let mut inner = self.leaf(kind, label.into());
        inner.style.checked = checked;
        inner.style.transition_ms = metrics.interaction_ms;
        inner.style.padding = metrics.space(2) as f32;
        inner.style.gap = metrics.spacing as f32 * if kind == Kind::Switch { 1.5 } else { 2. };
        inner.style.radius = metrics.radius as f32;
        inner.style.min_height = Some(metrics.control_height as f32);
        Toggle { inner, value }
    }
    fn leaf<'u, 't>(&'u mut self, kind: Kind, text: Cow<'t, str>) -> Leaf<'u, 'e, 't> {
        let key = self.engine.auto_key(self.parent);
        if kind.interactive() && self.in_button {
            self.engine.error.get_or_insert(FrameError::NestedControl);
        }
        let style = self.inherited_style();
        Leaf {
            ui: Some(self),
            key,
            kind,
            style,
            text,
            recycle_text: false,
            spans: Vec::new(),
        }
    }
}
pub struct Layout<'u, 'e> {
    ui: &'u mut Ui<'e>,
    key: Key,
    kind: Kind,
    style: Style,
    grid: Option<crate::GridTemplate>,
}
impl Layout<'_, '_> {
    pub fn grid_columns(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().columns = tracks.into_iter().map(Into::into).collect();
        self
    }
    pub fn grid_rows(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().rows = tracks.into_iter().map(Into::into).collect();
        self
    }
    pub fn grid_auto_columns(
        mut self,
        tracks: impl IntoIterator<Item: Into<crate::Track>>,
    ) -> Self {
        self.grid.get_or_insert_default().auto_columns =
            tracks.into_iter().map(Into::into).collect();
        self
    }
    pub fn grid_auto_rows(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().auto_rows = tracks.into_iter().map(Into::into).collect();
        self
    }
    pub fn grid_areas(mut self, rows: impl IntoIterator<Item: Into<String>>) -> Self {
        self.grid.get_or_insert_default().areas = rows.into_iter().map(Into::into).collect();
        self
    }
    pub fn grid_area(mut self, name: impl Into<String>) -> Self {
        self.grid.get_or_insert_default().area = Some(name.into());
        self
    }
    pub fn min_height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("min_height", value.into());
        self
    }
    pub fn display(mut self, value: crate::Display) -> Self {
        self.style.layout.display = value;
        self
    }
    pub fn flex_direction(mut self, value: crate::FlexDirection) -> Self {
        self.style.layout.flex_direction = Some(value);
        self
    }
    pub fn flex_wrap(mut self, value: crate::FlexWrap) -> Self {
        self.style.layout.flex_wrap = Some(value);
        self
    }
    pub fn box_sizing(mut self, value: crate::BoxSizing) -> Self {
        self.style.layout.box_sizing = value;
        self
    }
    pub fn direction(mut self, value: crate::Direction) -> Self {
        self.style.layout.direction = value;
        self
    }
    pub fn position(mut self, value: crate::Position) -> Self {
        self.style.layout.position = value;
        self
    }
    pub fn grid_flow(mut self, value: crate::GridFlow) -> Self {
        self.style.layout.grid_flow = value;
        self
    }
    pub fn align_self(mut self, value: crate::Align) -> Self {
        self.style.layout.align_self = Some(value);
        self
    }
    pub fn justify_self(mut self, value: crate::Align) -> Self {
        self.style.layout.justify_self = Some(value);
        self
    }
    pub fn justify_items(mut self, value: crate::Align) -> Self {
        self.style.layout.justify_items = Some(value);
        self
    }
    pub fn align_content(mut self, value: crate::Justify) -> Self {
        self.style.layout.align_content = Some(value);
        self
    }
    pub fn aspect_ratio(mut self, value: f32) -> Self {
        self.style.layout.aspect_ratio = Some(value);
        self
    }
    pub fn flex_basis(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.flex_basis = value.into();
        self
    }
    pub fn flex_line_count(mut self, value: u16) -> Self {
        self.style.layout.flex_line_count = value;
        self
    }
    pub fn column_gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.gap[0] = Some(value.into());
        self
    }
    pub fn row_gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.gap[1] = Some(value.into());
        self
    }
    pub fn margin(mut self, value: impl Into<crate::Length>) -> Self {
        self.style
            .set_edges("margin", crate::Edges::all(value.into()));
        self
    }
    pub fn margin_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("margin", value);
        self
    }
    pub fn margin_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "left", value.into());
        self
    }
    pub fn margin_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "right", value.into());
        self
    }
    pub fn margin_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "top", value.into());
        self
    }
    pub fn margin_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "bottom", value.into());
        self
    }
    pub fn padding_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("padding", value);
        self
    }
    pub fn padding_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "left", value.into());
        self
    }
    pub fn padding_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "right", value.into());
        self
    }
    pub fn padding_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "top", value.into());
        self
    }
    pub fn padding_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "bottom", value.into());
        self
    }
    pub fn inset(mut self, value: impl Into<crate::Length>) -> Self {
        self.style
            .set_edges("inset", crate::Edges::all(value.into()));
        self
    }
    pub fn inset_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("inset", value);
        self
    }
    pub fn inset_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "left", value.into());
        self
    }
    pub fn inset_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "right", value.into());
        self
    }
    pub fn inset_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "top", value.into());
        self
    }
    pub fn inset_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "bottom", value.into());
        self
    }
    pub fn grid_row(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
        self.style.layout.grid_row = [start, end];
        self
    }
    pub fn row_span(self, count: u16) -> Self {
        self.grid_row(crate::GridLine::Auto, crate::GridLine::Span(count))
    }
    pub fn grid_column(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
        self.style.layout.grid_column = [start, end];
        self
    }
    pub fn column_span(self, count: u16) -> Self {
        self.grid_column(crate::GridLine::Auto, crate::GridLine::Span(count))
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.style.translate = [x, y];
        self
    }
    pub fn scale(mut self, x: f32, y: f32) -> Self {
        self.style.scale = [x, y];
        self
    }
    /// Place this container relative to its containing block, outside flex flow.
    pub fn absolute(mut self, left: f32, top: f32) -> Self {
        self.style.absolute = Some([left, top]);
        self.style.layout.position = crate::Position::Absolute;
        if self.style.layout.inset.is_some() {
            self.style
                .set_edge("inset", "left", crate::Length::Px(left));
            self.style.set_edge("inset", "top", crate::Length::Px(top));
        }
        self
    }
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    pub fn gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("gap", value.into());
        self
    }
    pub fn padding(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding", value.into());
        self
    }
    pub fn padding_x(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding_x", value.into());
        self
    }
    pub fn padding_y(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding_y", value.into());
        self
    }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style.border_width = width;
        self.style.border_color = color;
        self
    }
    pub fn width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("width", value.into());
        self
    }
    pub fn height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("height", value.into());
        self
    }
    pub fn width_percent(mut self, value: f32) -> Self {
        self.style
            .set_dimension("width", crate::Length::Percent(value));
        self
    }
    pub fn height_percent(mut self, value: f32) -> Self {
        self.style
            .set_dimension("height", crate::Length::Percent(value));
        self
    }
    pub fn min_width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("min_width", value.into());
        self
    }
    pub fn max_height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("max_height", value.into());
        self
    }
    pub fn max_width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("max_width", value.into());
        self
    }
    pub fn grow(mut self, value: f32) -> Self {
        self.style.grow = value;
        self
    }
    pub fn shrink(mut self, value: f32) -> Self {
        self.style.shrink = value;
        self
    }
    /// A bounded vertical viewport. Its children keep their intrinsic height.
    /// Animate wheel scrolling and scrollbar track paging (160ms ease-out).
    pub fn smooth_scroll(mut self, enabled: bool) -> Self {
        self.style.smooth_scroll = enabled;
        self
    }
    pub fn selectable(mut self, value: bool) -> Self {
        self.style.selectable = value;
        self
    }
    pub fn scroll_y(mut self, value: bool) -> Self {
        self.style.scroll_y = value;
        self
    }
    pub fn clip(mut self, value: bool) -> Self {
        self.style.clip = value;
        self
    }
    pub fn wrap(mut self, value: bool) -> Self {
        self.style.wrap = value;
        self
    }
    pub fn align(mut self, value: Align) -> Self {
        self.style.align = Some(value);
        self
    }
    pub fn justify(mut self, value: Justify) -> Self {
        self.style.justify = Some(value);
        self
    }
    /// Blur the already-painted application backdrop behind this container.
    pub fn backdrop_blur(mut self, sigma: f32) -> Self {
        self.style.backdrop_blur = sigma;
        self
    }
    /// Tint only the background; text and child controls remain opaque.
    pub fn background_alpha(mut self, alpha: f32) -> Self {
        self.style.background_alpha = alpha;
        self
    }
    pub fn opacity(mut self, value: f32) -> Self {
        self.style.opacity = value;
        self
    }
    pub fn radius(mut self, px: f32) -> Self {
        self.style.radius = px;
        self
    }
    pub fn motion(self, preset: Motion) -> Self {
        self.transition(preset.duration_ms())
    }
    pub fn transition(mut self, milliseconds: u16) -> Self {
        self.style.transition_ms = milliseconds;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.style.background = Some(color);
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.style.disabled |= value;
        self
    }
    /// Declare this container's children. Returns `InnerResponse<R>` so the
    /// result chains without parentheses, matching every other container:
    /// containers are never interactive, so their response reports only the
    /// inherited disabled state.
    pub fn children<R>(self, build: impl FnOnce(&mut Ui<'_>) -> R) -> InnerResponse<R> {
        let entity = self.ui.engine.declare(
            self.ui.parent,
            self.key,
            self.ui.scope,
            self.kind,
            self.style,
            Cow::Borrowed(""),
        );
        self.ui.engine.set_grid(entity, self.grid.as_ref());
        let inner = self.ui.with_parent(entity, self.style, false, build);
        InnerResponse {
            inner,
            response: Response::new(false, !self.style.disabled),
        }
    }
}
/// Native operator spelling for the consuming children operation.
/// The closure receives the child context; its return value is preserved.
impl crate::ImplicitKey for Layout<'_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.key, Key::Auto(_)) {
            self.key = Key::Generated(site);
        }
        self
    }
}
impl<F, R> std::ops::Add<F> for Layout<'_, '_>
where
    F: FnOnce(&mut Ui<'_>) -> R,
{
    type Output = InnerResponse<R>;

    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

struct Leaf<'u, 'e, 't> {
    ui: Option<&'u mut Ui<'e>>,
    key: Key,
    kind: Kind,
    style: Style,
    text: Cow<'t, str>,
    recycle_text: bool,
    spans: Vec<crate::TextSpan>,
}
impl Leaf<'_, '_, '_> {
    fn children<R>(
        &mut self,
        name: &str,
        styled: bool,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> InnerResponse<R> {
        let ui = self.ui.take().expect("one declaration");
        let mut style = self.style;
        if styled {
            style.align.get_or_insert(Align::Center);
            style.justify.get_or_insert(Justify::Center);
            // Button spacing is shared by shortcuts and custom content.
            style.gap = ui.theme().metrics.space(2) as f32;
        }
        style.disabled |= ui.inherited.disabled;
        style.styled_button = styled;
        if styled && style.disabled {
            style.opacity *= 0.45;
        }
        let entity = ui.engine.declare(
            ui.parent,
            self.key,
            ui.scope,
            Kind::ButtonBase,
            style,
            Cow::Borrowed(name),
        );
        let response = Response {
            clicked: !style.disabled && ui.engine.clicked(entity),
            enabled: !style.disabled,
        };
        ui.engine.set_grid(entity, None);
        ui.engine.set_spans(entity, &[]);
        let inner = ui.with_parent(entity, style, true, build);
        InnerResponse { inner, response }
    }

    fn finish(&mut self) -> (Entity, bool) {
        self.finish_controlled(None)
    }
    fn finish_controlled(&mut self, value: Option<&mut bool>) -> (Entity, bool) {
        let ui = self.ui.take().unwrap();
        let text = std::mem::take(&mut self.text);
        let mut recycled = None;
        let declared = if self.recycle_text {
            recycled = Some(text.into_owned());
            Cow::Borrowed(recycled.as_ref().unwrap().as_str())
        } else {
            text
        };
        let entity = ui.engine.declare(
            ui.parent, self.key, ui.scope, self.kind, self.style, declared,
        );
        ui.engine.set_spans(entity, &self.spans);
        ui.engine.set_grid(entity, None);
        if let Some(buffer) = recycled {
            ui.engine.format_buffer = buffer;
        }
        let clicked = self.kind.interactive() && !self.style.disabled && ui.engine.clicked(entity);
        if clicked && let Some(value) = value {
            *value = !*value;
            self.style.checked = *value;
            let node = ui.engine.nodes.get_mut(entity).unwrap();
            node.style.checked = *value;
            node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
        }
        (entity, clicked)
    }
}
impl Drop for Leaf<'_, '_, '_> {
    fn drop(&mut self) {
        if self.ui.is_some() {
            self.finish();
        }
    }
}
pub struct Text<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
}
impl Text<'_, '_, '_> {
    pub fn spans(mut self, spans: &[crate::TextSpan]) -> Self {
        self.inner.spans.extend_from_slice(spans);
        self
    }
    /// Keep this paragraph on one line and truncate at the width limit.
    pub fn ellipsis(mut self, enabled: bool) -> Self {
        self.inner.style.text_ellipsis = enabled;
        self
    }
}
pub struct Button<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    icon: Option<crate::SvgAsset>,
    accessible_name: Option<Cow<'t, str>>,
    composable: bool,
}
macro_rules! common {
    ($name:ident) => {
        impl $name<'_, '_, '_> {
            pub fn min_width(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("min_width", value.into());
                self
            }
            pub fn max_width(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("max_width", value.into());
                self
            }
            pub fn max_height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("max_height", value.into());
                self
            }
            pub fn height_percent(mut self, value: f32) -> Self {
                self.inner
                    .style
                    .set_dimension("height", crate::Length::Percent(value));
                self
            }
            pub fn display(mut self, value: crate::Display) -> Self {
                self.inner.style.layout.display = value;
                self
            }
            pub fn flex_direction(mut self, value: crate::FlexDirection) -> Self {
                self.inner.style.layout.flex_direction = Some(value);
                self
            }
            pub fn flex_wrap(mut self, value: crate::FlexWrap) -> Self {
                self.inner.style.layout.flex_wrap = Some(value);
                self
            }
            pub fn box_sizing(mut self, value: crate::BoxSizing) -> Self {
                self.inner.style.layout.box_sizing = value;
                self
            }
            pub fn direction(mut self, value: crate::Direction) -> Self {
                self.inner.style.layout.direction = value;
                self
            }
            pub fn position(mut self, value: crate::Position) -> Self {
                self.inner.style.layout.position = value;
                self
            }
            pub fn grid_flow(mut self, value: crate::GridFlow) -> Self {
                self.inner.style.layout.grid_flow = value;
                self
            }
            pub fn align_self(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.align_self = Some(value);
                self
            }
            pub fn justify_self(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.justify_self = Some(value);
                self
            }
            pub fn justify_items(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.justify_items = Some(value);
                self
            }
            pub fn align_content(mut self, value: crate::Justify) -> Self {
                self.inner.style.layout.align_content = Some(value);
                self
            }
            pub fn aspect_ratio(mut self, value: f32) -> Self {
                self.inner.style.layout.aspect_ratio = Some(value);
                self
            }
            pub fn flex_basis(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.flex_basis = value.into();
                self
            }
            pub fn flex_line_count(mut self, value: u16) -> Self {
                self.inner.style.layout.flex_line_count = value;
                self
            }
            pub fn column_gap(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.gap[0] = Some(value.into());
                self
            }
            pub fn row_gap(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.gap[1] = Some(value.into());
                self
            }
            pub fn margin(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner
                    .style
                    .set_edges("margin", crate::Edges::all(value.into()));
                self
            }
            pub fn margin_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("margin", value);
                self
            }
            pub fn margin_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "left", value.into());
                self
            }
            pub fn margin_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "right", value.into());
                self
            }
            pub fn margin_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "top", value.into());
                self
            }
            pub fn margin_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "bottom", value.into());
                self
            }
            pub fn padding_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("padding", value);
                self
            }
            pub fn padding_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "left", value.into());
                self
            }
            pub fn padding_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "right", value.into());
                self
            }
            pub fn padding_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "top", value.into());
                self
            }
            pub fn padding_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "bottom", value.into());
                self
            }
            pub fn inset(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner
                    .style
                    .set_edges("inset", crate::Edges::all(value.into()));
                self
            }
            pub fn inset_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("inset", value);
                self
            }
            pub fn inset_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "left", value.into());
                self
            }
            pub fn inset_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "right", value.into());
                self
            }
            pub fn inset_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "top", value.into());
                self
            }
            pub fn inset_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "bottom", value.into());
                self
            }
            pub fn grid_row(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
                self.inner.style.layout.grid_row = [start, end];
                self
            }
            pub fn row_span(self, count: u16) -> Self {
                self.grid_row(crate::GridLine::Auto, crate::GridLine::Span(count))
            }
            pub fn grid_column(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
                self.inner.style.layout.grid_column = [start, end];
                self
            }
            pub fn column_span(self, count: u16) -> Self {
                self.grid_column(crate::GridLine::Auto, crate::GridLine::Span(count))
            }

            pub fn key(mut self, key: impl Into<Key>) -> Self {
                self.inner.key = key.into();
                self
            }
            pub fn weight(mut self, value: u16) -> Self {
                self.inner.style.font_weight = value;
                self
            }
            pub fn width_percent(mut self, value: f32) -> Self {
                self.inner
                    .style
                    .set_dimension("width", crate::Length::Percent(value));
                self
            }
            pub fn grow(mut self, value: f32) -> Self {
                self.inner.style.grow = value;
                self
            }
            pub fn font_size(mut self, px: u16) -> Self {
                self.inner.style.font_size = px;
                self
            }
            pub fn padding(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding", value.into());
                self
            }
            pub fn padding_x(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding_x", value.into());
                self
            }
            pub fn padding_y(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding_y", value.into());
                self
            }
            pub fn border(mut self, width: f32, color: Color) -> Self {
                self.inner.style.border_width = width;
                self.inner.style.border_color = color;
                self
            }
            pub fn hover_background(mut self, color: Color) -> Self {
                self.inner.style.hover_background = Some(color);
                self
            }
            pub fn pressed_background(mut self, color: Color) -> Self {
                self.inner.style.pressed_background = Some(color);
                self
            }
            pub fn min_height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("min_height", value.into());
                self
            }
            pub fn height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("height", value.into());
                self
            }
            pub fn translate(mut self, x: f32, y: f32) -> Self {
                self.inner.style.translate = [x, y];
                self
            }
            pub fn scale(mut self, x: f32, y: f32) -> Self {
                self.inner.style.scale = [x, y];
                self
            }
            pub fn opacity(mut self, value: f32) -> Self {
                self.inner.style.opacity = value;
                self
            }
            pub fn radius(mut self, px: f32) -> Self {
                self.inner.style.radius = px;
                self
            }
            pub fn background(mut self, color: Color) -> Self {
                self.inner.style.background = Some(color);
                self
            }
            pub fn motion(self, preset: Motion) -> Self {
                self.transition(preset.duration_ms())
            }
            pub fn transition(mut self, milliseconds: u16) -> Self {
                self.inner.style.transition_ms = milliseconds;
                self
            }
            pub fn color(mut self, color: Color) -> Self {
                self.inner.style.foreground = color;
                self
            }
            pub fn width(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("width", value.into());
                self
            }
        }
    };
}
common!(Input);
common!(Text);
common!(Button);
common!(ButtonBase);
common!(Toggle);
impl<'u, 'e, 't> Button<'u, 'e, 't> {
    pub(crate) fn composable(mut self) -> Self {
        self.composable = true;
        self
    }
    /// Standard text content; shares the same container as custom children.
    pub fn text(mut self, text: impl Into<Cow<'t, str>>) -> Self {
        self.inner.text = text.into();
        self.composable = true;
        self
    }
    /// Decorative leading icon, sized to match the button's text.
    pub fn icon(mut self, asset: &crate::SvgAsset) -> Self {
        self.icon = Some(*asset);
        self.composable = true;
        self
    }
    /// Accessible name override, required for a button with only an icon.
    pub fn aria_label(mut self, label: impl Into<Cow<'t, str>>) -> Self {
        self.accessible_name = Some(label.into());
        self.composable = true;
        self
    }
    /// Custom content replaces shortcut content. An explicit accessible name wins.
    pub fn children<R>(mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> InnerResponse<R> {
        let name = self
            .accessible_name
            .take()
            .unwrap_or_else(|| std::mem::take(&mut self.inner.text));
        self.inner.children(&name, true, build)
    }
    fn finish(&mut self) -> Response {
        if !self.composable {
            let enabled = !self.inner.style.disabled;
            return Response {
                clicked: self.inner.finish().1,
                enabled,
            };
        }
        let text = std::mem::take(&mut self.inner.text);
        let name = self.accessible_name.take();
        let icon = self.icon.take();
        self.inner
            .children(name.as_deref().unwrap_or(&text), true, |cx| {
                if let Some(asset) = icon {
                    let size = cx.inherited.font_size.saturating_add(2) as f32;
                    cx.svg(&asset)
                        .key(Key::Static("__button_icon"))
                        .size(size, size);
                }
                if !text.is_empty() {
                    cx.text(text.as_ref())
                        .key(Key::Static("__button_text"))
                        .ellipsis(true);
                }
            })
            .response
    }

    pub fn primary(mut self) -> Self {
        self.inner.style.background = Some(Color::Accent);
        self.inner.style.border_width = 0.;
        self.inner.style.foreground = Color::OnAccent;
        self
    }
    /// Use the surface color as a quiet button background.
    pub fn ghost(mut self) -> Self {
        self.inner.style.background = Some(Color::Surface);
        self.inner.style.border_width = 0.;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    pub fn response(mut self) -> Response {
        self.finish()
    }
    pub fn clicked(self) -> bool {
        self.response().clicked()
    }
}

impl crate::ImplicitKey for Text<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl crate::ImplicitKey for Button<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl crate::ImplicitKey for ButtonBase<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl Drop for Button<'_, '_, '_> {
    fn drop(&mut self) {
        if self.composable && self.inner.ui.is_some() {
            self.finish();
        }
    }
}

/// A controlled switch or checkbox. Configuration is applied before input is consumed.
/// Dropping the builder also submits it and updates the borrowed value synchronously.
pub struct Toggle<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    value: &'t mut bool,
}
impl crate::ImplicitKey for Toggle<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl Toggle<'_, '_, '_> {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    /// Space between the indicator and the label.
    pub fn gap(mut self, pixels: u16) -> Self {
        self.inner.style.gap = pixels as f32;
        self
    }
    fn finish(&mut self) -> ToggleResponse {
        let enabled = !self.inner.style.disabled;
        let changed = self.inner.finish_controlled(Some(&mut *self.value)).1;
        ToggleResponse {
            changed,
            checked: *self.value,
            enabled,
        }
    }
    pub fn response(mut self) -> ToggleResponse {
        self.finish()
    }
    pub fn changed(self) -> bool {
        self.response().changed()
    }
}
impl Drop for Toggle<'_, '_, '_> {
    fn drop(&mut self) {
        if self.inner.ui.is_some() {
            self.finish();
        }
    }
}
/// Copied result; retains neither the context nor the controlled value's borrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleResponse {
    changed: bool,
    checked: bool,
    enabled: bool,
}
impl ToggleResponse {
    pub const fn changed(self) -> bool {
        self.changed
    }
    pub const fn checked(self) -> bool {
        self.checked
    }
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

/// Decorative SVG with an explicit display size; source colors are preserved.
pub struct Svg<'u, 'e> {
    inner: Leaf<'u, 'e, 'static>,
}
impl crate::ImplicitKey for Svg<'_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl Svg<'_, '_> {
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.inner.style.width = Some(width);
        self.inner.style.height = Some(height);
        self
    }
    pub fn width_percent(mut self, value: f32) -> Self {
        self.inner
            .style
            .set_dimension("width", crate::Length::Percent(value));
        self
    }
    /// Resolve SVG currentColor against this token; explicit SVG fills stay unchanged.
    pub fn color(mut self, color: Color) -> Self {
        self.inner.style.foreground = color;
        self
    }
    pub fn opacity(mut self, value: f32) -> Self {
        self.inner.style.opacity = value;
        self
    }
    pub fn motion(mut self, preset: Motion) -> Self {
        self.inner.style.transition_ms = preset.duration_ms();
        self
    }
}

/// Event result copied out of a declaration; it retains no Ui borrow or callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response {
    clicked: bool,
    enabled: bool,
}
impl Response {
    pub(crate) const fn new(clicked: bool, enabled: bool) -> Self {
        Self { clicked, enabled }
    }
    pub const fn clicked(self) -> bool {
        self.clicked
    }
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InnerResponse<R> {
    pub inner: R,
    pub response: Response,
}

pub struct ButtonBase<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    styled: bool,
}
impl ButtonBase<'_, '_, '_> {
    pub(crate) fn semantic_role(mut self, role: crate::WidgetRole) -> Self {
        self.inner.style.role = Some(role);
        self
    }

    /// Expose disclosure state to assistive technology.
    pub fn expanded(mut self, value: bool) -> Self {
        self.inner.style.expanded = Some(value);
        self
    }
    pub fn selected(mut self, value: bool) -> Self {
        self.inner.style.selected = Some(value);
        self
    }
    pub fn gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.inner.style.set_spacing("gap", value.into());
        self
    }
    pub fn align(mut self, value: Align) -> Self {
        self.inner.style.align = Some(value);
        self
    }
    pub fn justify(mut self, value: Justify) -> Self {
        self.inner.style.justify = Some(value);
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.inner.style.disabled |= value;
        self
    }
    pub fn children<R>(mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> InnerResponse<R> {
        let name = std::mem::take(&mut self.inner.text);
        self.inner.children(&name, self.styled, build)
    }
}

/// A native editor binding; only its accessible label enters the display tree.
pub struct Input<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    buffer: crate::TextBuffer,
    placeholder: Cow<'t, str>,
    autofocus: bool,
    focus: bool,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct InputResponse {
    pub changed: bool,
    pub submitted: bool,
    pub navigation: Option<crate::InputNavigation>,
}
impl<'t> crate::ImplicitKey for Input<'_, '_, 't> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl<'t> Input<'_, '_, 't> {
    /// Explicitly reveal a secret in a supporting native editor.
    pub fn password_visible(mut self, visible: bool) -> Self {
        self.inner.style.input_password_visible = visible;
        self
    }
    /// Focus once when mounted; subsequent frames never steal focus.
    pub fn autofocus(mut self, enabled: bool) -> Self {
        self.autofocus = enabled;
        self
    }
    /// Explicit one-shot focus request. Applications pass true only on an action.
    pub fn request_focus(mut self, requested: bool) -> Self {
        self.focus = requested;
        self
    }
    /// Route unmodified Up/Down/PageUp/PageDown/Escape into this input's response.
    /// IME composition and ordinary editor shortcuts retain priority.
    pub fn navigation(mut self, enabled: bool) -> Self {
        self.inner.style.input_navigation = enabled;
        self
    }
    pub fn placeholder(mut self, hint: impl Into<Cow<'t, str>>) -> Self {
        self.placeholder = hint.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    fn finish(&mut self) -> InputResponse {
        let Some(ui) = self.inner.ui.take() else {
            return InputResponse::default();
        };
        self.inner.style.input_revision = self.buffer.revision();
        self.inner.style.disabled |= ui.inherited.disabled;
        let entity = ui.engine.declare(
            ui.parent,
            self.inner.key,
            ui.scope,
            Kind::TextInput,
            self.inner.style,
            std::mem::take(&mut self.inner.text),
        );
        if self.focus && !self.inner.style.disabled {
            ui.engine.focus_request = Some(entity);
        }
        let counters = self.buffer.counters();
        let mut response = ui.engine.inputs.get(&entity).map_or(
            InputResponse::default(),
            |(buffer, _, previous)| InputResponse {
                changed: buffer.same_storage(&self.buffer) && previous.0 != counters.0,
                submitted: buffer.same_storage(&self.buffer) && previous.1 != counters.1,
                navigation: None,
            },
        );
        match ui.engine.inputs.entry(entity) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let binding = entry.get_mut();
                binding.0 = self.buffer.clone();
                if binding.1 != self.placeholder {
                    self.buffer.invalidate();
                    let node = ui.engine.nodes.get_mut(entity).unwrap();
                    node.style.input_revision = self.buffer.revision();
                    node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
                    binding.1.clear();
                    binding.1.push_str(&self.placeholder);
                }
                binding.2 = counters;
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                if self.autofocus && !self.inner.style.disabled {
                    ui.engine.focus_request = Some(entity);
                }
                entry.insert((self.buffer.clone(), self.placeholder.to_string(), counters));
            }
        }
        if self.inner.style.disabled {
            InputResponse::default()
        } else {
            response.navigation = self.buffer.take_navigation();
            if response.changed || response.submitted || response.navigation.is_some() {
                ui.engine.changes.insert(Changes::VIEW);
            }
            response
        }
    }
    pub fn response(mut self) -> InputResponse {
        self.finish()
    }
}
impl Drop for Input<'_, '_, '_> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Custom WGSL surface; the builder and callback never enter the renderer.
pub struct Shader<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    params: crate::ShaderParams,
}
common!(Shader);
impl<'e> Ui<'e> {
    pub fn shader<'u>(&'u mut self, asset: &crate::ShaderAsset) -> Shader<'u, 'e, 'static> {
        let mut inner = self.leaf(Kind::Shader, Cow::Borrowed(asset.source()));
        inner.style.width = Some(320.);
        inner.style.height = Some(180.);
        inner.style.radius = 8.;
        inner.style.clip = true;
        Shader {
            inner,
            params: crate::ShaderParams {
                module: asset.is_module(),
                ..Default::default()
            },
        }
    }
    /// Portable host replay entry point; authored code normally uses shader().
    pub fn shader_element(
        &mut self,
        key: Key,
        style: Style,
        source: &str,
        params: &crate::ShaderParams,
    ) {
        if !params.valid() {
            self.engine.error.get_or_insert(FrameError::InvalidStyle);
        }
        let identity = Identity {
            parent: self.parent,
            key,
            scope: self.scope,
        };
        let mut style = style;
        if let Some(entity) = self.engine.lookup_shader(identity) {
            style.shader_revision = entity;
        }
        let entity = self.engine.declare(
            self.parent,
            key,
            self.scope,
            Kind::Shader,
            style,
            Cow::Borrowed(source),
        );
        let node = self.engine.nodes.get_mut(entity).unwrap();
        if node.shader.as_ref() != Some(params) {
            node.shader = Some(params.clone());
            node.style.shader_revision = node.style.shader_revision.wrapping_add(1);
            node.pending_changes |= Changes::PAINT;
        }
    }
}
impl crate::ImplicitKey for Shader<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl Shader<'_, '_, '_> {
    pub fn values(mut self, values: &[f32]) -> Self {
        self.params.values.clear();
        self.params.values.extend_from_slice(values);
        self
    }
    pub fn time(mut self, time: f32) -> Self {
        self.params.time = time;
        self
    }
    pub fn speed(mut self, speed: f32) -> Self {
        self.params.speed = speed;
        self
    }
    pub fn animated(mut self, animated: bool) -> Self {
        self.params.animated = animated;
        self
    }
    pub fn paused(mut self, paused: bool) -> Self {
        self.params.paused = paused;
        self
    }
}
impl Drop for Shader<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(ui) = self.inner.ui.take() {
            ui.shader_element(
                self.inner.key,
                self.inner.style,
                &self.inner.text,
                &self.params,
            );
        }
    }
}

/// Controlled numeric input; configuration is applied before consuming input.
pub struct Slider<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    value: &'t mut f64,
    range: crate::NumericRange,
}
common!(Slider);
impl<'e> Ui<'e> {
    pub fn slider<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        value: &'t mut f64,
        range: std::ops::RangeInclusive<f64>,
    ) -> Slider<'u, 'e, 't> {
        let height = self.theme().metrics.control_height as f32;
        let mut inner = self.leaf(Kind::Slider, label.into());
        inner.style.width = Some(240.);
        inner.style.height = Some(height);
        inner.style.min_height = Some(height);
        inner.style.transition_ms = Motion::Quick.duration_ms();
        let range = crate::NumericRange {
            min: *range.start(),
            max: *range.end(),
            value: *value,
            step: 0.,
            vertical: false,
            reversed: false,
        };
        Slider {
            inner,
            value,
            range,
        }
    }
    /// Numeric-event replay entry point for a portable declaration.
    pub fn slider_element(
        &mut self,
        key: Key,
        mut style: Style,
        label: &str,
        mut range: crate::NumericRange,
    ) -> Option<f64> {
        style.disabled |= self.inherited.disabled;
        if !range.valid() {
            self.engine.error.get_or_insert(FrameError::InvalidStyle);
        } else {
            range.value = range.normalize(range.value);
        }
        let identity = Identity {
            parent: self.parent,
            key,
            scope: self.scope,
        };
        if let Some(revision) = self.engine.lookup_numeric(identity) {
            style.numeric_revision = revision;
        }
        let entity = self.engine.declare(
            self.parent,
            key,
            self.scope,
            Kind::Slider,
            style,
            Cow::Borrowed(label),
        );
        let input = if style.disabled {
            None
        } else {
            self.engine.value(entity)
        };
        if let Some(value) = input
            && range.valid()
        {
            range.value = range.normalize(value);
        }
        let node = self.engine.nodes.get_mut(entity).unwrap();
        if node.numeric != Some(range) {
            node.numeric = Some(range);
            node.style.numeric_revision = node.style.numeric_revision.wrapping_add(1);
            node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
        }
        input.map(|_| range.value)
    }
}
impl crate::ImplicitKey for Slider<'_, '_, '_> {
    fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    fn site_id(mut self, site: u64) -> Self {
        if matches!(self.inner.key, Key::Auto(_)) {
            self.inner.key = Key::Generated(site);
        }
        self
    }
}
impl Slider<'_, '_, '_> {
    pub fn step(mut self, step: f64) -> Self {
        self.range.step = step;
        self
    }
    pub fn vertical(mut self, vertical: bool) -> Self {
        self.range.vertical = vertical;
        if vertical {
            self.inner.style.width = Some(32.);
            self.inner.style.height = Some(180.);
        }
        self
    }
    pub fn reversed(mut self, reversed: bool) -> Self {
        self.range.reversed = reversed;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    fn finish(&mut self) -> crate::SliderResponse {
        let Some(ui) = self.inner.ui.take() else {
            return crate::SliderResponse::default();
        };
        let before = *self.value;
        let range = &mut self.range;
        if range.valid() {
            *self.value = range.normalize(*self.value);
            range.value = *self.value;
        }
        let normalized = *self.value;
        let input = ui.slider_element(
            self.inner.key,
            self.inner.style,
            &self.inner.text,
            self.range,
        );
        if let Some(value) = input {
            let range = self.range;
            if range.valid() {
                *self.value = range.normalize(value);
            }
        }
        let changed = input.is_some() && normalized != *self.value;
        if before != *self.value {
            ui.request_rebuild();
        }
        crate::SliderResponse {
            changed,
            value: *self.value,
        }
    }
    pub fn response(mut self) -> crate::SliderResponse {
        self.finish()
    }
}
impl Drop for Slider<'_, '_, '_> {
    fn drop(&mut self) {
        self.finish();
    }
}

impl<F, R> std::ops::Add<F> for Button<'_, '_, '_>
where
    F: FnOnce(&mut Ui<'_>) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

impl<F, R> std::ops::Add<F> for ButtonBase<'_, '_, '_>
where
    F: FnOnce(&mut Ui<'_>) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}
