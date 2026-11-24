//! Window-backed command menu for hosts using the custom desktop presentation.
use super::*;
use crate::ContextMenuItem;
use kurbo::{Affine, BezPath, Rect, Stroke};
use peniko::{Color, Fill};
use plico_renderer::PaintScene;
use winit::{
    dpi::LogicalPosition,
    keyboard::{Key, NamedKey},
    window::{
        Window, WindowAnchor, WindowConstraintAdjustment as Adjust, WindowGravity,
        WindowPositioner, WindowType,
    },
};

const PANEL_RADIUS: u32 = 8;
const ROW: u32 = 32;
const PAD: u32 = 2;
const SEPARATOR: u32 = 9;
const TEXT_INSET: f64 = 15.;
const MARK_COLUMN: f64 = 28.;
const LIMIT: usize = 16;

pub(super) struct Popup {
    shell: Shell,
    #[cfg(target_os = "linux")]
    x11_shape: Option<x11_shape::PopupShape>,
    owner: Arc<dyn Window>,
    requester: WindowResourceKey,
    id: crate::EffectId,
    levels: Vec<Vec<ContextMenuItem>>,
    selected: Option<usize>,
    first: usize,
    first_frame: bool,
    focused: bool,
    visible: usize,
    width: u32,
}
fn height_of(item: Option<&ContextMenuItem>) -> u32 {
    if matches!(item, Some(ContextMenuItem::Separator)) {
        SEPARATOR
    } else {
        ROW
    }
}

fn label_layout(
    text: &mut plico_renderer::TextContext,
    label: &str,
    shade: u8,
) -> Arc<parley::Layout<[u8; 4]>> {
    plico_renderer::text::layout_text_styled(
        text,
        Arc::from(label),
        14.,
        400.,
        false,
        None,
        plico_renderer::style::TextAlign::Start,
        [shade, shade, shade, 255],
        Arc::from([]),
        None,
        None,
    )
}

fn dimensions(
    text: &mut plico_renderer::TextContext,
    items: &[ContextMenuItem],
    nested: bool,
) -> (u32, u32) {
    let mut heights = Vec::new();
    let mut width = 0.;
    let mut height = 0;
    let marks = items
        .iter()
        .any(|item| matches!(item, ContextMenuItem::Item { checked: true, .. }));
    let left = TEXT_INSET + if marks { MARK_COLUMN } else { 0. };
    if nested {
        heights.push(ROW);
        width = left + label_layout(text, "Back", 0).width() as f64 + TEXT_INSET + 20.;
    }
    for item in items {
        heights.push(height_of(Some(item)));
        let (label, extra) = match item {
            ContextMenuItem::Separator => continue,
            ContextMenuItem::Item { label, .. } => (label, 0.),
            ContextMenuItem::Submenu { label, items } => {
                let (w, h) = dimensions(text, items, true);
                width = width.max(f64::from(w));
                height = height.max(h);
                (label, 36.)
            }
        };
        width =
            width.max(left + f64::from(label_layout(text, label, 0).width()) + extra + TEXT_INSET);
    }
    let count = heights.len().clamp(1, LIMIT);
    height = height.max(
        heights
            .windows(count)
            .map(|rows| rows.iter().sum::<u32>() + 2 * PAD)
            .max()
            .unwrap_or(2 * PAD),
    );
    ((width.ceil() as u32).clamp(160, 560), height)
}

#[cfg(target_os = "linux")]
fn fit_axis(anchor: i32, size: i32, start: i32, end: i32) -> i32 {
    if anchor.saturating_add(size) <= end {
        anchor
    } else {
        anchor.saturating_sub(size).max(start)
    }
}
impl Popup {
    pub(super) fn owner_id(&self) -> WindowId {
        self.owner.id()
    }
    pub(super) fn open(
        events: &dyn ActiveEventLoop,
        parent: &mut Shell,
        menu: &crate::ContextMenuRequest,
        requester: WindowResourceKey,
        id: crate::EffectId,
        x: f64,
        y: f64,
    ) -> Result<Self, String> {
        let (width, height) = dimensions(parent.tcx_mut(), &menu.items, false);
        let owner = parent.window();
        let raw = owner.window_handle().map_err(|e| e.to_string())?.as_raw();
        let options = WindowOptions::new()
            .title("Menu")
            .initial_inner_size(width, height)
            .decorations(false)
            .linux_csd(false)
            .centered(false)
            .resizable(false)
            .corner_radius(PANEL_RADIUS);
        // Wayland composes the submitted alpha channel. X11 uses a bounding
        // shape, which also works on desktops without a compositor.
        #[cfg(target_os = "linux")]
        let options = if matches!(raw, RawWindowHandle::Wayland(_)) {
            options.background(plico_renderer::platform::WindowBackground::Transparent)
        } else {
            options
        };
        let shell = Shell::create_with_attributes(events, &options, |attrs| {
            #[cfg(target_os = "linux")]
            if matches!(raw, RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) {
                use winit::platform::x11::{WindowAttributesX11, WindowType as XType};
                let origin = owner.outer_position().unwrap_or_default();
                let inset = owner.surface_position();
                let scale = owner.scale_factor();
                let mut position = PhysicalPosition::new(
                    origin.x + inset.x + (x * scale) as i32,
                    origin.y + inset.y + (y * scale) as i32,
                );
                for monitor in owner.available_monitors() {
                    let (Some(origin), Some(mode)) =
                        (monitor.position(), monitor.current_video_mode())
                    else {
                        continue;
                    };
                    let right = origin.x + mode.size().width as i32;
                    let bottom = origin.y + mode.size().height as i32;
                    if (origin.x..right).contains(&position.x)
                        && (origin.y..bottom).contains(&position.y)
                    {
                        position.x = fit_axis(
                            position.x,
                            (f64::from(width) * scale).ceil() as i32,
                            origin.x,
                            right,
                        );
                        position.y = fit_axis(
                            position.y,
                            (f64::from(height) * scale).ceil() as i32,
                            origin.y,
                            bottom,
                        );
                        break;
                    }
                }
                return attrs
                    .with_position(position)
                    .with_platform_attributes(Box::new(
                        WindowAttributesX11::default().with_x11_window_type(vec![XType::PopupMenu]),
                    ));
            }
            let positioner = WindowPositioner::new(
                WindowAnchor::TopLeft,
                (
                    LogicalPosition::new(x, y).into(),
                    LogicalSize::new(1, 1).into(),
                ),
                LogicalPosition::new(0, 0).into(),
                WindowGravity::BottomRight,
                Adjust::FLIP_X | Adjust::FLIP_Y | Adjust::SLIDE_X | Adjust::SLIDE_Y,
            );
            // Popup keeps its parent alive until after the child Shell drops.
            unsafe { attrs.with_parent_window(Some(raw)) }
                .with_window_type(WindowType::Popup)
                .with_positioner(positioner)
        })
        .map_err(|e| e.to_string())?;

        #[cfg(target_os = "linux")]
        let x11_shape = x11_shape::PopupShape::new(shell.window().as_ref())?;
        shell.window().request_redraw();
        Ok(Self {
            shell,
            #[cfg(target_os = "linux")]
            x11_shape,
            owner: owner.clone(),
            requester,
            id,
            levels: vec![menu.items.clone()],
            selected: None,
            first: 0,
            focused: false,
            first_frame: true,
            visible: LIMIT,
            width,
        })
    }
    fn back(&self) -> usize {
        usize::from(self.levels.len() > 1)
    }
    fn len(&self) -> usize {
        self.levels.last().unwrap().len() + self.back()
    }
    fn item(&self, row: usize) -> Option<&ContextMenuItem> {
        self.levels.last()?.get(row.checked_sub(self.back())?)
    }
    fn enabled(&self, row: usize) -> bool {
        row < self.len()
            && !matches!(
                self.item(row),
                Some(ContextMenuItem::Separator | ContextMenuItem::Item { enabled: false, .. })
            )
    }
    fn point(&mut self, position: PhysicalPosition<f64>) {
        let p = position.to_logical::<f64>(self.shell.scale_factor());
        let mut top = f64::from(PAD);
        self.selected = (self.first..self.len().min(self.first + self.visible)).find(|&row| {
            let bottom = top + f64::from(height_of(self.item(row)));
            let hit = p.x >= 4.
                && p.x < self.width as f64 - 4.
                && p.y >= top
                && p.y < bottom
                && self.enabled(row);
            top = bottom;
            hit
        });
    }
    fn step(&mut self, forward: bool) {
        let n = self.len();
        let start = self.selected.unwrap_or(if forward { n - 1 } else { 0 });
        self.selected = (1..=n)
            .map(|i| {
                if forward {
                    (start + i) % n
                } else {
                    (start + n - i) % n
                }
            })
            .find(|&i| self.enabled(i));
        if let Some(i) = self.selected {
            self.first = self.first.min(i).max((i + 1).saturating_sub(self.visible));
        }
    }
    fn ascend(&mut self) {
        if self.back() > 0 {
            self.levels.pop();
            self.first = 0;
            self.selected = None;
        }
    }
    fn activate(&mut self) -> Option<String> {
        let i = self.selected?;
        match self.item(i) {
            Some(ContextMenuItem::Item {
                id, enabled: true, ..
            }) => return Some(id.clone()),
            Some(ContextMenuItem::Submenu { items, .. }) => {
                let children = items.clone();
                self.levels.push(children);
                self.first = 0;
                self.selected = None;
            }
            None => self.ascend(),
            _ => {}
        }
        None
    }
    fn paint(&mut self) {
        self.shell.sync_surface_size();
        self.shell.scene_mut().reset();
        let scale = self.shell.scale_factor();
        let transform = Affine::scale(scale);
        #[cfg(target_os = "linux")]
        if let Some(shape) = &mut self.x11_shape {
            shape.update(self.shell.window().surface_size(), scale);
        }
        // MyGO internal/windows/menudraw.go, fluentPalette and MenuFlyoutItem metrics.
        let dark = self.owner.theme() == Some(winit::window::Theme::Dark);
        let (back, plate, ink, muted, line, border) = if dark {
            (44, 56, 255, 120, 61, 65)
        } else {
            (249, 240, 26, 159, 234, 224)
        };
        let gray = |v| Color::from_rgb8(v, v, v);
        let width = f64::from(self.width);
        let height = self.shell.window().surface_size().height as f64 / scale;
        let panel = Rect::new(
            0.5 / scale,
            0.5 / scale,
            width - 0.5 / scale,
            height - 0.5 / scale,
        )
        .to_rounded_rect(f64::from(PANEL_RADIUS));
        self.shell.scene_mut().stroke(
            &Stroke::new(1. / scale),
            transform,
            gray(border),
            None,
            &panel,
        );
        let marks = self
            .levels
            .last()
            .unwrap()
            .iter()
            .any(|item| matches!(item, ContextMenuItem::Item { checked: true, .. }));
        let mut bottom = f64::from(PAD);
        for row in self.first..self.len().min(self.first + self.visible) {
            let top = bottom;
            bottom += f64::from(height_of(self.item(row)));
            let center = (top + bottom) * 0.5;
            let enabled = self.enabled(row);
            let (label, checked, submenu, back_row) = match self.item(row) {
                Some(ContextMenuItem::Separator) => {
                    self.shell.scene_mut().fill(
                        Fill::NonZero,
                        transform,
                        gray(line),
                        None,
                        &Rect::new(4., center.floor(), width - 4., center.floor() + 1. / scale),
                    );
                    continue;
                }
                Some(ContextMenuItem::Item { label, checked, .. }) => {
                    (label.clone(), *checked, false, false)
                }
                Some(ContextMenuItem::Submenu { label, .. }) => (label.clone(), false, true, false),
                None => ("Back".into(), false, false, true),
            };
            if self.selected == Some(row) {
                self.shell.scene_mut().fill(
                    Fill::NonZero,
                    transform,
                    gray(plate),
                    None,
                    &Rect::new(4., top + 2., width - 4., bottom - 2.).to_rounded_rect(4.),
                );
            }
            let mut mark = BezPath::new();
            if checked {
                mark.move_to((TEXT_INSET + 1., center));
                mark.line_to((TEXT_INSET + 4., center + 3.));
                mark.line_to((TEXT_INSET + 11., center - 4.));
            } else if submenu || back_row {
                let x = if submenu {
                    width - TEXT_INSET - 7.
                } else {
                    TEXT_INSET + 7.
                };
                let dx = if submenu { 4. } else { -4. };
                mark.move_to((x, center - 4.));
                mark.line_to((x + dx, center));
                mark.line_to((x, center + 4.));
            }
            self.shell
                .scene_mut()
                .stroke(&Stroke::new(1.5), transform, gray(ink), None, &mark);
            let text = self.shell.tcx_mut();
            let layout = label_layout(text, &label, if enabled { ink } else { muted });
            let x = TEXT_INSET + if marks || back_row { MARK_COLUMN } else { 0. };
            let y = center - f64::from(layout.height()) / 2.;
            let glyphs = text.glyph_scene(&layout);
            self.shell
                .scene_mut()
                .append_scene((*glyphs).clone(), transform * Affine::translate((x, y)));
        }
        #[cfg(target_os = "linux")]
        let clear = if self.x11_shape.is_none() {
            crate::window_frame::clip_window_frame(
                self.shell.scene_mut(),
                [
                    (width * scale).round() as u32,
                    (height * scale).round() as u32,
                ],
                f64::from(PANEL_RADIUS) * scale,
                gray(back),
            )
        } else {
            gray(back)
        };
        #[cfg(target_os = "windows")]
        let clear = self.shell.prepare_window_frame(gray(back));
        self.shell.present(clear);
        if std::mem::take(&mut self.first_frame) {
            self.shell.window().focus_window();
        }
    }
}
impl MultiWindowApp {
    pub(super) fn dismiss_popup(&mut self, value: Option<String>, focus: bool) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let Popup {
            requester,
            id,
            owner,
            shell,
            ..
        } = popup;
        drop(shell);
        if focus {
            owner.focus_window();
        }
        if let Some(app) = find_window_by_key(self.windows.values_mut(), requester)
            .or_else(|| self.hidden_windows.get_mut(&requester))
        {
            app.source.complete_effect(crate::EffectCompletion {
                id,
                op: crate::effect::builtin::CONTEXT_MENU_SHOW,
                result: crate::EffectResult::ContextMenuSelection(value),
            });
            if let Some(shell) = &app.state {
                shell.window().request_redraw();
            }
        }
    }
    pub(super) fn popup_event(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        let Some(p) = &mut self.popup else {
            return false;
        };
        if id != p.shell.window().id() {
            if matches!(
                event,
                WindowEvent::PointerButton {
                    state: ElementState::Pressed,
                    ..
                }
            ) {
                self.dismiss_popup(None, false);
                return true;
            }
            if id == p.owner.id()
                && matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed)
            {
                self.dismiss_popup(None, false);
            }
            return false;
        }
        let mut selection = None;
        match event {
            WindowEvent::RedrawRequested => {
                p.paint();
                return true;
            }
            WindowEvent::Focused(true) => {
                p.focused = true;
                return true;
            }
            WindowEvent::Focused(false) if !p.focused => return true,
            WindowEvent::Focused(false) | WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.dismiss_popup(None, false);
                return true;
            }
            WindowEvent::PointerMoved { position, .. } => p.point(*position),
            WindowEvent::PointerLeft { .. } => p.selected = None,
            WindowEvent::PointerButton {
                state: ElementState::Released,
                button,
                position,
                ..
            } if pointer_button(button) == PointerButton::Primary => {
                p.point(*position);
                selection = p.activate();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
                    MouseScrollDelta::PixelDelta(p) => p.y,
                    _ => return true,
                };
                p.first = p
                    .first
                    .saturating_add_signed(-y.signum() as isize)
                    .min(p.len().saturating_sub(p.visible));
                p.selected = None;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.dismiss_popup(None, true);
                        return true;
                    }
                    Key::Named(NamedKey::ArrowUp) => p.step(false),
                    Key::Named(NamedKey::ArrowDown) => p.step(true),
                    Key::Named(NamedKey::ArrowLeft) => p.ascend(),
                    Key::Named(NamedKey::ArrowRight)
                        if p.selected.is_some_and(|i| {
                            matches!(p.item(i), Some(ContextMenuItem::Submenu { .. }))
                        }) =>
                    {
                        p.activate();
                    }
                    Key::Character(s) if s.as_str() == " " => selection = p.activate(),
                    Key::Named(NamedKey::Enter) => selection = p.activate(),
                    _ => {}
                }
            }
            WindowEvent::SurfaceResized(_) => {}
            _ => return true,
        }
        if selection.is_some() {
            self.dismiss_popup(selection, true);
        } else {
            p.shell.window().request_redraw();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flyout_measures_compact_rules_and_the_largest_level() {
        let mut text = plico_renderer::TextContext::new();
        let action = ContextMenuItem::Item {
            label: "Copy".into(),
            id: "copy".into(),
            checked: false,
            enabled: true,
        };
        let level = vec![action.clone(), ContextMenuItem::Separator, action];
        assert_eq!(dimensions(&mut text, &level, false), (160, 77));
        let submenu = ContextMenuItem::Submenu {
            label: "View".into(),
            items: level,
        };
        assert_eq!(dimensions(&mut text, &[submenu], false), (160, 109));
    }
}

#[cfg(target_os = "linux")]
mod x11_shape {
    use super::*;
    use x11rb::{
        protocol::{
            shape::{ConnectionExt, SK, SO},
            xproto::{ClipOrdering, Rectangle},
        },
        rust_connection::RustConnection,
    };

    pub(super) struct PopupShape {
        connection: RustConnection,
        window: u32,
        applied: Option<(PhysicalSize<u32>, u32)>,
    }
    impl PopupShape {
        pub(super) fn new(window: &dyn Window) -> Result<Option<Self>, String> {
            let id = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
                RawWindowHandle::Xlib(h) => h.window as u32,
                RawWindowHandle::Xcb(h) => h.window.get(),
                _ => return Ok(None),
            };
            let (connection, _) = x11rb::connect(None).map_err(|e| e.to_string())?;
            let mut shape = Self {
                connection,
                window: id,
                applied: None,
            };
            shape.update(window.surface_size(), window.scale_factor());
            Ok(Some(shape))
        }
        pub(super) fn update(&mut self, size: PhysicalSize<u32>, scale: f64) {
            let radius = (f64::from(PANEL_RADIUS) * scale).round() as u32;
            if self.applied == Some((size, radius)) {
                return;
            }
            let rects = rounded_region(size, radius);
            match self.connection.shape_rectangles(
                SO::SET,
                SK::BOUNDING,
                ClipOrdering::Y_SORTED,
                self.window,
                0,
                0,
                &rects,
            ) {
                Ok(cookie) => match cookie.check() {
                    Ok(()) => self.applied = Some((size, radius)),
                    Err(e) => tracing::warn!("could not set popup bounds: {e}"),
                },
                Err(e) => tracing::warn!("could not send popup bounds: {e}"),
            }
        }
    }
    fn rounded_region(size: PhysicalSize<u32>, radius: u32) -> Vec<Rectangle> {
        let w = size.width.min(i16::MAX as u32) as u16;
        let h = size.height.min(i16::MAX as u32) as u16;
        let r = radius.min(u32::from(w.min(h) / 2)) as u16;
        let mut bands = Vec::<Rectangle>::new();
        for y in 0..h {
            let edge = y.min(h - 1 - y);
            let inset = if edge < r {
                let r = f64::from(r);
                let dy = r - f64::from(edge) - 0.5;
                (r - (r * r - dy * dy).sqrt()).round() as u16
            } else {
                0
            };
            if let Some(last) = bands.last_mut()
                && last.x == inset as i16
            {
                last.height += 1;
            } else {
                bands.push(Rectangle {
                    x: inset as i16,
                    y: y as i16,
                    width: w - 2 * inset,
                    height: 1,
                });
            }
        }
        bands
    }
}
