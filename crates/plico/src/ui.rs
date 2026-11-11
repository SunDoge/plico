use crate::{Entity, Theme, engine::Engine, model::*, motion::ValueChannel};
use std::borrow::Cow;

mod builders;
mod layout;
pub use builders::*;
pub use layout::*;

/// Context borrowed while declaring one UI tree. Widget constructors live in `widgets`.
pub struct UiContext<'e> {
    pub(crate) engine: &'e mut Engine,
    pub(crate) localizer: &'e dyn crate::Localizer,
    pub(crate) parent: Entity,
    pub(crate) scope: u64,
    pub(crate) inherited: Style,
    pub(crate) in_button: bool,
    pub(crate) toolbar_buttons: bool,
    pub(crate) accordion_items: bool,
    pub(crate) accordion_index: usize,
}
impl<'e> UiContext<'e> {
    /// Native accessibility preferences. Custom drawing can follow the same
    /// policy as the framework's materials, motion and semantic theme colors.
    pub fn display_preferences(&self) -> crate::DisplayPreferences {
        self.engine.display_preferences()
    }
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
    /// outer context naturally. Applications redeclare after locale changes.
    pub fn localized<R>(
        &mut self,
        localizer: &dyn crate::Localizer,
        build: impl FnOnce(&mut UiContext<'_>) -> R,
    ) -> R {
        let (result, accordion_index) = {
            let mut ui = UiContext {
                engine: &mut *self.engine,
                localizer,
                parent: self.parent,
                scope: self.scope,
                inherited: self.inherited,
                in_button: self.in_button,
                toolbar_buttons: self.toolbar_buttons,
                accordion_items: self.accordion_items,
                accordion_index: self.accordion_index,
            };
            let result = build(&mut ui);
            (result, ui.accordion_index)
        };
        self.accordion_index = accordion_index;
        result
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

    fn channel_state<T: 'static>(
        &mut self,
        namespace: u8,
        name: &'static str,
        init: impl FnOnce() -> T,
    ) -> crate::LocalState<T> {
        self.engine.locals.get_channel(
            self.parent,
            self.scope,
            namespace,
            name,
            self.engine.epoch,
            init,
        )
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
            toolbar_buttons: false,
            accordion_items: false,
            accordion_index: 0,
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
    /// Border-box size resolved by the host on the preceding layout pass.
    /// Reading it registers this node for sparse geometry feedback; nodes that
    /// do not read it incur no declaration invalidation when their size changes.
    pub fn layout_size(&self) -> Option<[f32; 2]> {
        self.engine.layout_size(self.parent)
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
            semantic_label: None,
            numeric: None,
            toolbar_buttons: false,
            accordion_items: false,
            material: None,
        }
    }
    #[inline(always)]
    pub fn text_input<'u, 't>(
        &'u mut self,
        label: impl Into<Cow<'t, str>>,
        buffer: &crate::TextBuffer,
    ) -> Input<'u, 'e, 't> {
        let theme = self.theme();
        let mut inner = self.leaf(Kind::TextInput, label.into());
        style_input(&mut inner.style, theme);
        inner.style.width = Some(240.);
        inner.style.clip = true;
        Input {
            inner,
            buffer: buffer.clone(),
            string: None,
            placeholder: Cow::Borrowed(""),
            autofocus: false,
            select_all_on_autofocus: false,
            focus: false,
        }
    }
    /// Composite fields reuse the same chrome as TextInput and supply their
    /// own editor/icon children, following MyGO's field constructor.
    pub(crate) fn input_field(&mut self) -> Layout<'_, 'e> {
        let theme = self.theme();
        let mut field = self.row();
        style_input(&mut field.style, theme);
        field.focus_within_border(1., Color::Accent)
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
            select_all_on_autofocus: false,
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
        let theme = self.theme();
        let appearance = if self.toolbar_buttons {
            ButtonAppearance::Toolbar
        } else {
            ButtonAppearance::Ordinary
        };
        let mut inner = self.leaf(Kind::Button, text.into());
        style_button(&mut inner.style, theme, appearance);
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
    /// Schedule another display-paced declaration frame. Renderer-owned
    /// presentation motion uses its retained animation path instead; this is
    /// for app declarations such as [`Self::loop_progress`] and
    /// [`Self::animate`] that must sample the injected clock again.
    pub fn request_animation_frame(&mut self) {
        self.engine.changes.insert(Changes::ANIMATE);
    }
    /// The progress of an animation that starts over every `period`, from
    /// 0 toward 1, sampled at the injected frame clock (MyGO's Loop).
    /// The current element and `key` are the channel identity, as in MyGO;
    /// several call sites can intentionally address the same loop. While a
    /// channel is declared, frames keep coming.
    /// Omitting it releases the local channel; if it is later declared again,
    /// it starts from zero like a newly mounted MyGO element.
    ///
    /// A spinner turns with `loop_progress("spin", 900, Ease::Linear) * 360`
    /// and a placeholder pulses with an opacity of
    /// `0.5 + 0.5 * loop_progress("pulse", period, Ease::EaseInOut)`.
    pub fn loop_progress(
        &mut self,
        key: &'static str,
        period: std::time::Duration,
        ease: crate::Easing,
    ) -> f32 {
        // Element scope and key form the channel identity. Local state makes a
        // loop start fresh after its declaring element unmounts, as MyGO's.
        let now = self.now();
        let start = *self
            .channel_state::<std::time::Duration>(1, key, || now)
            .read();
        self.request_animation_frame();
        let period = period.max(std::time::Duration::from_millis(1));
        let elapsed = self.now() - start;
        let phase = elapsed.as_nanos() % period.as_nanos();
        ease.sample(phase as f64 / period.as_nanos() as f64) as f32
    }
    /// Animate a declaration or layout value toward `target`, easing out as
    /// MyGO's `Animate` does. The named channel belongs to the current element
    /// and component scope, survives declaration-order changes, and retargets
    /// continuously.
    ///
    /// The first value is used immediately, and reduced-motion preferences
    /// snap to the target without scheduling another frame.
    #[track_caller]
    pub fn animate(
        &mut self,
        key: &'static str,
        target: f32,
        duration: std::time::Duration,
    ) -> f32 {
        self.animate_with(key, target, duration, Easing::EaseOut)
    }
    /// Animate a declaration or layout value with an explicit duration and curve.
    #[track_caller]
    pub fn animate_with(
        &mut self,
        key: &'static str,
        target: f32,
        duration: std::time::Duration,
        easing: Easing,
    ) -> f32 {
        self.animate_to(
            key,
            target,
            crate::Animation::tween(duration.max(std::time::Duration::from_millis(1)), easing),
        )
    }
    /// Animate with a curve, spring or delay. Springs preserve the channel's
    /// velocity when retargeted, including velocity handed off by a tween.
    #[track_caller]
    pub fn animate_to(
        &mut self,
        key: &'static str,
        target: f32,
        animation: crate::Animation,
    ) -> f32 {
        assert!(target.is_finite(), "animation target must be finite");
        animation.validate();
        let now = self.now();
        let state = self.channel_state(2, key, || {
            std::cell::RefCell::new(ValueChannel::new(target, now))
        });
        let (value, active) = state.read().borrow_mut().sample(
            now,
            target,
            animation,
            self.display_preferences().reduce_motion,
        );
        if active {
            self.request_animation_frame();
        }
        value
    }
    /// Follow `target` with an analytic spring.
    #[track_caller]
    pub fn spring(&mut self, key: &'static str, target: f32, options: crate::SpringOptions) -> f32 {
        self.animate_to(
            key,
            target,
            crate::Animation {
                spring: options,
                ..Default::default()
            },
        )
    }
    /// Set a channel immediately. Successive jumps record pointer-like
    /// velocity, which a later [`Self::spring`] keeps when it takes over.
    pub fn jump(&mut self, key: &'static str, value: f32) -> f32 {
        assert!(value.is_finite(), "animation value must be finite");
        let now = self.now();
        let state = self.channel_state(2, key, || {
            std::cell::RefCell::new(ValueChannel::new(value, now))
        });
        let state = state.read();
        state.borrow_mut().jump(now, value)
    }
    /// Current scalar channel velocity in value units per second.
    pub fn velocity(&self, key: &'static str) -> f32 {
        self.engine
            .locals
            .channel::<std::cell::RefCell<ValueChannel>>(self.parent, self.scope, 2, key)
            .map_or(0., |state| state.read().borrow().velocity())
    }
    /// Whether a scalar channel is at rest. Unknown channels are settled.
    pub fn settled(&self, key: &'static str) -> bool {
        self.engine
            .locals
            .channel::<std::cell::RefCell<ValueChannel>>(self.parent, self.scope, 2, key)
            .is_none_or(|state| !state.read().borrow().active())
    }
    /// Re-evaluate the view at an injected-clock deadline, retaining its
    /// declarations before then. Re-declaring or removing this component
    /// cancels its old deadline; multiple requests keep the earliest one.
    pub fn request_rebuild_at(&mut self, deadline: std::time::Duration) {
        let node = self.engine.nodes.get_mut(self.parent).unwrap();
        node.wake_at = Some(node.wake_at.map_or(deadline, |old| old.min(deadline)));
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
    /// Whether the current composed control owns keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.engine.focused() == Some(self.parent)
    }
    /// Request keyboard focus for the current composed control once in response
    /// to an application action (for example, grid arrow-key navigation).
    pub fn request_focus(&mut self) {
        if !self.inherited.disabled {
            self.engine.focus_request = Some(self.parent);
        }
    }
    /// Consume a secondary-button click on the current composed control.
    /// Like primary activation, the press and release must target the same
    /// enabled control; dragging away cancels the click.
    pub fn secondary_clicked(&mut self) -> bool {
        let clicked = self.engine.secondary_clicked(self.parent);
        clicked && !self.inherited.disabled
    }
    /// Current host pointer position in window logical pixels, when the
    /// pointer is inside the window. Read during declarations; pointer
    /// movement triggers a settling re-declaration like other input state.
    pub fn pointer_position(&self) -> Option<(f64, f64)> {
        self.engine.pointer_position()
    }
    /// Pointer position normalized to the current composed control's layout
    /// rectangle. This is renderer-independent geometry feedback for direct
    /// manipulation surfaces such as color fields and canvases.
    ///
    /// Values may leave 0..=1 while the pointer is dragged outside; controls
    /// decide whether to clamp them. The first declaration registers the
    /// reader and returns `None` until the host has presented its geometry.
    pub fn pointer_fraction(&self) -> Option<[f32; 2]> {
        self.engine.pointer_fraction(self.parent)
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
        let (result, accordion_index) = {
            let mut ui = UiContext {
                engine: &mut *self.engine,
                localizer: self.localizer,
                parent,
                scope,
                inherited: self.inherited,
                in_button: self.in_button,
                toolbar_buttons: self.toolbar_buttons,
                accordion_items: self.accordion_items,
                accordion_index: self.accordion_index,
            };
            let result = build(&mut ui);
            (result, ui.accordion_index)
        };
        self.accordion_index = accordion_index;
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
        inner.style.transition_ms = metrics.interaction_ms;
        inner.style.padding = 0.;
        inner.style.gap = metrics.spacing as f32 * if kind == Kind::Switch { 1.5 } else { 2. };
        inner.style.radius = metrics.radius as f32;
        inner.style.min_height =
            Some(metrics.space(if kind == Kind::Switch { 5 } else { 4 }) as f32);
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
