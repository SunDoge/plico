//! Immediate modal declarations with portable presence state and host-owned motion.
use crate::{Align, Color, Justify, Key, Kind, Motion, Style, UiContext};
use std::borrow::Cow;

#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModalState {
    open: bool,
    mounted: bool,
    closing_until_ms: Option<u64>,
}
impl ModalState {
    pub fn open(&mut self) {
        self.open = true;
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
    pub dismissed: bool,
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
    max_height: Option<f32>,
}
impl<'e> UiContext<'e> {
    pub fn modal<'u, 's>(&'u mut self, state: &'s mut ModalState) -> Modal<'u, 'e, 's> {
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
            max_height: None,
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
            max_height,
        } = self;
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
            let until = *state
                .closing_until_ms
                .get_or_insert_with(|| now.saturating_add(motion.duration_ms() as u64));
            if now >= until {
                state.mounted = false;
                state.closing_until_ms = None;
            }
        }
        if !state.mounted {
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
            modal_open: state.open,
            dismiss_on_backdrop: backdrop,
            dismiss_on_escape: escape,
            presence_until_ms: state.closing_until_ms,
            opacity: if state.open { 1. } else { 0. },
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
            parent: root,
            scope,
            inherited: Style::default(),
            in_button: false,
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
                        opacity: 0.38,
                        ..Default::default()
                    },
                    "",
                    |_| {},
                );
            }
            let panel = Style {
                width_percent: anchor.is_none().then_some(100.),
                width: anchor.map(|_| width),
                max_width: Some(width),
                shrink: 1.,
                padding: if anchor.is_some() { 8. } else { 24. },
                gap: if anchor.is_some() { 8. } else { 18. },
                radius: 12.,
                background: Some(Color::Panel),
                border_width: 1.,
                border_color: Color::Border,
                scroll_y: true,
                max_height,
                role: panel_role,
                disabled: !state.open,
                font_size: cx.theme().metrics.font_size,
                ..Default::default()
            };
            cx.element(Key::Static("dialog"), Kind::Dialog, panel, &title, |cx| {
                if anchor.is_none() {
                    let _ = crate::widgets::row(cx)
                        .gap(12)
                        .align(Align::Center)
                        .children(|cx| {
                            crate::widgets::text(cx, title.as_ref())
                                .font_size(20)
                                .weight(650)
                                .grow(1.);
                            if crate::widgets::button(cx)
                                .text("Close dialog")
                                .ghost()
                                .clicked()
                            {
                                cx.close_modal();
                            }
                        });
                }
                build(cx)
            })
            .0
        });
        dismissed |= portal.engine.clicked(entity);
        if dismissed && state.open {
            state.close();
        }
        ModalResponse {
            inner: Some(inner),
            dismissed,
        }
    }
}

/// Portable state for a dismissible anchored panel.
pub type PopoverState = ModalState;

/// A trigger and its anchored portal, declared together without retained callbacks.
pub struct Popover<'u, 'e, 's> {
    modal: Modal<'u, 'e, 's>,
    disabled: bool,
    trigger_text: Option<&'s str>,
}
impl<'e> UiContext<'e> {
    pub fn popover<'u, 's>(
        &'u mut self,
        state: &'s mut PopoverState,
        label: impl Into<Cow<'s, str>>,
    ) -> Popover<'u, 'e, 's> {
        Popover {
            modal: self.modal(state).title(label).width(240.),
            disabled: false,
            trigger_text: None,
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
    pub(crate) fn display_text(mut self, text: &'s str) -> Self {
        self.trigger_text = Some(text);
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
        let style = Style {
            anchor_id: Some(anchor),
            width: self.trigger_text.map(|_| self.modal.width),
            gap: 8.,
            role: (self.modal.panel_role == Some(crate::WidgetRole::ListBox))
                .then_some(crate::WidgetRole::ComboBox),
            expanded: Some(self.modal.state.is_open()),
            disabled,
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
        let (_, clicked) = ui.element(
            self.modal.key,
            if self.trigger_text.is_some() {
                Kind::ButtonBase
            } else {
                Kind::Button
            },
            style,
            self.modal.title.as_ref(),
            |cx| {
                if let Some(shown) = self.trigger_text {
                    crate::widgets::text(cx, shown).grow(1.);
                    crate::widgets::svg(cx, &crate::icons::CHEVRON_DOWN).size(16., 16.);
                }
            },
        );
        if clicked && !disabled {
            if self.modal.state.is_open() {
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
