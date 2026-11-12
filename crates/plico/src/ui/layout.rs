use super::*;

pub struct Layout<'u, 'e> {
    pub(super) ui: &'u mut UiContext<'e>,
    pub(super) key: Key,
    pub(super) kind: Kind,
    pub(super) style: Style,
    pub(super) grid: Option<crate::GridTemplate>,
    pub(super) semantic_label: Option<String>,
    pub(super) numeric: Option<crate::NumericRange>,
    pub(super) toolbar_buttons: bool,
    pub(super) accordion_items: bool,
    pub(super) material: Option<crate::LiquidGlass>,
}
impl Layout<'_, '_> {
    /// Attach a read-only numeric range and its accessible name to a composed
    /// container. Meter/progress visuals remain ordinary children while the
    /// semantic value stays on their shared root.
    pub(crate) fn semantic_range(
        mut self,
        role: crate::WidgetRole,
        label: &str,
        min: f64,
        max: f64,
        value: f64,
    ) -> Self {
        self.style.role = Some(role);
        self.semantic_label = Some(label.to_owned());
        let mut numeric = crate::NumericRange {
            min,
            max,
            value,
            high: None,
            step: 0.,
            vertical: false,
            reversed: false,
        };
        if numeric.valid() {
            numeric.value = numeric.normalize(value);
            self.numeric = Some(numeric);
        }
        self
    }

    pub(crate) fn focus_group(mut self, axis: crate::FocusAxis) -> Self {
        self.style.focus_axis = Some(axis);
        self
    }
    pub(crate) fn selection_follows_focus(mut self, value: bool) -> Self {
        self.style.selection_follows_focus = value;
        self
    }
    pub(crate) fn accordion_items(mut self) -> Self {
        self.accordion_items = true;
        self
    }
    pub(crate) fn toolbar_buttons(mut self) -> Self {
        self.toolbar_buttons = true;
        self
    }

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
    /// Add an inner shadow clipped to the container contour.
    pub fn inset_shadow(mut self, shadow: crate::BoxShadow) -> Self {
        self.style.inset_shadow = Some(shadow);
        self
    }
    /// Clear contact, ambient and inset shadows.
    #[inline(always)]
    pub fn no_shadow(mut self) -> Self {
        self.style.shadow = None;
        self.style.ambient_shadow = None;
        self.style.inset_shadow = None;
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
    /// Push this sticky node out when the next sticky sibling reaches it.
    /// Intended for section headers; ordinary sticky positioning does not
    /// enable sibling interaction.
    #[inline(always)]
    pub fn sticky_push(mut self, enabled: bool) -> Self {
        self.style.sticky_push = enabled;
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
    /// Mount at this presentation, then animate to the authored target using
    /// the host's motion clock. Descendants share the group's transform.
    pub fn enter_from(mut self, presentation: crate::Presentation) -> Self {
        self.style.enter_from = Some(presentation);
        self
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
    /// Hide an implementation subtree from accessibility while retaining its
    /// visual tree. Framework page transitions and composed controls use it.
    #[inline(always)]
    pub(crate) fn semantic_hidden(mut self, value: bool) -> Self {
        self.style.semantic_hidden = value;
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
        let label = self.semantic_label.unwrap_or_default();
        let entity = self.ui.engine.declare(
            self.ui.parent,
            self.key,
            self.ui.scope,
            self.kind,
            self.style,
            Cow::Owned(label),
        );
        let node = self.ui.engine.nodes.get_mut(entity).unwrap();
        if node.numeric != self.numeric {
            node.numeric = self.numeric;
            node.style.numeric_revision = node.style.numeric_revision.wrapping_add(1);
            node.pending_changes |= Changes::SEMANTICS;
        }
        self.ui.engine.set_grid(entity, self.grid.as_ref());
        let inner = self.ui.with_parent(entity, self.style, false, |cx| {
            if let Some(material) = self.material {
                material.show(cx, self.style.radius, self.style.corner_smoothing);
            }
            let previous_accordion = cx.accordion_items;
            let previous_accordion_index = cx.accordion_index;
            if self.accordion_items {
                cx.accordion_items = true;
                cx.accordion_index = 0;
            }
            let previous = cx.toolbar_buttons;
            cx.toolbar_buttons |= self.toolbar_buttons;
            let result = build(cx);
            cx.toolbar_buttons = previous;
            cx.accordion_items = previous_accordion;
            cx.accordion_index = previous_accordion_index;
            result
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
