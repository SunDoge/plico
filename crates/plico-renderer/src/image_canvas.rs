//! Retained raster canvas with original-pixel rectangle editing.
//! IO, OCR, persistence and undo belong to the host application.
use crate::{
    PaintContext, PaintScene, PointerButton, PointerId, PointerPhase, UiEvent, WakeCallback,
    Widget, WidgetChanges, WidgetEventResult, WidgetGeometry, image::RasterImage,
};
use kurbo::{Affine, Rect, Stroke};
use peniko::{Color, Fill};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// Primary-pointer operation; the middle button always pans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CanvasTool {
    /// Create a new rectangle.
    #[default]
    Draw,
    /// Select, move or resize a rectangle.
    Select,
    /// Move the viewport.
    Pan,
}
/// One host-owned rectangle, in original image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasRegion {
    /// Stable business identity.
    pub id: u64,
    /// Left, top, right, bottom; image-local coordinates.
    pub rect: [f64; 4],
}
/// Committed operations, never transient drag positions.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasEvent {
    /// A new rectangle was drawn.
    Added([f64; 4]),
    /// An existing rectangle was changed.
    Edited(CanvasRegion),
    /// Selection changed, possibly to nothing.
    Selected(Option<u64>),
    /// Delete requested; the host decides whether to remove it.
    Delete(u64),
}
struct Shared {
    image: Option<Arc<RasterImage>>,
    regions: Vec<CanvasRegion>,
    selected: Option<u64>,
    tool: CanvasTool,
    zoom: f64,
    pan: [f64; 2],
    size: [f32; 2],
    revision: u64,
    document: u64,
    events: VecDeque<CanvasEvent>,
    wake: WakeCallback,
}
/// Retained resource handle. Reuse image identities; no encode/decode during paint.
#[derive(Clone)]
pub struct ImageCanvasState(Arc<Mutex<Shared>>);
impl Default for ImageCanvasState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Shared {
            image: None,
            regions: vec![],
            selected: None,
            tool: CanvasTool::Draw,
            zoom: 1.,
            pan: [0.; 2],
            size: [0.; 2],
            revision: 1,
            document: 0,
            events: VecDeque::new(),
            wake: Arc::new(|| {}),
        })))
    }
}
impl ImageCanvasState {
    fn update(&self, f: impl FnOnce(&mut Shared)) {
        let wake = {
            let mut s = self.0.lock().unwrap();
            f(&mut s);
            s.revision += 1;
            s.wake.clone()
        };
        wake();
    }
    /// Replace the document, clear queued edits and reset the view. None clears it.
    pub fn set_document(&self, image: Option<Arc<RasterImage>>, regions: Vec<CanvasRegion>) {
        self.update(|s| {
            s.image = image.filter(|i| i.size().iter().all(|v| *v > 0.));
            s.regions = regions;
            sanitize(s);
            s.selected = None;
            s.zoom = 1.;
            s.pan = [0.; 2];
            s.document += 1;
            s.events.clear();
        });
    }
    /// Replace overlays after a host-side edit/undo. Cancels an in-progress edit.
    pub fn set_regions(&self, regions: Vec<CanvasRegion>, selected: Option<u64>) {
        self.update(|s| {
            s.regions = regions;
            s.selected = selected;
            sanitize(s);
            s.document += 1;
        });
    }
    /// Set the tool; changing tools cancels an uncommitted drag.
    pub fn set_tool(&self, tool: CanvasTool) {
        self.update(|s| {
            s.tool = tool;
            s.document += 1;
        });
    }
    /// Fit the complete image to the available layout slot.
    pub fn fit(&self) {
        self.update(|s| {
            s.zoom = 1.;
            s.pan = [0.; 2];
            s.document += 1;
        });
    }
    /// Display one image pixel per logical pixel, centered in the viewport.
    pub fn actual_size(&self) {
        self.update(|s| {
            s.zoom = 1. / fit_scale(s);
            s.pan = [0.; 2];
            s.document += 1;
        });
    }
    /// Zoom around the viewport center; wheel input uses its own pointer anchor.
    pub fn zoom(&self, factor: f64) {
        if factor.is_finite() && factor > 0. {
            self.update(|s| zoom_at(s, [s.size[0] as f64 / 2., s.size[1] as f64 / 2.], factor));
        }
    }
    /// Move the viewport by logical pixels, as a keyboard/button alternative.
    pub fn pan(&self, delta: [f64; 2]) {
        if delta.iter().all(|v| v.is_finite()) {
            self.update(|s| {
                s.pan[0] += delta[0];
                s.pan[1] += delta[1];
            });
        }
    }
    /// Pop one committed operation. Process these on the UI/host thread.
    pub fn take_event(&self) -> Option<CanvasEvent> {
        self.0.lock().unwrap().events.pop_front()
    }
    /// Current image-to-logical-pixel scale.
    pub fn scale(&self) -> f64 {
        let s = self.0.lock().unwrap();
        fit_scale(&s) * s.zoom
    }
}
fn fit_scale(s: &Shared) -> f64 {
    let size = s.image.as_ref().map(|i| i.size()).unwrap_or([1.; 2]);
    (s.size[0] as f64 / size[0] as f64)
        .min(s.size[1] as f64 / size[1] as f64)
        .max(0.00001)
}
fn transform(s: &Shared) -> (f64, [f64; 2]) {
    let size = s.image.as_ref().map(|i| i.size()).unwrap_or([1.; 2]);
    let scale = fit_scale(s) * s.zoom;
    (
        scale,
        [
            (s.size[0] as f64 - size[0] as f64 * scale) / 2. + s.pan[0],
            (s.size[1] as f64 - size[1] as f64 * scale) / 2. + s.pan[1],
        ],
    )
}
fn image_point(s: &Shared, p: [f64; 2]) -> [f64; 2] {
    let (z, o) = transform(s);
    [(p[0] - o[0]) / z, (p[1] - o[1]) / z]
}
fn zoom_at(s: &mut Shared, p: [f64; 2], factor: f64) {
    let anchor = image_point(s, p);
    s.zoom = (s.zoom * factor).clamp(0.1, 64.);
    let (z, o) = transform(s);
    s.pan[0] += p[0] - o[0] - anchor[0] * z;
    s.pan[1] += p[1] - o[1] - anchor[1] * z;
}
fn contains(r: [f64; 4], p: [f64; 2]) -> bool {
    p[0] >= r[0] && p[1] >= r[1] && p[0] <= r[2] && p[1] <= r[3]
}
fn sanitize(s: &mut Shared) {
    let mut ids = std::collections::HashSet::new();
    s.regions = std::mem::take(&mut s.regions)
        .into_iter()
        .filter(|r| clamp_rect(s, r.rect) == Some(r.rect) && ids.insert(r.id))
        .collect();
    s.selected = s
        .selected
        .filter(|id| s.regions.iter().any(|r| r.id == *id));
}
fn clamp_rect(s: &Shared, r: [f64; 4]) -> Option<[f64; 4]> {
    if r.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let [w, h] = s.image.as_ref()?.size().map(f64::from);
    let r = [
        r[0].min(r[2]).clamp(0., w),
        r[1].min(r[3]).clamp(0., h),
        r[0].max(r[2]).clamp(0., w),
        r[1].max(r[3]).clamp(0., h),
    ];
    (r.iter().all(|v| v.is_finite()) && r[2] - r[0] >= 2. && r[3] - r[1] >= 2.).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point, PointerEvent, PointerProperties};
    fn fixture() -> (ImageCanvasState, ImageCanvas) {
        let state = ImageCanvasState::default();
        state.set_document(
            Some(Arc::new(RasterImage::from_rgba(
                image::RgbaImage::from_pixel(100, 200, image::Rgba([255; 4])),
            ))),
            vec![],
        );
        let mut widget = ImageCanvas::new(state.clone());
        widget.layout_changed(WidgetGeometry {
            content_size: [200., 400.],
            device_scale: 2.,
            local_to_window: [1., 0., 0., 1., 0., 0.],
            window_to_local: [1., 0., 0., 1., 0., 0.],
        });
        (state, widget)
    }
    fn pointer(widget: &mut ImageCanvas, phase: PointerPhase, x: f64, y: f64) {
        widget.handle_event(&UiEvent::Pointer(PointerEvent {
            phase,
            position: Point { x, y },
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Default::default(),
            properties: PointerProperties::default(),
        }));
    }
    #[test]
    fn draws_original_pixel_rect_with_reverse_drag_and_outside_release() {
        let (state, mut widget) = fixture();
        pointer(&mut widget, PointerPhase::Down, 160., 360.);
        pointer(&mut widget, PointerPhase::Up, -10., 40.);
        assert_eq!(
            state.take_event(),
            Some(CanvasEvent::Added([0., 20., 80., 180.]))
        );
        assert!(state.take_event().is_none());
    }
    #[test]
    fn zoom_anchor_and_idle_resource_reuse() {
        let (state, mut widget) = fixture();
        let image = state.0.lock().unwrap().image.clone().unwrap();
        {
            let mut s = state.0.lock().unwrap();
            let p = [43., 123.];
            let before = image_point(&s, p);
            zoom_at(&mut s, p, 2.);
            assert_eq!(before, image_point(&s, p));
        }
        widget.poll_async_changes();
        assert!(widget.poll_async_changes().is_empty());
        let mut text = crate::TextContext::new();
        let mut cx = PaintContext::new(200., 400., 2., &mut text);
        widget.paint(&mut cx);
        assert!(!cx.finish().commands.is_empty());
        assert!(Arc::ptr_eq(
            &image,
            state.0.lock().unwrap().image.as_ref().unwrap()
        ));
    }
    #[test]
    fn document_replacement_and_focus_loss_cancel_uncommitted_drag() {
        let (state, mut widget) = fixture();
        pointer(&mut widget, PointerPhase::Down, 20., 20.);
        widget.focus_changed(false);
        pointer(&mut widget, PointerPhase::Up, 100., 100.);
        assert!(state.take_event().is_none());
        pointer(&mut widget, PointerPhase::Down, 20., 20.);
        state.set_document(None, vec![]);
        pointer(&mut widget, PointerPhase::Up, 100., 100.);
        assert!(state.take_event().is_none());
    }
    #[test]
    fn select_move_resize_and_escape() {
        let (state, mut widget) = fixture();
        let r = CanvasRegion {
            id: 7,
            rect: [10., 20., 60., 80.],
        };
        state.set_regions(vec![r], Some(7));
        state.set_tool(CanvasTool::Select);
        pointer(&mut widget, PointerPhase::Down, 70., 100.);
        pointer(&mut widget, PointerPhase::Up, 90., 120.);
        assert_eq!(state.take_event(), Some(CanvasEvent::Selected(Some(7))));
        assert_eq!(
            state.take_event(),
            Some(CanvasEvent::Edited(CanvasRegion {
                id: 7,
                rect: [20., 30., 70., 90.]
            }))
        );
        pointer(&mut widget, PointerPhase::Down, 120., 160.);
        pointer(&mut widget, PointerPhase::Up, 160., 200.);
        assert_eq!(state.take_event(), Some(CanvasEvent::Selected(Some(7))));
        assert_eq!(
            state.take_event(),
            Some(CanvasEvent::Edited(CanvasRegion {
                id: 7,
                rect: [10., 20., 80., 100.]
            }))
        );
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Draw,
    Pan([f64; 2]),
    Move(CanvasRegion),
    Resize(CanvasRegion, usize),
}
#[derive(Clone, Copy)]
struct Drag {
    pointer: PointerId,
    start: [f64; 2],
    current: [f64; 2],
    operation: Operation,
    document: u64,
}
/// Framework widget suitable for a `SurfaceSource` layout slot.
pub struct ImageCanvas {
    state: ImageCanvasState,
    revision: u64,
    drag: Option<Drag>,
}
impl ImageCanvas {
    /// Bind the host's retained canvas resource.
    pub fn new(state: ImageCanvasState) -> Self {
        Self {
            state,
            revision: 0,
            drag: None,
        }
    }
    fn pending(s: &Shared, drag: Drag) -> Option<CanvasRegion> {
        let a = image_point(s, drag.start);
        let b = image_point(s, drag.current);
        let (id, rect) = match drag.operation {
            Operation::Draw => (0, [a[0], a[1], b[0], b[1]]),
            Operation::Move(region) => {
                let [w, h] = s.image.as_ref()?.size().map(f64::from);
                let r = region.rect;
                let dx = (b[0] - a[0]).clamp(-r[0], w - r[2]);
                let dy = (b[1] - a[1]).clamp(-r[1], h - r[3]);
                (region.id, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy])
            }
            Operation::Resize(region, corner) => {
                let mut r = region.rect;
                r[if corner % 2 == 0 { 0 } else { 2 }] = b[0];
                r[if corner < 2 { 1 } else { 3 }] = b[1];
                (region.id, r)
            }
            Operation::Pan(_) => return None,
        };
        clamp_rect(s, rect).map(|rect| CanvasRegion { id, rect })
    }
}
impl Widget for ImageCanvas {
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.state.0.lock().unwrap().wake = wake;
    }
    fn poll_async_changes(&mut self) -> WidgetChanges {
        let s = self.state.0.lock().unwrap();
        if self.drag.is_some_and(|d| d.document != s.document) {
            self.drag = None;
        }
        if self.revision == s.revision {
            WidgetChanges::empty()
        } else {
            self.revision = s.revision;
            WidgetChanges::REDRAW
        }
    }
    fn layout_changed(&mut self, geometry: WidgetGeometry) {
        let mut s = self.state.0.lock().unwrap();
        if s.size != geometry.content_size {
            s.size = geometry.content_size;
            self.drag = None;
        }
    }
    fn focus_changed(&mut self, focused: bool) -> WidgetChanges {
        if !focused {
            self.drag = None;
        }
        WidgetChanges::REDRAW
    }
    fn visibility_changed(&mut self, visible: bool) -> WidgetChanges {
        if !visible {
            self.drag = None;
        }
        WidgetChanges::REDRAW
    }
    fn handle_event(&mut self, event: &UiEvent) -> WidgetEventResult {
        let mut s = self.state.0.lock().unwrap();
        if self.drag.is_some_and(|d| d.document != s.document) {
            self.drag = None;
        }
        if s.image.is_none() {
            return WidgetEventResult::IGNORED;
        }
        match event {
            UiEvent::Wheel(w)
                if self.drag.is_none()
                    && w.delta_y.is_finite()
                    && w.position.x.is_finite()
                    && w.position.y.is_finite() =>
            {
                zoom_at(
                    &mut s,
                    [w.position.x, w.position.y],
                    (-w.delta_y * 0.0025).exp(),
                );
            }
            UiEvent::Key(k) if k.phase == crate::KeyPhase::Down => match k.key.as_str() {
                "Escape" => self.drag = None,
                "Delete" | "Backspace" => {
                    if let Some(id) = s.selected {
                        s.events.push_back(CanvasEvent::Delete(id));
                    }
                }
                _ => return WidgetEventResult::IGNORED,
            },
            UiEvent::Pointer(p) => {
                let point = [p.position.x, p.position.y];
                match p.phase {
                    PointerPhase::Down
                        if self.drag.is_none()
                            && matches!(
                                p.button,
                                Some(PointerButton::Primary | PointerButton::Auxiliary)
                            ) =>
                    {
                        let operation = if p.button == Some(PointerButton::Auxiliary)
                            || s.tool == CanvasTool::Pan
                        {
                            Operation::Pan(s.pan)
                        } else if s.tool == CanvasTool::Draw {
                            let q = image_point(&s, point);
                            let size = s.image.as_ref().unwrap().size();
                            if !contains([0., 0., size[0] as f64, size[1] as f64], q) {
                                return WidgetEventResult::HANDLED;
                            }
                            Operation::Draw
                        } else {
                            let q = image_point(&s, point);
                            let radius = 8. / (fit_scale(&s) * s.zoom);
                            let corner = s
                                .regions
                                .iter()
                                .find(|r| Some(r.id) == s.selected)
                                .and_then(|r| {
                                    [
                                        [r.rect[0], r.rect[1]],
                                        [r.rect[2], r.rect[1]],
                                        [r.rect[0], r.rect[3]],
                                        [r.rect[2], r.rect[3]],
                                    ]
                                    .iter()
                                    .position(|c| {
                                        (q[0] - c[0]).abs() <= radius
                                            && (q[1] - c[1]).abs() <= radius
                                    })
                                    .map(|i| (*r, i))
                                });
                            let hit = s
                                .regions
                                .iter()
                                .rev()
                                .find(|r| contains(r.rect, q))
                                .copied();
                            let selected = corner.map(|(r, _)| r.id).or(hit.map(|r| r.id));
                            s.selected = selected;
                            s.events.push_back(CanvasEvent::Selected(selected));
                            if let Some((r, i)) = corner {
                                Operation::Resize(r, i)
                            } else if let Some(r) = hit {
                                Operation::Move(r)
                            } else {
                                return WidgetEventResult::HANDLED;
                            }
                        };
                        self.drag = Some(Drag {
                            pointer: p.properties.id,
                            start: point,
                            current: point,
                            operation,
                            document: s.document,
                        });
                    }
                    PointerPhase::Move | PointerPhase::Up
                        if self.drag.is_some_and(|d| d.pointer == p.properties.id) =>
                    {
                        let drag = self.drag.as_mut().unwrap();
                        drag.current = point;
                        if let Operation::Pan(start) = drag.operation {
                            s.pan = [
                                start[0] + point[0] - drag.start[0],
                                start[1] + point[1] - drag.start[1],
                            ];
                        }
                        if p.phase == PointerPhase::Up {
                            if let Some(region) = Self::pending(&s, *drag) {
                                let event = if matches!(drag.operation, Operation::Draw) {
                                    CanvasEvent::Added(region.rect)
                                } else {
                                    CanvasEvent::Edited(region)
                                };
                                s.events.push_back(event);
                            }
                            self.drag = None;
                        }
                    }
                    PointerPhase::Cancel => self.drag = None,
                    _ => return WidgetEventResult::IGNORED,
                }
            }
            _ => return WidgetEventResult::IGNORED,
        }
        s.revision += 1;
        let wake = s.wake.clone();
        drop(s);
        wake();
        WidgetEventResult::HANDLED
    }
    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        self.poll_async_changes();
        let s = self.state.0.lock().unwrap();
        let Some(image) = s.image.as_ref() else {
            return;
        };
        let (scale, origin) = transform(&s);
        let transform = Affine::translate((origin[0], origin[1])) * Affine::scale(scale);
        let scene = cx.scene_mut();
        scene.draw_image(image.brush().into(), transform);
        let pending = self.drag.and_then(|d| Self::pending(&s, d));
        for region in s.regions.iter().copied().chain(pending.filter(|_| {
            self.drag
                .is_some_and(|d| matches!(d.operation, Operation::Draw))
        })) {
            let region = pending
                .filter(|p| {
                    p.id == region.id
                        && !self
                            .drag
                            .is_some_and(|d| matches!(d.operation, Operation::Draw))
                })
                .unwrap_or(region);
            let r = region.rect;
            let selected = Some(region.id) == s.selected;
            let color = if selected {
                Color::from_rgb8(234, 88, 12)
            } else {
                Color::from_rgb8(37, 99, 235)
            };
            scene.stroke(
                &Stroke::new(2. / scale),
                transform,
                color,
                None,
                &Rect::new(r[0], r[1], r[2], r[3]),
            );
            if selected {
                for [x, y] in [[r[0], r[1]], [r[2], r[1]], [r[0], r[3]], [r[2], r[3]]] {
                    let d = 4. / scale;
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        color,
                        None,
                        &Rect::new(x - d, y - d, x + d, y + d),
                    );
                }
            }
        }
    }
}
