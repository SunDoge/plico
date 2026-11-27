//! Immediate modal declarations with portable presence state and host-owned motion.
use crate::{Align, Color, Justify, Key, Kind, Motion, Style, UiContext};
use std::borrow::Cow;

static POPUP_NECK: crate::ShaderAsset =
    crate::shader_asset!("../assets/materials/popup_neck.wgsl").read_backdrop();

#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModalState {
    open: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    preview: bool,
    mounted: bool,
    closing_until_ms: Option<u64>,
}
impl ModalState {
    pub fn open(&mut self) {
        self.open = true;
        self.preview = false;
    }
    /// Show an anchored popup for a visual demonstration without an input scope.
    pub fn open_preview(&mut self) {
        self.open = true;
        self.preview = true;
    }
    /// Whether the anchored popup is in its visual-only presentation.
    pub fn is_preview(&self) -> bool {
        self.preview
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
}
pub struct ModalResponse<R> {
    pub inner: Option<R>,
    dismissed: bool,
}
impl<R> ModalResponse<R> {
    pub const fn dismissed(&self) -> bool {
        self.dismissed
    }
}
pub struct Modal<'u, 'e, 's> {
    ui: &'u mut UiContext<'e>,
    state: &'s mut ModalState,
    key: Key,
    title: Cow<'s, str>,
    width: f32,
    motion: Motion,
    backdrop: bool,
    escape: bool,
    anchor: Option<u64>,
    panel_role: Option<crate::WidgetRole>,
    over_anchor: bool,
    max_height: Option<f32>,
    header: bool,
    material: Option<crate::LiquidGlass>,
    scrim_opacity: Option<f32>,
}
impl<'e> UiContext<'e> {
    pub(crate) fn modal<'u, 's>(&'u mut self, state: &'s mut ModalState) -> Modal<'u, 'e, 's> {
        let key = self.engine.auto_key(self.parent);
        Modal {
            ui: self,
            state,
            key,
            title: Cow::Borrowed("Dialog"),
            width: 440.,
            motion: Motion::Quick,
            backdrop: true,
            escape: true,
            anchor: None,
            panel_role: None,
            over_anchor: false,
            max_height: None,
            header: true,
            material: None,
            scrim_opacity: None,
        }
    }
    /// Close the innermost popover containing this declaration.
    pub fn close_popover(&mut self) {
        let mut entity = self.parent;
        while entity != self.engine.root {
            let node = self.engine.nodes.get(entity).unwrap();
            if node.kind == Kind::Overlay {
                if node.style.popup_anchor.is_some() && node.style.modal_open {
                    self.engine.queue_click(entity);
                }
                return;
            }
            entity = node.identity.parent;
        }
    }
    /// Close the innermost modal containing this declaration.
    pub fn close_modal(&mut self) {
        let mut entity = self.parent;
        while entity != self.engine.root {
            let node = self.engine.nodes.get(entity).unwrap();
            if node.kind == Kind::Overlay {
                if node.style.modal_open {
                    self.engine.queue_click(entity);
                }
                return;
            }
            entity = node.identity.parent;
        }
    }
}
impl<'s> crate::ImplicitKey for Modal<'_, '_, 's> {
    fn key_of(&self) -> Key {
        self.key
    }
    fn set_key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}
impl<'s> Modal<'_, '_, 's> {
    pub fn title(mut self, title: impl Into<Cow<'s, str>>) -> Self {
        self.title = title.into();
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }
    pub fn dismiss_on_backdrop(mut self, enabled: bool) -> Self {
        self.backdrop = enabled;
        self
    }
    pub fn dismiss_on_escape(mut self, enabled: bool) -> Self {
        self.escape = enabled;
        self
    }
    /// Show the title row and its close button. Alerts turn it off and put the
    /// title and message in their own content.
    pub fn header(mut self, enabled: bool) -> Self {
        self.header = enabled;
        self
    }
    /// The semantic role of the dialog panel, such as an alert dialog.
    pub(crate) fn panel_role(mut self, role: crate::WidgetRole) -> Self {
        self.panel_role = Some(role);
        self
    }
    /// Scene-backed glass with the dialog's own fixed background, rounded rim
    /// and overlay shadows. Scrolling affects content, not the glass surface.
    pub fn material(mut self, material: crate::LiquidGlass) -> Self {
        self.material = Some(material);
        self
    }
    /// Override the dimming behind a modal. Glass defaults to 18%, opaque to 38%.
    pub fn scrim_opacity(mut self, opacity: f32) -> Self {
        self.scrim_opacity = opacity.is_finite().then(|| opacity.clamp(0., 1.));
        self
    }
    pub fn children<R>(self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> ModalResponse<R> {
        let Self {
            ui,
            state,
            key,
            title,
            width,
            motion,
            backdrop,
            escape,
            anchor,
            panel_role,
            over_anchor,
            max_height,
            header,
            material,
            scrim_opacity,
        } = self;
        let preferences = ui.display_preferences();
        let material = material.map(|material| material.follow_preferences(preferences));
        let motion = if preferences.reduce_motion
            || material.is_some_and(|material| material.reduce_motion)
        {
            Motion::None
        } else {
            motion
        };
        let animate_list = anchor.is_some()
            && (over_anchor
                || matches!(
                    panel_role,
                    Some(crate::WidgetRole::Menu | crate::WidgetRole::ListBox)
                ))
            && material.is_some_and(|glass| glass.enabled)
            && motion != Motion::None;
        let animate_morph = animate_list && over_anchor;
        let now = ui.now().as_millis().min(u64::MAX as u128) as u64;
        if ui.inherited.disabled {
            state.close();
            state.mounted = false;
            state.closing_until_ms = None;
        }
        if state.open {
            state.mounted = true;
            state.closing_until_ms = None;
        } else if state.mounted {
            let until = *state.closing_until_ms.get_or_insert_with(|| {
                now.saturating_add(if animate_list {
                    if over_anchor { 700 } else { 348 }
                } else {
                    u64::from(motion.duration_ms())
                })
            });
            if now >= until {
                state.mounted = false;
                state.closing_until_ms = None;
            }
        }
        let glass_popup = anchor.is_some() && material.is_some_and(|glass| glass.enabled);
        let liquid_list = material.is_some()
            && (over_anchor
                || matches!(
                    panel_role,
                    Some(crate::WidgetRole::Menu | crate::WidgetRole::ListBox)
                ));
        let over_center = anchor.is_some() && over_anchor;
        let opening = if glass_popup && !animate_list && motion != Motion::None {
            ui.keyed(key, |cx| {
                cx.animate(
                    "glass-popup-opening",
                    f32::from(state.open),
                    std::time::Duration::from_millis(u64::from(motion.duration_ms())),
                )
            })
        } else {
            1.
        };
        if !state.mounted {
            state.preview = false;
            return ModalResponse {
                inner: None,
                dismissed: false,
            };
        }
        // Hash logical ancestry, never arena handles: identity survives replay/reload.
        let mut hash = crate::macro_support::StableHasher::new();
        hash.write(b"plico.modal.portal");
        hash.write_u64(ui.scope);
        let mut ancestor = ui.parent;
        while ancestor != ui.engine.root {
            let identity = ui.engine.nodes.get(ancestor).unwrap().identity;
            hash.write_u64(crate::macro_support::key_seed(identity.key));
            hash.write_u64(identity.scope);
            ancestor = identity.parent;
        }
        let scope = hash.finish();
        let style = Style {
            overlay: true,
            popup_anchor: anchor,
            popup_preview: anchor.is_some() && state.preview,
            semantic_hidden: anchor.is_some() && state.preview,
            liquid_popup: liquid_list,
            popup_over_anchor: over_center,
            popup_motion: animate_list,
            modal_open: state.open,
            dismiss_on_backdrop: backdrop,
            dismiss_on_escape: escape,
            presence_until_ms: state.closing_until_ms,
            opacity: if state.open || animate_list { 1. } else { 0. },
            transition_ms: motion.duration_ms(),
            font_size: ui.theme().metrics.font_size,
            padding: if anchor.is_some() { 0. } else { 20. },
            align: Some(Align::Center),
            justify: Some(Justify::Center),
            ..Default::default()
        };
        let root = ui.engine.root;
        let mut portal = UiContext {
            engine: ui.engine,
            localizer: ui.localizer,
            parent: root,
            scope,
            inherited: Style::default(),
            in_button: false,
            toolbar_buttons: false,
            accordion_items: false,
            accordion_index: 0,
        };
        let entity = portal.engine.declare(
            portal.parent,
            key,
            scope,
            Kind::Overlay,
            style,
            Cow::Borrowed(""),
        );
        let mut dismissed = portal.engine.clicked(entity);
        let inner = portal.with_parent(entity, style, false, |cx| {
            if anchor.is_none() {
                cx.element(
                    Key::Static("scrim"),
                    Kind::Column,
                    Style {
                        overlay: true,
                        background: Some(Color::Literal(0)),
                        opacity: scrim_opacity.unwrap_or(
                            if material.is_some_and(|glass| glass.enabled) {
                                0.18
                            } else {
                                0.4
                            },
                        ),
                        ..Default::default()
                    },
                    "",
                    |_| {},
                );
            }
            if glass_popup && !over_center && !animate_list && opening > 0. && opening < 1. {
                crate::widgets::shader(cx, &POPUP_NECK)
                    .absolute(0., 0.)
                    .width(1.)
                    .height(1.)
                    .values(&[opening, -1.]);
            }
            let metrics = cx.theme().metrics;
            let mut panel = Style {
                width_percent: anchor.is_none().then_some(100.),
                width: anchor.map(|_| width),
                max_width: Some(width),
                shrink: 1.,
                padding: if anchor.is_some() { 4. } else { 24. },
                gap: if anchor.is_some() { 4. } else { 16. },
                radius: if anchor.is_some() {
                    metrics.panel_radius() as f32
                } else {
                    metrics.surface_radius() as f32
                },
                enter_from: anchor.is_none().then(|| crate::Presentation {
                    translate: [0., 8.],
                    ..Default::default()
                }),
                translate: [
                    0.,
                    if state.open || anchor.is_some() {
                        0.
                    } else {
                        8.
                    },
                ],
                transition_ms: motion.duration_ms(),
                background: Some(Color::Panel),
                border_width: if anchor.is_some() { 1. } else { 0. },
                border_color: Color::Border,
                scroll_y: true,
                max_height,
                role: panel_role,
                disabled: false,
                font_size: cx.theme().metrics.font_size,
                ..Default::default()
            };
            if liquid_list {
                crate::Elevation::LIQUID_POPUP.apply(&mut panel);
            } else if anchor.is_some() {
                crate::Elevation::FLOATING.apply(&mut panel);
            } else {
                crate::Elevation::OVERLAY.apply(&mut panel);
            }
            if animate_morph {
                panel.shadow = None;
                panel.ambient_shadow = None;
            }
            if material.is_some() {
                if glass_popup && !animate_list {
                    // Scale the content uniformly so text and shadows keep their proportions.
                    panel.scale = [0.88 + 0.12 * opening; 2];
                }
                panel.padding = 0.;
                panel.gap = 0.;
                panel.radius = if over_anchor && panel_role.is_none() {
                    26.
                } else {
                    24.
                };
                panel.corner_smoothing = if liquid_list { 0. } else { 1. };
                panel.background = None;
                panel.border_width = 0.;
                panel.scroll_y = false;
                panel.layout.position = crate::Position::Relative;
            }
            let content = |cx: &mut UiContext<'_>| {
                if anchor.is_none() && header {
                    let _ = crate::widgets::row(cx)
                        .gap(12)
                        .align(Align::Center)
                        .children(|cx| {
                            crate::widgets::text(cx, title.as_ref())
                                .font_size(20)
                                .weight(650)
                                .grow(1.);
                            let close = cx.tr("dialog-close", "Close dialog");
                            let clicked = crate::widgets::tooltip(cx, close.clone())
                                .children(|cx| {
                                    crate::widgets::button(cx)
                                        .icon(&crate::icons::X)
                                        .aria_label(close)
                                        .ghost()
                                        .color(Color::Muted)
                                        .width(32.)
                                        .min_width(32.)
                                        .max_width(32.)
                                        .height(32.)
                                        .min_height(32.)
                                        .max_height(32.)
                                        .padding(7)
                                        .radius(6.)
                                        .clicked()
                                })
                                .inner;
                            if clicked {
                                cx.close_modal();
                            }
                        });
                }
                build(cx)
            };
            cx.element(Key::Static("dialog"), Kind::Dialog, panel, &title, |cx| {
                if !state.open && !animate_list {
                    cx.engine.retain_exiting_children(cx.parent);
                    return None;
                }
                Some(if let Some(material) = material {
                    if animate_morph {
                        material.show_face(
                            cx,
                            panel.radius,
                            crate::materials::GlassFace::MorphPanel,
                        );
                    } else if animate_list {
                        material.show_face(
                            cx,
                            panel.radius,
                            crate::materials::GlassFace::SelectPanel,
                        );
                    } else if liquid_list {
                        material.show_face(cx, panel.radius, crate::materials::GlassFace::Panel);
                    } else {
                        material.show(cx, panel.radius, panel.corner_smoothing);
                    }
                    let content_style = Style {
                        width_percent: Some(100.),
                        padding: if over_center && panel_role.is_none() {
                            16.
                        } else if liquid_list {
                            6.
                        } else if anchor.is_some() {
                            4.
                        } else {
                            20.
                        },
                        gap: if over_center && panel_role.is_none() {
                            8.
                        } else if liquid_list {
                            0.
                        } else if anchor.is_some() {
                            4.
                        } else {
                            12.
                        },
                        scroll_y: true,
                        max_height,
                        radius: if liquid_list { 18. } else { panel.radius },
                        corner_smoothing: panel.corner_smoothing,
                        font_size: cx.theme().metrics.font_size,
                        ..Default::default()
                    };
                    cx.element(
                        Key::Static("dialog-content"),
                        Kind::Column,
                        content_style,
                        "",
                        content,
                    )
                    .0
                } else {
                    content(cx)
                })
            })
            .0
        });
        dismissed |= portal.engine.clicked(entity);
        if dismissed && state.open {
            state.close();
        }
        ModalResponse { inner, dismissed }
    }
}

/// Portable state for a dismissible anchored panel.
pub type PopoverState = ModalState;

/// A trigger and its anchored portal, declared together without retained callbacks.
pub struct Popover<'u, 'e, 's> {
    modal: Modal<'u, 'e, 's>,
    disabled: bool,
    ghost_trigger: bool,
    trigger_text: Option<Cow<'s, str>>,
    trigger_icon: Option<(crate::SvgAsset, bool)>,
}
impl<'e> UiContext<'e> {
    pub(crate) fn popover<'u, 's>(
        &'u mut self,
        state: &'s mut PopoverState,
        label: impl Into<Cow<'s, str>>,
    ) -> Popover<'u, 'e, 's> {
        Popover {
            modal: self.modal(state).title(label).width(240.),
            disabled: false,
            ghost_trigger: false,
            trigger_text: None,
            trigger_icon: None,
        }
    }
}
impl<'s> crate::ImplicitKey for Popover<'_, '_, 's> {
    fn key_of(&self) -> Key {
        self.modal.key_of()
    }
    fn set_key(self, key: Key) -> Self {
        Self {
            modal: self.modal.set_key(key),
            ..self
        }
    }
}
impl<'s> Popover<'_, '_, 's> {
    pub fn trigger_icon(mut self, icon: &crate::SvgAsset, icon_only: bool) -> Self {
        self.trigger_icon = Some((*icon, icon_only));
        self
    }
    /// Use a toolbar-style trigger with a fill only on hover or press.
    /// An attached material still paints the panel and its opening animation.
    pub fn ghost_trigger(mut self) -> Self {
        self.ghost_trigger = true;
        self
    }
    /// Center the popup over its trigger, clamping the result to the viewport.
    pub fn over_anchor(mut self, enabled: bool) -> Self {
        self.modal.over_anchor = enabled;
        self
    }
    /// Render the anchor and its popup with the same scene-backed glass.
    pub fn material(mut self, glass: crate::LiquidGlass) -> Self {
        self.modal.material = Some(glass);
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.modal = self.modal.width(width);
        self
    }
    pub fn max_height(mut self, height: f32) -> Self {
        self.modal.max_height = Some(height);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn motion(mut self, motion: Motion) -> Self {
        self.modal = self.modal.motion(motion);
        self
    }
    pub(crate) fn display_text(mut self, text: impl Into<Cow<'s, str>>) -> Self {
        self.trigger_text = Some(text.into());
        self
    }
    pub(crate) fn role(mut self, role: crate::WidgetRole) -> Self {
        self.modal.panel_role = Some(role);
        self
    }
    pub fn children<R>(mut self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> ModalResponse<R> {
        let mut hash = crate::macro_support::StableHasher::new();
        hash.write(b"plico.popover.anchor");
        hash.write_u64(crate::macro_support::key_seed(self.modal.key));
        hash.write_u64(self.modal.ui.scope);
        let mut ancestor = self.modal.ui.parent;
        while ancestor != self.modal.ui.engine.root {
            let identity = self.modal.ui.engine.nodes.get(ancestor).unwrap().identity;
            hash.write_u64(crate::macro_support::key_seed(identity.key));
            hash.write_u64(identity.scope);
            ancestor = identity.parent;
        }
        let anchor = hash.finish();
        let ui = &mut self.modal.ui;
        let disabled = self.disabled || ui.inherited.disabled;
        if disabled {
            self.modal.state.close();
        }
        let metrics = ui.theme().metrics;
        let trigger_text = self.trigger_text.take();
        let glass = self.modal.material;
        let trigger_icon = self.trigger_icon;
        let icon_only = trigger_icon.is_some_and(|(_, only)| only);
        let custom_trigger = glass.is_some() || trigger_icon.is_some() || self.ghost_trigger;
        let tinted_icon_trigger = !self.ghost_trigger
            && icon_only
            && glass.is_some_and(|material| material.tint_opacity > 0.);
        let mut style = Style {
            anchor_id: Some(anchor),
            width: trigger_text.as_ref().map(|_| self.modal.width),
            gap: 8.,
            role: (self.modal.panel_role == Some(crate::WidgetRole::ListBox))
                .then_some(crate::WidgetRole::ComboBox),
            expanded: Some(self.modal.state.is_open() && !self.modal.state.is_preview()),
            disabled,
            styled_button: trigger_text.is_some() || self.ghost_trigger,
            font_size: metrics.font_size,
            transition_ms: metrics.interaction_ms,
            padding_x: Some(metrics.spacing as f32 * 3.5),
            padding_y: Some(metrics.spacing as f32 * 1.5),
            border_width: 1.,
            radius: metrics.radius as f32,
            min_height: Some(metrics.control_height as f32),
            background: Some(Color::Control),
            ..Default::default()
        };
        let liquid_list = glass.is_some()
            && (self.modal.over_anchor
                || matches!(
                    self.modal.panel_role,
                    Some(crate::WidgetRole::Menu | crate::WidgetRole::ListBox)
                ));
        let covered = self.modal.over_anchor
            && liquid_list
            && (self.modal.state.open || self.modal.state.mounted);
        let preferences = ui.display_preferences();
        let animate_morph = self.modal.over_anchor
            && glass.is_some_and(|glass| glass.enabled && !glass.reduce_motion)
            && !preferences.reduce_transparency
            && !preferences.reduce_motion
            && self.modal.motion != Motion::None;
        let label_opacity = f32::from(animate_morph || !covered);
        let trigger_glass = animate_morph || !covered;
        if liquid_list {
            style.gap = 8.;
            style.height = Some(48.);
            style.radius = 24.;
            style.corner_smoothing = 0.;
            style.font_size = 15;
            style.align = Some(Align::Center);
            style.justify = Some(Justify::Center);
            style.padding_x = Some(if trigger_text.is_some() { 20. } else { 24. });
            style.padding_y = Some(0.);
        }
        if icon_only {
            style.width = Some(40.);
            style.height = Some(40.);
            style.radius = 20.;
            style.padding_x = Some(0.);
        }
        if glass.is_some() {
            style.background = None;
            style.border_width = 0.;
            style.layout.position = crate::Position::Relative;
        }
        if custom_trigger {
            style.align = Some(Align::Center);
            if icon_only {
                style.justify = Some(Justify::Center);
                style.padding_y = Some(0.);
            }
        }
        if self.ghost_trigger {
            let height = f32::from(metrics.control_height);
            style.height = Some(height);
            style.padding_y = Some(0.);
            style.border_width = 0.;
            style.background = None;
            style.hover_background = Some(Color::ControlHover);
            style.pressed_background = Some(Color::ControlPressed);
            style.radius = if icon_only {
                height / 2.
            } else {
                f32::from(metrics.radius)
            };
            style.width = if icon_only { Some(height) } else { style.width };
            if !icon_only {
                style.padding_x = Some(f32::from(metrics.spacing) * 2.5);
                style.justify = Some(Justify::Start);
            }
        }
        let (_, clicked) = ui.element(
            self.modal.key,
            if trigger_text.is_some() || custom_trigger {
                Kind::ButtonBase
            } else {
                Kind::Button
            },
            style,
            self.modal.title.as_ref(),
            |cx| {
                if let Some(glass) = glass.filter(|_| !self.ghost_trigger) {
                    if trigger_glass && liquid_list {
                        let face = if tinted_icon_trigger {
                            crate::materials::GlassFace::Group(false)
                        } else {
                            crate::materials::GlassFace::Trigger
                        };
                        glass.show_face(cx, style.radius, face);
                    } else if !covered {
                        glass.show_interactive(
                            cx,
                            style.radius,
                            style.corner_smoothing,
                            cx.hovered(),
                            cx.pressed(),
                        );
                    }
                }
                if custom_trigger && trigger_text.is_none() {
                    if let Some((icon, _)) = trigger_icon {
                        crate::widgets::svg(cx, &icon)
                            .size(16., 16.)
                            .color(if tinted_icon_trigger {
                                Color::Literal(0x171717)
                            } else {
                                Color::Foreground
                            })
                            .opacity(label_opacity);
                    }
                    if !icon_only {
                        crate::widgets::text(cx, self.modal.title.as_ref())
                            .opacity(label_opacity)
                            .weight(if self.modal.panel_role == Some(crate::WidgetRole::Menu) {
                                600
                            } else if liquid_list {
                                500
                            } else {
                                400
                            });
                    }
                    if liquid_list && trigger_icon.is_none() {
                        crate::widgets::svg(cx, &crate::icons::CHEVRON_DOWN)
                            .size(16., 16.)
                            .opacity(label_opacity * 0.6);
                    }
                }
                if let Some(shown) = trigger_text.as_deref() {
                    crate::widgets::text(cx, shown).grow(1.);
                    crate::widgets::svg(cx, &crate::icons::CHEVRON_DOWN).size(16., 16.);
                }
            },
        );
        if clicked && !disabled {
            if self.modal.state.is_open() && !self.modal.state.is_preview() {
                self.modal.state.close();
            } else {
                self.modal.state.open();
            }
        }
        self.modal.anchor = Some(anchor);
        self.modal.children(build)
    }
}

/// Native addition spelling for the consuming children operation.
impl<F, R> std::ops::Add<F> for Modal<'_, '_, '_>
where
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Output = ModalResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}

impl<F, R> std::ops::Add<F> for Popover<'_, '_, '_>
where
    F: FnOnce(&mut UiContext<'_>) -> R,
{
    type Output = ModalResponse<R>;
    fn add(self, build: F) -> Self::Output {
        self.children(build)
    }
}
