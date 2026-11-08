use crate::{Entity, Theme, engine::Engine, model::*};
use std::borrow::Cow;

/// Context borrowed while declaring one UI tree. Widget constructors live in `widgets`.
pub struct UiContext<'e> {
    pub(crate) engine: &'e mut Engine,
    pub(crate) localizer: &'e dyn crate::Localizer,
    pub(crate) parent: Entity,
    pub(crate) scope: u64,
    pub(crate) inherited: Style,
    pub(crate) in_button: bool,
}
impl<'e> UiContext<'e> {
    /// Translation context inherited by this declaration's children and portals.
    pub fn localizer(&self) -> &'e dyn crate::Localizer {
        self.localizer
    }
    /// Resolve a message without coupling its display text to widget identity.
    pub fn tr(&self, id: &str, fallback: &'static str) -> Cow<'e, str> {
        self.localizer.text(id, fallback)
    }
    /// Borrow an app-owned catalog for a synchronous declaration scope. No global
    /// locale or reference is retained; nested scopes and unwinding restore the
    /// outer context naturally. Regions must re-inject on each replay and declare
    /// the selected locale as an explicit dependency.
    pub fn localized<R>(
        &mut self,
        localizer: &dyn crate::Localizer,
        build: impl FnOnce(&mut UiContext<'_>) -> R,
    ) -> R {
        build(&mut UiContext {
            engine: &mut *self.engine,
            localizer,
            parent: self.parent,
            scope: self.scope,
            inherited: self.inherited,
            in_button: self.in_button,
        })
    }

    /// Component-local storage. Call sites are independent of declaration order.
    /// Use `keyed` around repeated component instances in dynamic lists.
    #[track_caller]
    pub fn state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> crate::LocalState<T> {
        let site = std::panic::Location::caller();
        self.engine
            .locals
            .get(self.parent, self.scope, site, self.engine.epoch, init)
    }

    /// Named storage retained across unmounts. The host chooses disk persistence.
    pub fn persisted_state<T: serde::Serialize + serde::de::DeserializeOwned + 'static>(
        &mut self,
        name: &str,
        init: impl FnOnce() -> T,
    ) -> crate::LocalState<T> {
        let key = self.storage_key(name);
        self.engine.storage.get(key, init)
    }
    fn storage_key(&self, name: &str) -> crate::storage::StorageKey {
        crate::storage::StorageKey {
            path: self.engine.logical_path(self.parent),
            scope: self.scope,
            name: name.to_owned(),
        }
    }
    /// Frame-to-frame state outside checkpoints entirely: toasts, drag
    /// gestures and other ephemeral host state that must not resurrect on
    /// restore. The value is never serialized, unlike `persisted_state`.
    pub fn transient_state<T: 'static>(
        &mut self,
        name: &str,
        init: impl FnOnce() -> T,
    ) -> crate::LocalState<T> {
        let key = self.storage_key(name);
        self.engine.storage.transient(key, init)
    }
    pub fn remove_persisted_state(&mut self, name: &str) -> bool {
        let key = self.storage_key(name);
        self.engine.storage.remove(&key)
    }
    /// Remove this exact logical parent/component namespace, not its descendants.
    pub fn clear_persisted_scope(&mut self) {
        let path = self.engine.logical_path(self.parent);
        self.engine.storage.clear_scope(&path, self.scope);
    }

    pub fn element<R>(
        &mut self,
        key: Key,
        kind: Kind,
        style: Style,
        text: &str,
        children: impl FnOnce(&mut UiContext<'_>) -> R,
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
        children: impl FnOnce(&mut UiContext<'_>) -> R,
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

    /// Low-level backend entry point for a root portal. Widget composition and
    /// open/close policy stay with the caller; IDs must be globally namespaced.
    pub fn portal_element<R>(
        &mut self,
        key: Key,
        kind: Kind,
        style: Style,
        text: &str,
        children: impl FnOnce(&mut UiContext<'_>) -> R,
    ) -> (R, bool) {
        let root = self.engine.root;
        let mut portal = UiContext {
            engine: self.engine,
            localizer: self.localizer,
            parent: root,
            scope: 0,
            inherited: Style::default(),
            in_button: false,
        };
        portal.element(key, kind, style, text, children)
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
    /// The entity the backend focused at the end of the previous frame.
    /// Controls also report this through `Response::focused`.
    pub fn focused(&self) -> Option<Entity> {
        self.engine.focused()
    }
    #[inline(always)]
    fn inherited_style(&self) -> Style {
        Style {
            font_size: self.inherited.font_size,
            font_weight: self.inherited.font_weight,
            font_family: self.inherited.font_family,
            foreground: self.inherited.foreground,
            disabled: self.inherited.disabled,
            ..Default::default()
        }
    }
    #[inline(always)]
    pub(crate) fn with_parent<R>(
        &mut self,
        parent: Entity,
        style: Style,
        in_button: bool,
        build: impl FnOnce(&mut UiContext<'_>) -> R,
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
    #[inline(always)]
    pub fn button_base<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
    ) -> ButtonBase<'u, 'e, 't> {
        ButtonBase {
            inner: self.leaf(Kind::ButtonBase, label.into()),
            styled: false,
        }
    }
    #[inline(always)]
    pub fn column(&mut self) -> Layout<'_, 'e> {
        self.layout(Kind::Column)
    }
    #[inline(always)]
    pub fn grid(&mut self) -> Layout<'_, 'e> {
        self.column().display(crate::Display::Grid)
    }
    #[inline(always)]
    pub fn row(&mut self) -> Layout<'_, 'e> {
        self.layout(Kind::Row)
    }
    #[inline(always)]
    fn layout(&mut self, kind: Kind) -> Layout<'_, 'e> {
        let key = self.engine.auto_key(self.parent);
        let style = self.inherited_style();
        Layout {
            ui: self,
            key,
            kind,
            style,
            grid: None,
            material: None,
        }
    }
    #[inline(always)]
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
        inner.style.hover_background = Some(Color::Control);
        inner.style.pressed_background = Some(Color::ControlHover);
        inner.style.transition_ms = m.interaction_ms;
        inner.style.clip = true;
        Input {
            inner,
            buffer: buffer.clone(),
            string: None,
            placeholder: Cow::Borrowed(""),
            autofocus: false,
            focus: false,
        }
    }
    /// Backend text-editing surface with caller-owned appearance. Retains only
    /// editor mechanics (selection, caret, IME and platform text operations).
    pub fn text_edit_surface<'u, 't>(
        &'u mut self,
        key: Key,
        style: Style,
        label: &'t str,
        buffer: &crate::TextBuffer,
    ) -> Input<'u, 'e, 't> {
        let mut inner = self.leaf(Kind::TextInput, Cow::Borrowed(label));
        inner.key = key;
        inner.style = style;
        Input {
            inner,
            buffer: buffer.clone(),
            string: None,
            placeholder: Cow::Borrowed(""),
            autofocus: false,
            focus: false,
        }
    }

    /// Edit an ordinary string while retaining caret, selection, and IME state
    /// at this call site. External replacements reset the editor without
    /// reporting a synthetic user `changed` response. Secret values must use
    /// [`Self::text_input`] with [`crate::TextBuffer::secret`] instead.
    #[track_caller]
    pub fn text_input_string<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        value: &'t mut String,
    ) -> Input<'u, 'e, 't> {
        let state = self.state(|| StringBinding::new(value.as_str()));
        let buffer = {
            let mut binding = state.value.borrow_mut();
            if binding.synced != *value {
                binding.buffer.set(value.clone().into());
                binding.synced.clone_from(value);
                binding.external_sync = true;
            }
            binding.buffer.clone()
        };
        let mut input = self.text_input(label, &buffer);
        input.string = Some((state, value));
        input
    }
    #[inline(always)]
    pub fn text<'u, 't>(&'u mut self, text: impl Into<Cow<'t, str>>) -> Text<'u, 'e, 't> {
        Text {
            inner: self.leaf(Kind::Text, text.into()),
        }
    }
    /// Declare empty text, typically as the baseline for a reactive binding.
    #[inline(always)]
    pub fn text_empty<'u, 't>(&'u mut self) -> Text<'u, 'e, 't> {
        self.text("")
    }
    /// Format synchronously into an engine-owned reusable scratch buffer.
    #[inline(always)]
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
    #[inline(always)]
    pub fn svg(&mut self, asset: &crate::SvgAsset) -> Svg<'_, 'e> {
        let mut inner = self.leaf(Kind::Svg, Cow::Borrowed(asset.source()));
        inner.style.width = Some(24.);
        inner.style.height = Some(24.);
        inner.style.shrink = 0.;
        Svg { inner }
    }
    /// Declare an embedded raster image; see [`crate::ImageAsset`].
    pub fn image<'u>(&'u mut self, asset: &'u crate::ImageAsset) -> Image<'u, 'e> {
        let mut inner = self.leaf(Kind::Image, Cow::Borrowed(asset.encoded_str()));
        inner.style.width = Some(96.);
        inner.style.height = Some(96.);
        inner.style.shrink = 0.;
        Image { inner }
    }
    /// Declare generated SVG markup without requiring a leaked static asset.
    #[inline(always)]
    pub fn svg_source(&mut self, source: String) -> Svg<'_, 'e> {
        let mut inner = self.leaf(Kind::Svg, Cow::Owned(source));
        inner.style.width = Some(24.);
        inner.style.height = Some(24.);
        inner.style.shrink = 0.;
        Svg { inner }
    }
    #[inline(always)]
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
    /// Root window visibility in logical coordinates, not the current container's
    /// layout or clip. Use `to_local` and intersect explicit container bounds for
    /// nested content. Drawing still needs the usual layout clip.
    pub fn viewport_region(&self) -> crate::VisibleRegion {
        crate::VisibleRegion::from_size(self.viewport_size())
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
    /// Whether the host pointer targets the current parent control. Used by
    /// composed framework controls and logical-path replay adapters.
    pub fn hovered(&self) -> bool {
        self.engine.hovered(self.parent)
    }
    /// Whether the pointer is inside this group's visible bounds, including
    /// noninteractive content and descendant controls. Hosts resolve geometry.
    pub fn hovered_within(&self) -> bool {
        self.engine.hovered_within(self.parent)
    }
    /// Whether the primary pointer press started on the current parent
    /// control and the button is still down. Drag surfaces use this to
    /// anchor a drag at the press and track it across frames.
    pub fn pressed(&self) -> bool {
        self.engine.pressed(self.parent)
    }
    /// Current host pointer position in window logical pixels, when the
    /// pointer is inside the window. Read during declarations; pointer
    /// movement triggers a settling re-declaration like other input state.
    pub fn pointer_position(&self) -> Option<(f64, f64)> {
        self.engine.pointer_position()
    }
    /// Request another declaration for a non-event update performed during show.
    pub fn request_rebuild(&mut self) {
        self.engine.changes.insert(Changes::VIEW);
    }
    #[doc(hidden)]
    /// Open a child declaration scope: node identity inside `build` is
    /// namespaced under `site`, so equal call sites in sibling scopes cannot
    /// collide. `keyed` is the public spelling; the component macro drives
    /// this directly with its generated site identity.
    #[doc(hidden)]
    pub fn scope<R>(&mut self, site: u64, build: impl FnOnce(&mut UiContext<'_>) -> R) -> R {
        let scope =
            crate::macro_support::site_id("plico.scope", 0) ^ self.scope.rotate_left(17) ^ site;
        let parent = self.parent;
        let previous_auto = self.engine.nodes.get(parent).unwrap().auto;
        self.engine.nodes.get_mut(parent).unwrap().auto = 0;
        let mut ui = UiContext {
            engine: &mut *self.engine,
            localizer: self.localizer,
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
    pub fn keyed<R>(
        &mut self,
        key: impl Into<Key>,
        build: impl FnOnce(&mut UiContext<'_>) -> R,
    ) -> R {
        let scope = crate::macro_support::key_seed(key.into());
        self.scope(scope, build)
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
    #[inline(always)]
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
        inner.style.hover_background = Some(Color::ControlHover);
        inner.style.pressed_background = Some(Color::ControlPressed);
        inner.style.transition_ms = metrics.interaction_ms;
        inner.style.padding = metrics.space(2) as f32;
        inner.style.gap = metrics.spacing as f32 * if kind == Kind::Switch { 1.5 } else { 2. };
        inner.style.radius = metrics.radius as f32;
        inner.style.min_height = Some(metrics.control_height as f32);
        Toggle { inner, value }
    }
    #[inline(always)]
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
            hovered: false,
            recycle_text: false,
            spans: Vec::new(),
            material: None,
        }
    }
}
pub struct Layout<'u, 'e> {
    ui: &'u mut UiContext<'e>,
    key: Key,
    kind: Kind,
    style: Style,
    grid: Option<crate::GridTemplate>,
    material: Option<crate::LiquidGlass>,
}
impl Layout<'_, '_> {
    /// Fill this container with a scene-backed material. Establishes a positioned
    /// container for its absolute background without overriding absolute layout.
    #[inline(always)]
    pub fn material(mut self, material: crate::LiquidGlass) -> Self {
        self.material = Some(material);
        self.style.background = None;
        if self.style.layout.position == crate::Position::Static {
            self.style.layout.position = crate::Position::Relative;
        }
        self
    }
    #[inline(always)]
    pub fn elevation(mut self, elevation: crate::Elevation) -> Self {
        elevation.apply(&mut self.style);
        self
    }

    /// Override this container's border while a descendant has native focus.
    /// Preserves the scoped identity and requires no application focus state.
    #[inline(always)]
    pub fn focus_within_border(mut self, width: f32, color: Color) -> Self {
        if self.ui.engine.focus_within(Identity {
            parent: self.ui.parent,
            key: self.key,
            scope: self.ui.scope,
        }) {
            self.style.border_width = width;
            self.style.border_color = color;
        }
        self
    }

    #[inline(always)]
    pub fn grid_columns(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().columns = tracks.into_iter().map(Into::into).collect();
        self
    }
    #[inline(always)]
    pub fn grid_rows(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().rows = tracks.into_iter().map(Into::into).collect();
        self
    }
    #[inline(always)]
    pub fn grid_auto_columns(
        mut self,
        tracks: impl IntoIterator<Item: Into<crate::Track>>,
    ) -> Self {
        self.grid.get_or_insert_default().auto_columns =
            tracks.into_iter().map(Into::into).collect();
        self
    }
    #[inline(always)]
    pub fn grid_auto_rows(mut self, tracks: impl IntoIterator<Item: Into<crate::Track>>) -> Self {
        self.grid.get_or_insert_default().auto_rows = tracks.into_iter().map(Into::into).collect();
        self
    }
    #[inline(always)]
    pub fn grid_areas(mut self, rows: impl IntoIterator<Item: Into<String>>) -> Self {
        self.grid.get_or_insert_default().areas = rows.into_iter().map(Into::into).collect();
        self
    }
    #[inline(always)]
    pub fn grid_area(mut self, name: impl Into<String>) -> Self {
        self.grid.get_or_insert_default().area = Some(name.into());
        self
    }
    #[inline(always)]
    pub fn min_height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("min_height", value.into());
        self
    }
    #[inline(always)]
    pub fn display(mut self, value: crate::Display) -> Self {
        self.style.layout.display = value;
        self
    }
    #[inline(always)]
    pub fn flex_direction(mut self, value: crate::FlexDirection) -> Self {
        self.style.layout.flex_direction = Some(value);
        self
    }
    #[inline(always)]
    pub fn flex_wrap(mut self, value: crate::FlexWrap) -> Self {
        self.style.layout.flex_wrap = Some(value);
        self
    }
    #[inline(always)]
    pub fn box_sizing(mut self, value: crate::BoxSizing) -> Self {
        self.style.layout.box_sizing = value;
        self
    }
    #[inline(always)]
    pub fn direction(mut self, value: crate::Direction) -> Self {
        self.style.layout.direction = value;
        self
    }
    #[inline(always)]
    pub fn position(mut self, value: crate::Position) -> Self {
        self.style.layout.position = value;
        self
    }
    #[inline(always)]
    pub fn grid_flow(mut self, value: crate::GridFlow) -> Self {
        self.style.layout.grid_flow = value;
        self
    }
    #[inline(always)]
    pub fn align_self(mut self, value: crate::Align) -> Self {
        self.style.layout.align_self = Some(value);
        self
    }
    #[inline(always)]
    pub fn justify_self(mut self, value: crate::Align) -> Self {
        self.style.layout.justify_self = Some(value);
        self
    }
    #[inline(always)]
    pub fn justify_items(mut self, value: crate::Align) -> Self {
        self.style.layout.justify_items = Some(value);
        self
    }
    #[inline(always)]
    pub fn align_content(mut self, value: crate::Justify) -> Self {
        self.style.layout.align_content = Some(value);
        self
    }
    #[inline(always)]
    pub fn aspect_ratio(mut self, value: f32) -> Self {
        self.style.layout.aspect_ratio = Some(value);
        self
    }
    #[inline(always)]
    pub fn flex_basis(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.flex_basis = value.into();
        self
    }
    #[inline(always)]
    pub fn flex_line_count(mut self, value: u16) -> Self {
        self.style.layout.flex_line_count = value;
        self
    }
    #[inline(always)]
    pub fn column_gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.gap[0] = Some(value.into());
        self
    }
    #[inline(always)]
    pub fn row_gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.layout.gap[1] = Some(value.into());
        self
    }
    #[inline(always)]
    pub fn margin(mut self, value: impl Into<crate::Length>) -> Self {
        self.style
            .set_edges("margin", crate::Edges::all(value.into()));
        self
    }
    #[inline(always)]
    pub fn margin_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("margin", value);
        self
    }
    #[inline(always)]
    pub fn margin_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "left", value.into());
        self
    }
    #[inline(always)]
    pub fn margin_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "right", value.into());
        self
    }
    #[inline(always)]
    pub fn margin_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "top", value.into());
        self
    }
    #[inline(always)]
    pub fn margin_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("margin", "bottom", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("padding", value);
        self
    }
    #[inline(always)]
    pub fn padding_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "left", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "right", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "top", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("padding", "bottom", value.into());
        self
    }
    #[inline(always)]
    pub fn inset(mut self, value: impl Into<crate::Length>) -> Self {
        self.style
            .set_edges("inset", crate::Edges::all(value.into()));
        self
    }
    #[inline(always)]
    pub fn inset_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
        self.style.set_edges("inset", value);
        self
    }
    #[inline(always)]
    pub fn inset_left(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "left", value.into());
        self
    }
    #[inline(always)]
    pub fn inset_right(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "right", value.into());
        self
    }
    #[inline(always)]
    pub fn inset_top(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "top", value.into());
        self
    }
    #[inline(always)]
    pub fn inset_bottom(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_edge("inset", "bottom", value.into());
        self
    }
    #[inline(always)]
    pub fn grid_row(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
        self.style.layout.grid_row = [start, end];
        self
    }
    #[inline(always)]
    pub fn row_span(self, count: u16) -> Self {
        self.grid_row(crate::GridLine::Auto, crate::GridLine::Span(count))
    }
    #[inline(always)]
    pub fn grid_column(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
        self.style.layout.grid_column = [start, end];
        self
    }
    #[inline(always)]
    pub fn column_span(self, count: u16) -> Self {
        self.grid_column(crate::GridLine::Auto, crate::GridLine::Span(count))
    }
    #[inline(always)]
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.style.translate = [x, y];
        self
    }
    #[inline(always)]
    pub fn scale(mut self, x: f32, y: f32) -> Self {
        self.style.scale = [x, y];
        self
    }
    /// Place this container relative to its containing block, outside flex flow.
    #[inline(always)]
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
    #[inline(always)]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = key.into();
        self
    }
    #[inline(always)]
    pub fn gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("gap", value.into());
        self
    }
    #[inline(always)]
    /// Inherit this generic family in descendant text and controls.
    pub fn font_family(mut self, family: crate::FontFamily) -> Self {
        self.style.font_family = family;
        self
    }
    #[inline(always)]
    pub fn padding(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_x(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding_x", value.into());
        self
    }
    #[inline(always)]
    pub fn padding_y(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_spacing("padding_y", value.into());
        self
    }
    #[inline(always)]
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style.border_width = width;
        self.style.border_color = color;
        self
    }
    #[inline(always)]
    pub fn width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("width", value.into());
        self
    }
    #[inline(always)]
    pub fn height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("height", value.into());
        self
    }
    #[inline(always)]
    pub fn width_percent(mut self, value: f32) -> Self {
        self.style
            .set_dimension("width", crate::Length::Percent(value));
        self
    }
    #[inline(always)]
    pub fn height_percent(mut self, value: f32) -> Self {
        self.style
            .set_dimension("height", crate::Length::Percent(value));
        self
    }
    #[inline(always)]
    pub fn min_width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("min_width", value.into());
        self
    }
    #[inline(always)]
    pub fn max_height(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("max_height", value.into());
        self
    }
    #[inline(always)]
    pub fn max_width(mut self, value: impl Into<crate::Length>) -> Self {
        self.style.set_dimension("max_width", value.into());
        self
    }
    #[inline(always)]
    pub fn grow(mut self, value: f32) -> Self {
        self.style.grow = value;
        self
    }
    #[inline(always)]
    pub fn shrink(mut self, value: f32) -> Self {
        self.style.shrink = value;
        self
    }
    /// A bounded vertical viewport. Its children keep their intrinsic height.
    /// Animate wheel scrolling and scrollbar track paging (160ms ease-out).
    #[inline(always)]
    pub fn smooth_scroll(mut self, enabled: bool) -> Self {
        self.style.smooth_scroll = enabled;
        self
    }
    #[inline(always)]
    pub fn selectable(mut self, value: bool) -> Self {
        self.style.selectable = value;
        self
    }
    #[inline(always)]
    pub fn scroll_y(mut self, value: bool) -> Self {
        self.style.scroll_y = value;
        self
    }
    #[inline(always)]
    pub fn clip(mut self, value: bool) -> Self {
        self.style.clip = value;
        self
    }
    #[inline(always)]
    pub fn wrap(mut self, value: bool) -> Self {
        self.style.wrap = value;
        self
    }
    #[inline(always)]
    pub fn align(mut self, value: Align) -> Self {
        self.style.align = Some(value);
        self
    }
    #[inline(always)]
    pub fn justify(mut self, value: Justify) -> Self {
        self.style.justify = Some(value);
        self
    }
    /// An outer box shadow, excluded from the container interior.
    #[inline(always)]
    pub fn shadow(mut self, shadow: crate::BoxShadow) -> Self {
        self.style.shadow = Some(shadow);
        self
    }
    /// Add a broad ambient shadow beneath the primary contact shadow.
    #[inline(always)]
    pub fn ambient_shadow(mut self, shadow: crate::BoxShadow) -> Self {
        self.style.ambient_shadow = Some(shadow);
        self
    }
    /// Clear both contact and ambient shadows.
    #[inline(always)]
    pub fn no_shadow(mut self) -> Self {
        self.style.shadow = None;
        self.style.ambient_shadow = None;
        self
    }
    /// Blur this container and its content, in logical Gaussian sigma units.
    #[inline(always)]
    pub fn blur(mut self, sigma: f32) -> Self {
        self.style.foreground_blur = sigma;
        self
    }
    /// Blur the already-painted application backdrop behind this container.
    #[inline(always)]
    pub fn backdrop_blur(mut self, sigma: f32) -> Self {
        self.style.backdrop_blur = sigma;
        self
    }
    /// Keep this container at least `top` logical pixels below the top of its
    /// nearest scrollable ancestor while its containing block is visible.
    /// This is CSS `position: sticky` for the top edge; the node still
    /// participates in normal flow.
    #[inline(always)]
    pub fn sticky(mut self, top: f32) -> Self {
        self.style.sticky_top = Some(top.max(0.));
        self
    }
    /// Tint only the background; text and child controls remain opaque.
    #[inline(always)]
    pub fn background_alpha(mut self, alpha: f32) -> Self {
        self.style.background_alpha = alpha;
        self
    }
    #[inline(always)]
    pub fn opacity(mut self, value: f32) -> Self {
        self.style.opacity = value;
        self
    }
    #[inline(always)]
    pub fn corner_smoothing(mut self, value: f32) -> Self {
        self.style.corner_smoothing = value;
        self
    }
    #[inline(always)]
    pub fn radius(mut self, px: f32) -> Self {
        self.style.radius = px;
        self
    }
    #[inline(always)]
    pub fn motion(self, preset: Motion) -> Self {
        self.transition(preset.duration_ms())
    }
    #[inline(always)]
    pub fn transition(mut self, milliseconds: u16) -> Self {
        self.style.transition_ms = milliseconds;
        self
    }
    #[inline(always)]
    pub fn background(mut self, color: Color) -> Self {
        self.style.background = Some(color);
        self
    }
    #[inline(always)]
    pub fn disabled(mut self, value: bool) -> Self {
        self.style.disabled |= value;
        self
    }
    /// Expose a semantic role for this container, such as a navigation
    /// landmark, a meter bar or a progress indicator. Composed widgets use
    /// it; interactive roles belong on the control that handles the input.
    #[inline(always)]
    pub(crate) fn semantic_role(mut self, role: crate::WidgetRole) -> Self {
        self.style.role = Some(role);
        self
    }

    /// Declare an empty container, such as a flex spacer or painted track.
    /// This is shorthand for `children(|_| {})` / `+ |_cx| {}`.
    #[inline(always)]
    pub fn end(self) -> InnerResponse<()> {
        self.children(|_| {})
    }

    /// Declare this container's children. Returns `InnerResponse<R>` so the
    /// result chains without parentheses, matching every other container:
    /// containers are never interactive, so their response reports only the
    /// inherited disabled state.
    #[inline(always)]
    pub fn children<R>(self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> InnerResponse<R> {
        let entity = self.ui.engine.declare(
            self.ui.parent,
            self.key,
            self.ui.scope,
            self.kind,
            self.style,
            Cow::Borrowed(""),
        );
        self.ui.engine.set_grid(entity, self.grid.as_ref());
        let inner = self.ui.with_parent(entity, self.style, false, |cx| {
            if let Some(material) = self.material {
                material.show(cx, self.style.radius, self.style.corner_smoothing);
            }
            build(cx)
        });
        InnerResponse {
            inner,
            response: Response::new(false, !self.style.disabled, false),
        }
    }
}
/// Native operator spelling for the consuming children operation.
/// The closure receives the child context; its return value is preserved.
impl crate::ImplicitKey for Layout<'_, '_> {
    fn key_of(&self) -> Key {
        self.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}
impl<F, R> std::ops::Add<F> for Layout<'_, '_>
where
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Output = InnerResponse<R>;

    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

struct Leaf<'u, 'e, 't> {
    ui: Option<&'u mut UiContext<'e>>,
    key: Key,
    kind: Kind,
    style: Style,
    text: Cow<'t, str>,
    hovered: bool,
    recycle_text: bool,
    spans: Vec<crate::TextSpan>,
    material: Option<crate::LiquidGlass>,
}
impl Leaf<'_, '_, '_> {
    #[inline(always)]
    fn children<R>(
        &mut self,
        name: &str,
        styled: bool,
        build: impl FnOnce(&mut UiContext<'_>) -> R,
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
        let mut material = self.material;
        if let Some(ref mut glass) = material {
            style.material_background = true;
            // Disabled controls retain a readable theme surface instead of
            // spending backdrop work on a translucent, noninteractive pane.
            if style.disabled {
                glass.enabled = false;
            }
            if style.background == Some(Color::Accent) {
                glass.tint = Color::Accent;
                glass.tint_opacity = 1.;
                glass.fallback = Color::Accent;
            }
            style.background = None;
            style.border_width = 0.;
            if style.layout.position == crate::Position::Static {
                style.layout.position = crate::Position::Relative;
            }
        }
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
            hovered: !style.disabled && ui.engine.hovered(entity),
            clicked: !style.disabled && ui.engine.clicked(entity),
            enabled: !style.disabled,
            focused: ui.engine.focused() == Some(entity),
        };
        ui.engine.set_grid(entity, None);
        ui.engine.set_spans(entity, &[]);
        let inner = ui.with_parent(entity, style, true, |cx| {
            if let Some(mut glass) = material {
                if glass.interactive && !style.disabled {
                    glass.show_interactive(
                        cx,
                        style.radius,
                        style.corner_smoothing,
                        response.hovered,
                        cx.pressed(),
                    );
                } else {
                    if !style.disabled {
                        if cx.pressed() {
                            glass.tint_opacity = (glass.tint_opacity + 0.12).min(1.);
                            glass.rim *= 0.65;
                        } else if response.hovered {
                            glass.tint_opacity = (glass.tint_opacity + 0.06).min(1.);
                            glass.rim = (glass.rim + 0.15).min(1.);
                        }
                    }
                    glass.show(cx, style.radius, style.corner_smoothing);
                }
            }
            build(cx)
        });
        InnerResponse { inner, response }
    }
    #[inline(always)]
    fn finish(&mut self) -> (Entity, bool) {
        self.finish_controlled(None)
    }
    #[inline(always)]
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
        self.hovered = !self.style.disabled && ui.engine.hovered(entity);
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
    /// Finish this leaf and return its generational target for property bindings.
    #[doc(hidden)]
    #[cfg(feature = "experiment-patch")]
    pub fn experiment_property_target(
        mut self,
    ) -> (crate::ExperimentContext, String, crate::ExperimentTextStyle) {
        let text = self.inner.text.to_string();
        let attributes = self.inner.style.into();
        let mut context = self.inner.ui.as_ref().unwrap().experiment_context();
        context.parent = self.inner.finish().0;
        (context, text, attributes)
    }
    #[inline(always)]
    pub fn spans(mut self, spans: &[crate::TextSpan]) -> Self {
        self.inner.spans.extend_from_slice(spans);
        self
    }
    /// Keep this paragraph on one line and truncate at the width limit.
    #[inline(always)]
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
            crate::macro_support::text_style_methods!(owned(inner.style));
            #[inline(always)]
            pub fn min_width(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("min_width", value.into());
                self
            }
            #[inline(always)]
            pub fn max_width(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("max_width", value.into());
                self
            }
            #[inline(always)]
            pub fn max_height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("max_height", value.into());
                self
            }
            #[inline(always)]
            pub fn height_percent(mut self, value: f32) -> Self {
                self.inner
                    .style
                    .set_dimension("height", crate::Length::Percent(value));
                self
            }
            #[inline(always)]
            pub fn display(mut self, value: crate::Display) -> Self {
                self.inner.style.layout.display = value;
                self
            }
            #[inline(always)]
            pub fn flex_direction(mut self, value: crate::FlexDirection) -> Self {
                self.inner.style.layout.flex_direction = Some(value);
                self
            }
            #[inline(always)]
            pub fn flex_wrap(mut self, value: crate::FlexWrap) -> Self {
                self.inner.style.layout.flex_wrap = Some(value);
                self
            }
            #[inline(always)]
            pub fn box_sizing(mut self, value: crate::BoxSizing) -> Self {
                self.inner.style.layout.box_sizing = value;
                self
            }
            #[inline(always)]
            pub fn direction(mut self, value: crate::Direction) -> Self {
                self.inner.style.layout.direction = value;
                self
            }
            /// Position this leaf relative to its immediate containing block.
            #[inline(always)]
            pub fn absolute(mut self, left: f32, top: f32) -> Self {
                self.inner.style.absolute = Some([left, top]);
                self.inner.style.layout.position = crate::Position::Absolute;
                self
            }
            #[inline(always)]
            pub fn position(mut self, value: crate::Position) -> Self {
                self.inner.style.layout.position = value;
                self
            }
            #[inline(always)]
            pub fn grid_flow(mut self, value: crate::GridFlow) -> Self {
                self.inner.style.layout.grid_flow = value;
                self
            }
            #[inline(always)]
            pub fn align_self(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.align_self = Some(value);
                self
            }
            #[inline(always)]
            pub fn justify_self(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.justify_self = Some(value);
                self
            }
            #[inline(always)]
            pub fn justify_items(mut self, value: crate::Align) -> Self {
                self.inner.style.layout.justify_items = Some(value);
                self
            }
            #[inline(always)]
            pub fn align_content(mut self, value: crate::Justify) -> Self {
                self.inner.style.layout.align_content = Some(value);
                self
            }
            #[inline(always)]
            pub fn aspect_ratio(mut self, value: f32) -> Self {
                self.inner.style.layout.aspect_ratio = Some(value);
                self
            }
            #[inline(always)]
            pub fn flex_basis(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.flex_basis = value.into();
                self
            }
            #[inline(always)]
            pub fn flex_line_count(mut self, value: u16) -> Self {
                self.inner.style.layout.flex_line_count = value;
                self
            }
            #[inline(always)]
            pub fn column_gap(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.gap[0] = Some(value.into());
                self
            }
            #[inline(always)]
            pub fn row_gap(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.layout.gap[1] = Some(value.into());
                self
            }
            #[inline(always)]
            pub fn margin(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner
                    .style
                    .set_edges("margin", crate::Edges::all(value.into()));
                self
            }
            #[inline(always)]
            pub fn margin_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("margin", value);
                self
            }
            #[inline(always)]
            pub fn margin_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "left", value.into());
                self
            }
            #[inline(always)]
            pub fn margin_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "right", value.into());
                self
            }
            #[inline(always)]
            pub fn margin_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "top", value.into());
                self
            }
            #[inline(always)]
            pub fn margin_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("margin", "bottom", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("padding", value);
                self
            }
            #[inline(always)]
            pub fn padding_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "left", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "right", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "top", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("padding", "bottom", value.into());
                self
            }
            #[inline(always)]
            pub fn inset(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner
                    .style
                    .set_edges("inset", crate::Edges::all(value.into()));
                self
            }
            #[inline(always)]
            pub fn inset_edges(mut self, value: crate::Edges<crate::Length>) -> Self {
                self.inner.style.set_edges("inset", value);
                self
            }
            #[inline(always)]
            pub fn inset_left(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "left", value.into());
                self
            }
            #[inline(always)]
            pub fn inset_right(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "right", value.into());
                self
            }
            #[inline(always)]
            pub fn inset_top(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "top", value.into());
                self
            }
            #[inline(always)]
            pub fn inset_bottom(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_edge("inset", "bottom", value.into());
                self
            }
            #[inline(always)]
            pub fn grid_row(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
                self.inner.style.layout.grid_row = [start, end];
                self
            }
            #[inline(always)]
            pub fn row_span(self, count: u16) -> Self {
                self.grid_row(crate::GridLine::Auto, crate::GridLine::Span(count))
            }
            #[inline(always)]
            pub fn grid_column(mut self, start: crate::GridLine, end: crate::GridLine) -> Self {
                self.inner.style.layout.grid_column = [start, end];
                self
            }
            #[inline(always)]
            pub fn column_span(self, count: u16) -> Self {
                self.grid_column(crate::GridLine::Auto, crate::GridLine::Span(count))
            }
            #[inline(always)]
            pub fn key(mut self, key: impl Into<Key>) -> Self {
                self.inner.key = key.into();
                self
            }
            #[inline(always)]
            pub fn width_percent(mut self, value: f32) -> Self {
                self.inner
                    .style
                    .set_dimension("width", crate::Length::Percent(value));
                self
            }
            #[inline(always)]
            pub fn grow(mut self, value: f32) -> Self {
                self.inner.style.grow = value;
                self
            }
            #[inline(always)]
            pub fn shrink(mut self, value: f32) -> Self {
                self.inner.style.shrink = value;
                self
            }
            #[inline(always)]
            pub fn padding(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_x(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding_x", value.into());
                self
            }
            #[inline(always)]
            pub fn padding_y(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_spacing("padding_y", value.into());
                self
            }
            #[inline(always)]
            pub fn border(mut self, width: f32, color: Color) -> Self {
                self.inner.style.border_width = width;
                self.inner.style.border_color = color;
                self
            }
            #[inline(always)]
            pub fn hover_background(mut self, color: Color) -> Self {
                self.inner.style.hover_background = Some(color);
                self
            }
            #[inline(always)]
            pub fn pressed_background(mut self, color: Color) -> Self {
                self.inner.style.pressed_background = Some(color);
                self
            }
            #[inline(always)]
            pub fn min_height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("min_height", value.into());
                self
            }
            #[inline(always)]
            pub fn height(mut self, value: impl Into<crate::Length>) -> Self {
                self.inner.style.set_dimension("height", value.into());
                self
            }
            #[inline(always)]
            pub fn translate(mut self, x: f32, y: f32) -> Self {
                self.inner.style.translate = [x, y];
                self
            }
            #[inline(always)]
            pub fn scale(mut self, x: f32, y: f32) -> Self {
                self.inner.style.scale = [x, y];
                self
            }
            #[inline(always)]
            pub fn corner_smoothing(mut self, value: f32) -> Self {
                self.inner.style.corner_smoothing = value;
                self
            }
            #[inline(always)]
            pub fn elevation(mut self, elevation: crate::Elevation) -> Self {
                elevation.apply(&mut self.inner.style);
                self
            }
            /// Clear both contact and ambient shadows.
            #[inline(always)]
            pub fn no_shadow(mut self) -> Self {
                self.inner.style.shadow = None;
                self.inner.style.ambient_shadow = None;
                self
            }
            /// Blur this node and its content, in logical Gaussian sigma units.
            #[inline(always)]
            pub fn blur(mut self, sigma: f32) -> Self {
                self.inner.style.foreground_blur = sigma;
                self
            }
            /// Add a broad ambient shadow beneath the primary contact shadow.
            #[inline(always)]
            pub fn ambient_shadow(mut self, shadow: crate::BoxShadow) -> Self {
                self.inner.style.ambient_shadow = Some(shadow);
                self
            }
            /// An outer box shadow, excluded from the control interior.
            #[inline(always)]
            pub fn shadow(mut self, shadow: crate::BoxShadow) -> Self {
                self.inner.style.shadow = Some(shadow);
                self
            }
            #[inline(always)]
            pub fn radius(mut self, px: f32) -> Self {
                self.inner.style.radius = px;
                self
            }
            /// Tint only the background, preserving opaque control content.
            #[inline(always)]
            pub fn background_alpha(mut self, alpha: f32) -> Self {
                self.inner.style.background_alpha = alpha;
                self
            }
            #[inline(always)]
            pub fn background(mut self, color: Color) -> Self {
                self.inner.style.background = Some(color);
                self
            }
            #[inline(always)]
            pub fn motion(self, preset: Motion) -> Self {
                self.transition(preset.duration_ms())
            }
            #[inline(always)]
            pub fn transition(mut self, milliseconds: u16) -> Self {
                self.inner.style.transition_ms = milliseconds;
                self
            }
            #[inline(always)]
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
    /// Paint scene-backed glass beneath sharp button content. Primary buttons
    /// tint the material with the accent; disabled materials use an opaque fallback.
    /// Hover and press adjust the material without changing layout or hit testing.
    pub fn material(mut self, material: crate::LiquidGlass) -> Self {
        self.inner.material = Some(material);
        self.composable = true;
        self
    }
    /// Alignment of shortcut text/icon content along the button's main axis.
    pub fn justify(mut self, value: Justify) -> Self {
        self.inner.style.justify = Some(value);
        self
    }
    #[inline(always)]
    pub(crate) fn composable(mut self) -> Self {
        self.composable = true;
        self
    }
    /// Standard text content; shares the same container as custom children.
    #[inline(always)]
    pub fn text(mut self, text: impl Into<Cow<'t, str>>) -> Self {
        self.inner.text = text.into();
        self.composable = true;
        self
    }
    /// Decorative leading icon, sized to match the button's text.
    #[inline(always)]
    pub fn icon(mut self, asset: &crate::SvgAsset) -> Self {
        self.icon = Some(*asset);
        self.composable = true;
        self
    }
    /// Accessible name override, required for a button with only an icon.
    #[inline(always)]
    pub fn aria_label(mut self, label: impl Into<Cow<'t, str>>) -> Self {
        self.accessible_name = Some(label.into());
        self.composable = true;
        self
    }
    /// Custom content replaces shortcut content. An explicit accessible name wins.
    #[inline(always)]
    pub fn children<R>(mut self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> InnerResponse<R> {
        let name = self
            .accessible_name
            .take()
            .unwrap_or_else(|| std::mem::take(&mut self.inner.text));
        self.inner.children(&name, true, build)
    }
    #[inline(always)]
    fn finish(&mut self) -> Response {
        self.finish_target().0
    }
    fn finish_target(&mut self) -> (Response, crate::Entity) {
        if !self.composable {
            let enabled = !self.inner.style.disabled;
            let focused = self.inner.ui.as_ref().and_then(|ui| ui.engine.focused());
            let (entity, clicked) = self.inner.finish();
            return (
                Response {
                    clicked,
                    enabled,
                    hovered: self.inner.hovered,
                    focused: focused == Some(entity),
                },
                entity,
            );
        }
        let text = std::mem::take(&mut self.inner.text);
        let name = self.accessible_name.take();
        let icon = self.icon.take();
        let focused = self.inner.ui.as_ref().and_then(|ui| ui.engine.focused());
        let mut entity = None;
        let response = self
            .inner
            .children(name.as_deref().unwrap_or(&text), true, |cx| {
                entity = Some(cx.parent);
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
            .response;
        let entity = entity.expect("button declaration");
        (response.with_focused(focused == Some(entity)), entity)
    }
    /// Finish this control and retain its identity for a property subscription.
    #[doc(hidden)]
    #[cfg(feature = "experiment-patch")]
    pub fn experiment_property_target(
        mut self,
    ) -> (
        crate::ExperimentContext,
        crate::ExperimentButtonStyle,
        Response,
    ) {
        let mut attributes: crate::ExperimentButtonStyle = self.inner.style.into();
        attributes.styled_button = self.composable;
        let mut context = self.inner.ui.as_ref().unwrap().experiment_context();
        let (response, entity) = self.finish_target();
        context.parent = entity;
        (context, attributes, response)
    }
    #[inline(always)]
    pub fn primary(mut self) -> Self {
        self.inner.style.background = Some(Color::Accent);
        self.inner.style.border_width = 0.;
        self.inner.style.foreground = Color::OnAccent;
        self
    }
    /// Use the surface color as a quiet button background.
    #[inline(always)]
    pub fn ghost(mut self) -> Self {
        self.inner.style.background = Some(Color::Surface);
        self.inner.style.hover_background = Some(Color::ControlHover);
        self.inner.style.pressed_background = Some(Color::ControlPressed);
        self.inner.style.border_width = 0.;
        self
    }
    #[inline(always)]
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
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl crate::ImplicitKey for Button<'_, '_, '_> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl crate::ImplicitKey for ButtonBase<'_, '_, '_> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
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
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl Toggle<'_, '_, '_> {
    #[inline(always)]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    /// Space between the indicator and the label.
    #[inline(always)]
    pub fn gap(mut self, pixels: u16) -> Self {
        self.inner.style.gap = pixels as f32;
        self
    }
    #[inline(always)]
    fn finish(&mut self) -> ToggleResponse {
        let enabled = !self.inner.style.disabled;
        let focused = self.inner.ui.as_ref().and_then(|ui| ui.engine.focused());
        let (entity, changed) = self.inner.finish_controlled(Some(&mut *self.value));
        ToggleResponse {
            changed,
            checked: *self.value,
            enabled,
            focused: focused == Some(entity),
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
    focused: bool,
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
    /// Whether the backend had this control focused when it was declared.
    pub const fn focused(self) -> bool {
        self.focused
    }
}

/// Decorative SVG with an explicit display size; source colors are preserved.
pub struct Svg<'u, 'e> {
    inner: Leaf<'u, 'e, 'static>,
}

/// A decoded raster image. The asset's encoded bytes are carried hex-encoded
/// in the node's text; the backend decodes and caches by content.
pub struct Image<'u, 'e> {
    inner: Leaf<'u, 'e, 'u>,
}
impl crate::ImplicitKey for Image<'_, '_> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl Image<'_, '_> {
    /// Display size in logical pixels; the aspect ratio is not preserved
    /// automatically — the caller stretches or letterboxes explicitly.
    #[inline(always)]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.inner.style.width = Some(width);
        self.inner.style.height = Some(height);
        self
    }
    /// The declared display width, when set.
    pub fn width(&self) -> Option<f32> {
        self.inner.style.width
    }
    /// The declared display height, when set.
    pub fn height(&self) -> Option<f32> {
        self.inner.style.height
    }
}
impl crate::ImplicitKey for Svg<'_, '_> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl Svg<'_, '_> {
    #[inline(always)]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.inner.key = key.into();
        self
    }
    #[inline(always)]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.inner.style.width = Some(width);
        self.inner.style.height = Some(height);
        self
    }
    #[inline(always)]
    pub fn width_percent(mut self, value: f32) -> Self {
        self.inner
            .style
            .set_dimension("width", crate::Length::Percent(value));
        self
    }
    /// Resolve SVG currentColor against this token; explicit SVG fills stay unchanged.
    #[inline(always)]
    pub fn color(mut self, color: Color) -> Self {
        self.inner.style.foreground = color;
        self
    }
    #[inline(always)]
    pub fn opacity(mut self, value: f32) -> Self {
        self.inner.style.opacity = value;
        self
    }
    #[inline(always)]
    pub fn motion(mut self, preset: Motion) -> Self {
        self.inner.style.transition_ms = preset.duration_ms();
        self
    }
}

/// Event result copied out of a declaration; it retains no Ui borrow or callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response {
    hovered: bool,
    clicked: bool,
    enabled: bool,
    focused: bool,
}
impl Response {
    pub(crate) const fn new(clicked: bool, enabled: bool, hovered: bool) -> Self {
        Self {
            clicked,
            enabled,
            hovered,
            focused: false,
        }
    }
    pub(crate) const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
    pub const fn clicked(self) -> bool {
        self.clicked
    }
    /// Persistent pointer-over state. Independent of clicked, which can also
    /// come from keyboard/accessibility activation. Repeated reads do not consume it.
    pub const fn hovered(self) -> bool {
        self.hovered
    }
    pub const fn enabled(self) -> bool {
        self.enabled
    }
    /// Whether the backend had this control focused when it was declared.
    pub const fn focused(self) -> bool {
        self.focused
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
    /// Scene-backed glass for custom button content, with native button semantics.
    pub fn material(mut self, material: crate::LiquidGlass) -> Self {
        self.inner.material = Some(material);
        self
    }
    #[inline(always)]
    pub(crate) fn semantic_role(mut self, role: crate::WidgetRole) -> Self {
        self.inner.style.role = Some(role);
        self
    }

    /// Expose disclosure state to assistive technology.
    #[inline(always)]
    pub fn expanded(mut self, value: bool) -> Self {
        self.inner.style.expanded = Some(value);
        self
    }
    #[inline(always)]
    pub fn selected(mut self, value: bool) -> Self {
        self.inner.style.selected = Some(value);
        self
    }
    #[inline(always)]
    pub fn gap(mut self, value: impl Into<crate::Length>) -> Self {
        self.inner.style.set_spacing("gap", value.into());
        self
    }
    #[inline(always)]
    pub fn align(mut self, value: Align) -> Self {
        self.inner.style.align = Some(value);
        self
    }
    #[inline(always)]
    pub fn justify(mut self, value: Justify) -> Self {
        self.inner.style.justify = Some(value);
        self
    }
    #[inline(always)]
    pub fn disabled(mut self, value: bool) -> Self {
        self.inner.style.disabled |= value;
        self
    }
    #[inline(always)]
    pub fn children<R>(mut self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> InnerResponse<R> {
        let name = std::mem::take(&mut self.inner.text);
        self.inner.children(&name, self.styled, build)
    }
}

struct StringBinding {
    buffer: crate::TextBuffer,
    synced: String,
    external_sync: bool,
}
impl StringBinding {
    fn new(value: &str) -> Self {
        let buffer = crate::TextBuffer::new();
        buffer.set(value.to_owned().into());
        Self {
            buffer,
            synced: value.to_owned(),
            external_sync: false,
        }
    }
}

/// A native editor binding; only its accessible label enters the display tree.
pub struct Input<'u, 'e, 't> {
    inner: Leaf<'u, 'e, 't>,
    buffer: crate::TextBuffer,
    string: Option<(crate::LocalState<StringBinding>, &'t mut String)>,
    placeholder: Cow<'t, str>,
    autofocus: bool,
    focus: bool,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct InputResponse {
    pub changed: bool,
    pub submitted: bool,
    pub navigation: Option<crate::InputNavigation>,
    /// Whether the backend focused this field when it was declared.
    pub focused: bool,
}
impl<'t> crate::ImplicitKey for Input<'_, '_, 't> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl<'t> Input<'_, '_, 't> {
    /// Keep native input, caret and semantics, with visuals supplied by a parent.
    /// The parent must provide an appropriate focus indication.
    #[inline(always)]
    pub fn unstyled(mut self) -> Self {
        self.inner.style.control_chrome = false;
        self.inner.style.background = None;
        self.inner.style.border_width = 0.;
        self
    }

    /// Explicitly reveal a secret in a supporting native editor.
    #[inline(always)]
    pub fn password_visible(mut self, visible: bool) -> Self {
        self.inner.style.input_password_visible = visible;
        self
    }
    /// Focus once when mounted; subsequent frames never steal focus.
    #[inline(always)]
    pub fn autofocus(mut self, enabled: bool) -> Self {
        self.autofocus = enabled;
        self
    }
    /// Explicit one-shot focus request. Applications pass true only on an action.
    #[inline(always)]
    pub fn request_focus(mut self, requested: bool) -> Self {
        self.focus = requested;
        self
    }
    /// Route unmodified Up/Down/PageUp/PageDown/Escape into this input's response.
    /// IME composition and ordinary editor shortcuts retain priority.
    #[inline(always)]
    pub fn navigation(mut self, enabled: bool) -> Self {
        self.inner.style.input_navigation = enabled;
        self
    }
    #[inline(always)]
    pub fn placeholder(mut self, hint: impl Into<Cow<'t, str>>) -> Self {
        self.placeholder = hint.into();
        self
    }
    #[inline(always)]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    #[inline(always)]
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
        let focused = ui.engine.focused() == Some(entity);
        let mut response = ui.engine.inputs.get(&entity).map_or(
            InputResponse::default(),
            |(buffer, _, previous)| InputResponse {
                changed: buffer.same_storage(&self.buffer) && previous.0 != counters.0,
                submitted: buffer.same_storage(&self.buffer) && previous.1 != counters.1,
                navigation: None,
                focused,
            },
        );
        response.focused = focused;
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
        if let Some((state, target)) = &mut self.string {
            let mut binding = state.value.borrow_mut();
            if binding.external_sync {
                response.changed = false;
                binding.external_sync = false;
            } else if response.changed {
                if self.inner.style.disabled {
                    self.buffer.set((**target).clone().into());
                    binding.external_sync = true;
                } else {
                    let text = self.buffer.text().unwrap_or_default();
                    **target = text.clone();
                    binding.synced = text;
                }
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
impl<'e> UiContext<'e> {
    pub fn shader<'u>(&'u mut self, asset: &crate::ShaderAsset) -> Shader<'u, 'e, 'static> {
        let mut inner = self.leaf(Kind::Shader, Cow::Borrowed(asset.source()));
        inner.style.width = Some(320.);
        inner.style.height = Some(180.);
        inner.style.radius = 8.;
        inner.style.clip = true;
        Shader {
            inner,
            params: crate::ShaderParams {
                source_path: asset.source_path().map(str::to_owned),
                module: asset.is_module(),
                sksl: asset.sksl().map(str::to_owned),
                reads_backdrop: asset.reads_backdrop(),
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
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl Shader<'_, '_, '_> {
    /// Interpolate one scalar on target changes, using the injected native clock.
    pub fn transition_value(mut self, index: usize, milliseconds: u16) -> Self {
        let value = crate::ShaderTransition {
            index,
            milliseconds,
        };
        if let Some(existing) = self
            .params
            .transitions
            .iter_mut()
            .find(|t| t.index == index)
        {
            *existing = value;
        } else {
            self.params.transitions.push(value);
        }
        self
    }
    /// Expand only the painted canvas. Parent clips still apply.
    pub fn paint_outset(mut self, x: f32, y: f32) -> Self {
        self.params.paint_outset = [x, y];
        self
    }
    #[inline(always)]
    pub fn values(mut self, values: &[f32]) -> Self {
        self.params.values.clear();
        self.params.values.extend_from_slice(values);
        self
    }
    /// Set a complete WGSL module's top-level f32/vec2/vec3/vec4 uniform field.
    /// The host checks WGSL types/offsets; repeated names replace earlier values.
    /// Reserved width/height/time fields cannot be written. Do not mix with values().
    pub fn param(mut self, name: &str, value: impl Into<crate::ShaderValue>) -> Self {
        let value = value.into();
        if let Some((_, existing)) = self
            .params
            .parameters
            .iter_mut()
            .find(|(key, _)| key == name)
        {
            *existing = value;
        } else {
            self.params.parameters.push((name.to_owned(), value));
        }
        self
    }
    #[inline(always)]
    pub fn time(mut self, time: f32) -> Self {
        self.params.time = time;
        self
    }
    #[inline(always)]
    pub fn speed(mut self, speed: f32) -> Self {
        self.params.speed = speed;
        self
    }
    #[inline(always)]
    pub fn animated(mut self, animated: bool) -> Self {
        self.params.animated = animated;
        self
    }
    #[inline(always)]
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
    high: Option<&'t mut f64>,
    range: crate::NumericRange,
}
common!(Slider);
impl<'e> UiContext<'e> {
    pub fn slider<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        value: &'t mut f64,
        range: std::ops::RangeInclusive<f64>,
    ) -> Slider<'u, 'e, 't> {
        let metrics = self.theme().metrics;
        let height = metrics.control_height as f32;
        let mut inner = self.leaf(Kind::Slider, label.into());
        inner.style.width = Some(240.);
        inner.style.height = Some(height);
        inner.style.min_height = Some(height);
        inner.style.radius = metrics.radius as f32;
        inner.style.hover_background = Some(Color::ControlHover);
        inner.style.transition_ms = Motion::Quick.duration_ms();
        let range = crate::NumericRange {
            min: *range.start(),
            max: *range.end(),
            value: *value,
            high: None,
            step: 0.,
            vertical: false,
            reversed: false,
        };
        Slider {
            inner,
            value,
            high: None,
            range,
        }
    }
    pub fn range_slider<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        low: &'t mut f64,
        high: &'t mut f64,
        range: std::ops::RangeInclusive<f64>,
    ) -> Slider<'u, 'e, 't> {
        let mut slider = self.slider(label, low, range);
        if *high < *slider.value {
            *high = *slider.value;
        }
        slider.range.high = Some(*high);
        slider.high = Some(high);
        slider
    }
    /// Numeric-event replay entry point for a portable declaration.
    pub fn slider_element(
        &mut self,
        key: Key,
        mut style: Style,
        label: &str,
        range: &mut crate::NumericRange,
    ) -> Option<f64> {
        style.disabled |= self.inherited.disabled;
        if !range.valid() {
            self.engine.error.get_or_insert(FrameError::InvalidStyle);
        } else {
            range.value = range.normalize(range.value);
            if let Some(high) = range.high {
                range.high = Some(range.normalize(high).max(range.value));
            }
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
        let high_input = if style.disabled {
            None
        } else {
            self.engine.high_value(entity)
        };
        if let Some(value) = input
            && range.valid()
        {
            range.value = range.normalize(value);
        }
        if let Some(high) = high_input
            && range.valid()
        {
            let high = range.normalize(high).max(range.value);
            range.value = range.value.min(high);
            range.high = Some(high);
        }
        let node = self.engine.nodes.get_mut(entity).unwrap();
        if node.numeric != Some(*range) {
            node.numeric = Some(*range);
            node.style.numeric_revision = node.style.numeric_revision.wrapping_add(1);
            node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
        }
        input.map(|_| range.value)
    }
}
impl crate::ImplicitKey for Slider<'_, '_, '_> {
    fn key_of(&self) -> Key {
        self.inner.key
    }
    #[inline(always)]
    fn set_key(mut self, key: Key) -> Self {
        self.inner.key = key;
        self
    }
}
impl Slider<'_, '_, '_> {
    #[inline(always)]
    pub fn step(mut self, step: f64) -> Self {
        self.range.step = step;
        self
    }
    #[inline(always)]
    pub fn vertical(mut self, vertical: bool) -> Self {
        self.range.vertical = vertical;
        if vertical {
            self.inner.style.width = Some(32.);
            self.inner.style.height = Some(180.);
        }
        self
    }
    #[inline(always)]
    pub fn reversed(mut self, reversed: bool) -> Self {
        self.range.reversed = reversed;
        self
    }
    #[inline(always)]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.style.disabled |= disabled;
        self
    }
    #[inline(always)]
    fn finish(&mut self) -> crate::SliderResponse {
        let Some(ui) = self.inner.ui.take() else {
            return crate::SliderResponse::default();
        };
        let before = *self.value;
        if let Some(high) = self.high.as_deref_mut()
            && *high < *self.value
        {
            *self.value = *high;
        }
        if self.range.valid() {
            *self.value = self.range.normalize(*self.value);
            self.range.value = *self.value;
            if let Some(high) = self.range.high {
                self.range.high = Some(self.range.normalize(high).max(*self.value));
            }
        }
        let normalized = *self.value;
        let normalized_high = self.range.high;
        let input = ui.slider_element(
            self.inner.key,
            self.inner.style,
            &self.inner.text,
            &mut self.range,
        );
        if let Some(value) = input
            && self.range.valid()
        {
            *self.value = self.range.normalize(value);
        }
        if let Some(high) = self.high.as_deref_mut()
            && let Some(current) = self.range.high
        {
            *high = current;
        }
        let changed = (input.is_some() && normalized != *self.value)
            || (normalized_high.is_some() && normalized_high != self.range.high);
        if before != *self.value {
            ui.request_rebuild();
        }
        crate::SliderResponse {
            changed,
            value: *self.value,
            high: self.range.high,
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
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

impl<F, R> std::ops::Add<F> for ButtonBase<'_, '_, '_>
where
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Output = InnerResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}
