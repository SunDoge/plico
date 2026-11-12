use super::*;

pub(super) struct Leaf<'u, 'e, 't> {
    pub(super) ui: Option<&'u mut UiContext<'e>>,
    pub(super) key: Key,
    pub(super) kind: Kind,
    pub(super) style: Style,
    pub(super) text: Cow<'t, str>,
    pub(super) hovered: bool,
    pub(super) recycle_text: bool,
    pub(super) spans: Vec<crate::TextSpan>,
    pub(super) material: Option<crate::LiquidGlass>,
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
            style.gap = ui.theme().metrics.spacing as f32 * 1.5;
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
    pub(super) inner: Leaf<'u, 'e, 't>,
}
impl Text<'_, '_, '_> {
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
    #[inline(always)]
    pub(crate) fn semantic_role(mut self, role: crate::WidgetRole) -> Self {
        self.inner.style.role = Some(role);
        self
    }
}
pub struct Button<'u, 'e, 't> {
    pub(super) inner: Leaf<'u, 'e, 't>,
    pub(super) icon: Option<crate::SvgAsset>,
    pub(super) accessible_name: Option<Cow<'t, str>>,
    pub(super) composable: bool,
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
            pub fn clip(mut self, value: bool) -> Self {
                self.inner.style.clip = value;
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
            /// Add an inner shadow clipped to the control contour.
            pub fn inset_shadow(mut self, shadow: crate::BoxShadow) -> Self {
                self.inner.style.inset_shadow = Some(shadow);
                self
            }
            /// Clear contact, ambient and inset shadows.
            #[inline(always)]
            pub fn no_shadow(mut self) -> Self {
                self.inner.style.shadow = None;
                self.inner.style.ambient_shadow = None;
                self.inner.style.inset_shadow = None;
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
            pub fn enter_from(mut self, presentation: crate::Presentation) -> Self {
                self.inner.style.enter_from = Some(presentation);
                self
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
    #[inline(always)]
    pub fn primary(mut self) -> Self {
        self.inner.style.background = Some(Color::Accent);
        self.inner.style.hover_background = Some(Color::AccentHover);
        self.inner.style.pressed_background = Some(Color::AccentPressed);
        self.inner.style.border_width = 0.;
        self.inner.style.foreground = Color::OnAccent;
        self
    }
    /// MyGO toolbar appearance: transparent at rest, filled on hover or press.
    #[inline(always)]
    pub fn ghost(mut self) -> Self {
        self.inner.style.background = None;
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
    pub(super) inner: Leaf<'u, 'e, 't>,
    pub(super) value: &'t mut bool,
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
    pub(super) inner: Leaf<'u, 'e, 'static>,
}

/// A decoded raster image. The asset's encoded bytes are carried hex-encoded
/// in the node's text; the backend decodes and caches by content.
pub struct Image<'u, 'e> {
    pub(super) inner: Leaf<'u, 'e, 'u>,
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
impl<R> InnerResponse<R> {
    /// Whether the composed control was activated. This mirrors MyGO's
    /// `Element.Clicked` without exposing the nested response for common use.
    pub const fn clicked(&self) -> bool {
        self.response.clicked()
    }
    pub const fn hovered(&self) -> bool {
        self.response.hovered()
    }
    pub const fn enabled(&self) -> bool {
        self.response.enabled()
    }
    pub const fn focused(&self) -> bool {
        self.response.focused()
    }
}

pub(super) fn style_input(style: &mut Style, theme: Theme) {
    let m = theme.metrics;
    style.min_height = Some(m.control_height as f32);
    style.padding = 0.;
    style.padding_x = Some(m.spacing as f32 * 2.5);
    style.padding_y = Some(m.spacing as f32 * 1.5);
    style.radius = m.radius as f32;
    style.border_width = 1.;
    style.background = Some(Color::Control);
    style.transition_ms = m.interaction_ms;
}

/// Visual contexts for the same composed button, following MyGO styleButton.
/// Kept outside widgets so shortcut and custom-content buttons share it.
pub(super) enum ButtonAppearance {
    Ordinary,
    Toolbar,
    Segment(bool),
}
pub(super) fn style_button(style: &mut Style, theme: Theme, appearance: ButtonAppearance) {
    let m = theme.metrics;
    style.transition_ms = m.interaction_ms;
    style.padding = 0.;
    style.padding_x = Some(m.spacing as f32 * 3.5);
    style.padding_y = Some(m.spacing as f32 * 1.5);
    style.gap = m.spacing as f32 * 1.5;
    style.border_width = 1.;
    style.radius = m.radius as f32;
    style.min_height = Some(m.control_height as f32);
    style.background = Some(Color::Control);
    style.hover_background = Some(Color::ControlHover);
    style.pressed_background = Some(Color::ControlPressed);
    match appearance {
        ButtonAppearance::Ordinary => {
            // Infer native faces from the untouched base color. An explicit
            // application background then keeps its own look, as MyGO does.
            style.hover_background = None;
            style.pressed_background = None;
        }
        ButtonAppearance::Toolbar => {
            style.padding_x = Some(m.spacing as f32 * 2.5);
            style.background = None;
            style.border_width = 0.;
        }
        ButtonAppearance::Segment(on) => {
            style.padding_x = Some(m.space(3) as f32);
            style.padding_y = Some(m.space(1) as f32);
            style.radius = (m.radius as f32 - m.spacing as f32 * 0.5).max(0.);
            style.min_height = Some(0.);
            style.border_width = 0.;
            style.background = on.then_some(if theme.surface & 0xff < 128 {
                Color::ControlPressed
            } else {
                Color::Panel
            });
            if on {
                // A chosen segment keeps its raised face while hovered.
                style.hover_background = style.background;
                style.pressed_background = style.background;
                style.shadow = Some(BoxShadow {
                    offset_x: 0.,
                    offset_y: 1.,
                    blur: 2.,
                    spread: 0.,
                    color: Color::Literal(0),
                    opacity: 0.12,
                });
            }
        }
    }
}

pub struct ButtonBase<'u, 'e, 't> {
    pub(super) inner: Leaf<'u, 'e, 't>,
    pub(super) styled: bool,
}
impl ButtonBase<'_, '_, '_> {
    /// A segment is an ordinary button with a different face, not another
    /// interaction implementation. All focus/disabled/motion stays shared.
    pub(crate) fn segment(mut self, selected: bool) -> Self {
        let theme = self.inner.ui.as_ref().expect("button context").theme();
        style_button(
            &mut self.inner.style,
            theme,
            ButtonAppearance::Segment(selected),
        );
        self.inner.style.selected = Some(selected);
        self.styled = true;
        self
    }

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

    #[inline(always)]
    pub(crate) fn tab_stop(mut self, value: bool) -> Self {
        self.inner.style.tab_stop = value;
        self
    }

    #[inline(always)]
    pub(crate) fn semantic_hidden(mut self, value: bool) -> Self {
        self.inner.style.semantic_hidden = value;
        self
    }

    #[inline(always)]
    pub(crate) fn tree_depth(mut self, depth: usize) -> Self {
        self.inner.style.tree_depth = Some(depth.min(u16::MAX as usize) as u16);
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

pub(super) struct StringBinding {
    pub(super) buffer: crate::TextBuffer,
    pub(super) synced: String,
    pub(super) external_sync: bool,
}
impl StringBinding {
    pub(super) fn new(value: &str) -> Self {
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
    pub(super) inner: Leaf<'u, 'e, 't>,
    pub(super) buffer: crate::TextBuffer,
    pub(super) string: Option<(crate::LocalState<StringBinding>, &'t mut String)>,
    pub(super) placeholder: Cow<'t, str>,
    pub(super) autofocus: bool,
    pub(super) select_all_on_autofocus: bool,
    pub(super) focus: bool,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct InputResponse {
    changed: bool,
    submitted: bool,
    navigation: Option<crate::InputNavigation>,
    /// Whether the backend focused this field when it was declared.
    focused: bool,
}
impl InputResponse {
    pub const fn changed(self) -> bool {
        self.changed
    }
    pub const fn submitted(self) -> bool {
        self.submitted
    }
    pub const fn navigation(self) -> Option<crate::InputNavigation> {
        self.navigation
    }
    pub const fn focused(self) -> bool {
        self.focused
    }
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
        self.inner.style.padding = 0.;
        self.inner.style.padding_x = None;
        self.inner.style.padding_y = None;
        // Even an empty editor needs one line for caret, IME and semantics.
        // Remove the shared frame's vertical insets from its control minimum.
        let m = self
            .inner
            .ui
            .as_ref()
            .expect("input context")
            .theme()
            .metrics;
        self.inner.style.min_height = Some(
            (m.control_height as f32 - m.spacing as f32 * 3.)
                .max(self.inner.style.font_size as f32),
        );
        self.inner.style.radius = 0.;
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
    /// Select the existing value when autofocus first mounts this field.
    /// Find bars use this so reopening is immediately ready for replacement.
    #[inline(always)]
    pub(crate) fn select_all_on_autofocus(mut self, enabled: bool) -> Self {
        self.select_all_on_autofocus = enabled;
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
    /// Route find traversal/dismissal keys without exposing host events to the
    /// composed search field.
    #[inline(always)]
    pub(crate) fn find_navigation(mut self, enabled: bool) -> Self {
        self.inner.style.input_find_navigation = enabled;
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
                    if self.select_all_on_autofocus {
                        self.buffer.edit(crate::InputEdit::SelectAll);
                        let node = ui.engine.nodes.get_mut(entity).unwrap();
                        node.style.input_revision = self.buffer.revision();
                        node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
                    }
                }
                entry.insert((
                    self.buffer.clone(),
                    self.placeholder.to_string(),
                    self.buffer.counters(),
                ));
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
    /// Consume the input and report whether its bound text changed.
    pub fn changed(self) -> bool {
        self.response().changed()
    }
    /// Consume the input and report whether it was submitted.
    pub fn submitted(self) -> bool {
        self.response().submitted()
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
                prefilter_backdrop: asset.backdrop_prefiltered(),
                builtin: asset.builtin_kind(),
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
    /// Build backdrop prefilter levels up to this logical-pixel sigma.
    /// The asset must declare backdrop reading; layout and hit testing are unchanged.
    pub fn prefilter_backdrop(mut self, max_sigma: f32) -> Self {
        self.params.prefilter_backdrop = Some(max_sigma);
        self
    }
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

impl UiContext<'_> {
    /// Declare a composed scalar editor. The container owns one numeric value,
    /// keyboard focus, arrow/Home/End handling and slider accessibility; its
    /// children provide all visuals and pointer targets.
    pub fn value_control<R>(
        &mut self,
        label: &str,
        value: &mut f64,
        range: std::ops::RangeInclusive<f64>,
        build: impl FnOnce(&mut UiContext<'_>, &mut f64) -> R,
    ) -> crate::ValueControlResponse<R> {
        self.value_control_with(
            label,
            value,
            range,
            1.,
            crate::WidgetRole::Slider,
            crate::FlexDirection::Row,
            |style| style,
            build,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn value_control_with<R>(
        &mut self,
        label: &str,
        value: &mut f64,
        range: std::ops::RangeInclusive<f64>,
        step: f64,
        role: crate::WidgetRole,
        direction: crate::FlexDirection,
        configure: impl FnOnce(Style) -> Style,
        build: impl FnOnce(&mut UiContext<'_>, &mut f64) -> R,
    ) -> crate::ValueControlResponse<R> {
        let key = self.engine.auto_key(self.parent);
        let mut style = self.inherited_style();
        style.control_chrome = false;
        style.role = Some(role);
        style.layout.flex_direction = Some(direction);
        style.align = Some(crate::Align::Center);
        style = configure(style);
        let mut numeric = crate::NumericRange {
            min: *range.start(),
            max: *range.end(),
            value: *value,
            high: None,
            step,
            vertical: false,
            reversed: false,
        };
        if !numeric.valid() {
            self.engine.error.get_or_insert(FrameError::InvalidStyle);
        } else {
            numeric.value = numeric.normalize(numeric.value);
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
            Kind::ValueControl,
            style,
            Cow::Borrowed(label),
        );
        let input = (!style.disabled)
            .then(|| self.engine.value(entity))
            .flatten();
        if let Some(input) = input
            && numeric.valid()
        {
            numeric.value = numeric.normalize(input);
        }
        let node = self.engine.nodes.get_mut(entity).unwrap();
        if node.numeric != Some(numeric) {
            node.numeric = Some(numeric);
            node.style.numeric_revision = node.style.numeric_revision.wrapping_add(1);
            node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
        }
        self.engine.set_grid(entity, None);
        let before = *value;
        *value = numeric.value;
        let focused = self.engine.focused() == Some(entity);
        let inner = self.with_parent(entity, style, false, |cx| build(cx, value));
        if before != *value {
            self.request_rebuild();
        }
        crate::ValueControlResponse::new(inner, input.is_some(), before != *value, *value, focused)
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

    /// Declare a visual range-slider group with two real focus/semantic
    /// children. Numeric edits are queued on the group; the children identify
    /// which bound receives keyboard, pointer, and accessibility actions.
    pub(crate) fn range_slider_element(
        &mut self,
        key: Key,
        mut style: Style,
        label: &str,
        range: &mut crate::NumericRange,
    ) -> Option<f64> {
        style.disabled |= self.inherited.disabled;
        style.role = Some(crate::WidgetRole::Group);
        style.tab_stop = false;
        if !range.valid() || range.high.is_none() {
            self.engine.error.get_or_insert(FrameError::InvalidStyle);
        } else {
            range.value = range.normalize(range.value);
            range.high = range
                .high
                .map(|high| range.normalize(high).max(range.value));
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
            Kind::ValueControl,
            style,
            Cow::Borrowed(label),
        );
        let input = (!style.disabled)
            .then(|| self.engine.value(entity))
            .flatten();
        let high_input = (!style.disabled)
            .then(|| self.engine.high_value(entity))
            .flatten();
        if let Some(value) = input
            && range.valid()
        {
            let high = high_input.or(range.high).unwrap_or(range.max);
            range.value = range.normalize(value).min(high);
        }
        if let Some(high) = high_input
            && range.valid()
        {
            range.high = Some(range.normalize(high).max(range.value));
        }
        let node = self.engine.nodes.get_mut(entity).unwrap();
        if node.numeric != Some(*range) {
            node.numeric = Some(*range);
            node.style.numeric_revision = node.style.numeric_revision.wrapping_add(1);
            node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
        }
        self.engine.set_grid(entity, None);

        let low = range.value;
        let high = range.high.unwrap_or(low);
        let min = range.min;
        let max = range.max;
        let step = range.step;
        self.with_parent(entity, style, false, |cx| {
            for (key, thumb, suffix, value) in [
                (
                    Key::Static("__range_minimum"),
                    crate::SliderThumb::Minimum,
                    "minimum",
                    low,
                ),
                (
                    Key::Static("__range_maximum"),
                    crate::SliderThumb::Maximum,
                    "maximum",
                    high,
                ),
            ] {
                let mut thumb_style = cx.inherited_style();
                thumb_style.role = Some(crate::WidgetRole::Slider);
                thumb_style.control_chrome = false;
                thumb_style.slider_thumb = Some(thumb);
                thumb_style.absolute = Some([0., 0.]);
                thumb_style.width_percent = Some(100.);
                thumb_style.height_percent = Some(100.);
                let thumb_identity = Identity {
                    parent: entity,
                    key,
                    scope: cx.scope,
                };
                if let Some(revision) = cx.engine.lookup_numeric(thumb_identity) {
                    thumb_style.numeric_revision = revision;
                }
                let name = format!("{label} {suffix}");
                let thumb_entity = cx.engine.declare(
                    entity,
                    key,
                    cx.scope,
                    Kind::ValueControl,
                    thumb_style,
                    Cow::Owned(name),
                );
                let numeric = crate::NumericRange {
                    min,
                    max,
                    value,
                    high: None,
                    step,
                    vertical: range.vertical,
                    reversed: range.reversed,
                };
                let thumb_node = cx.engine.nodes.get_mut(thumb_entity).unwrap();
                if thumb_node.numeric != Some(numeric) {
                    thumb_node.numeric = Some(numeric);
                    thumb_node.style.numeric_revision =
                        thumb_node.style.numeric_revision.wrapping_add(1);
                    thumb_node.pending_changes |= Changes::PAINT | Changes::SEMANTICS;
                }
                cx.engine.set_grid(thumb_entity, None);
            }
        });
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
    /// Keep native slider input, keyboard handling and semantics while a
    /// composed parent supplies the track and thumb visuals.
    #[inline(always)]
    pub fn unstyled(mut self) -> Self {
        self.inner.style.control_chrome = false;
        self.inner.style.background = None;
        self.inner.style.border_width = 0.;
        self
    }

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
        let input = if self.range.high.is_some() {
            ui.range_slider_element(
                self.inner.key,
                self.inner.style,
                &self.inner.text,
                &mut self.range,
            )
        } else {
            ui.slider_element(
                self.inner.key,
                self.inner.style,
                &self.inner.text,
                &mut self.range,
            )
        };
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
        crate::SliderResponse::new(changed, *self.value, self.range.high)
    }
    pub fn response(mut self) -> crate::SliderResponse {
        self.finish()
    }
    pub fn changed(self) -> bool {
        self.response().changed()
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
