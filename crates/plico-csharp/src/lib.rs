//! C# frontend experiment: bounded batch ABI, shared Plico Engine/runtime/wgpu.
pub mod abi;
mod batch;
mod canvas;
mod properties;
mod virtual_list;
mod window;
use abi::{Batch, Build, Event, Frame, Stats};
use batch::*;
use plico::{ImageAsset, TextBuffer, prelude::*};
use plico_renderer::image::RasterImage;
use plico_renderer::image_canvas::{ImageCanvas, ImageCanvasState};
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::{HostView, SurfaceSource};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Debug, snafu::Snafu)]
#[snafu(display("C# bridge: {message}"))]
struct Error {
    message: String,
}
thread_local! {
    static HOSTS: RefCell<HashMap<u64,Host>> = RefCell::new(HashMap::new());
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Host {
    clock: Option<plico::VirtualClock>,
    source: SurfaceSource<ManagedView, ImageCanvas>,
    text: TextContext,
}
// Only this mailbox crosses threads; the host and managed callback stay UI-owned.
#[derive(Default)]
struct Mailbox {
    canvas: ImageCanvasState,
    released: Mutex<std::collections::HashSet<u64>>,
    pending: AtomicBool,
    images: Mutex<HashMap<u64, ImageResource>>,
    wake: Mutex<Option<plico_renderer::WakeCallback>>,
}
static MAILBOXES: OnceLock<Mutex<HashMap<u64, Arc<Mailbox>>>> = OnceLock::new();
fn mailboxes() -> &'static Mutex<HashMap<u64, Arc<Mailbox>>> {
    MAILBOXES.get_or_init(Mutex::default)
}
struct ImageResource {
    asset: Option<ImageAsset>,
    raster: Arc<RasterImage>,
}
struct ManagedView {
    canvas_input: Option<canvas::Description>,
    canvas_resource: u64,
    canvas_node: Option<u64>,
    pending_replies: Vec<Reply>,
    id: u64,
    mailbox: Arc<Mailbox>,
    build: Build,
    user: *mut c_void,
    batch: Option<OwnedBatch>,
    dirty: bool,
    inputs: HashMap<u64, TextBuffer>,
    images: HashMap<u64, ImageResource>,
    stats: Stats,
    error: Option<String>,
}
struct Reply {
    id: u64,
    kind: u32,
    value: f64,
    text: String,
}
impl Drop for Reply {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.text);
    }
}
impl ManagedView {
    fn rebuild(&mut self, cx: &UiContext, events: &[Reply]) -> Result<(), String> {
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        let events: Vec<_> = events
            .iter()
            .map(|event| {
                let text_start = bytes.len() as u32;
                bytes.extend_from_slice(event.text.as_bytes());
                Event {
                    id: event.id,
                    kind: event.kind,
                    value: event.value,
                    text_start,
                    text_len: event.text.len() as u32,
                }
            })
            .collect();
        let [width, height] = cx.viewport_size();
        let frame = Frame {
            version: 5,
            width,
            height,
            seconds: cx.now().as_secs_f64(),
            events: events.as_ptr(),
            event_count: events.len() as u32,
            utf8: bytes.as_ptr(),
            utf8_len: bytes.len() as u32,
        };
        let mut batch = Batch {
            version: 0,
            dark: 0,
            animate: 0,
            commands: std::ptr::null(),
            command_count: 0,
            utf8: std::ptr::null(),
            utf8_len: 0,
        };
        let start = Instant::now();
        // Callback lifetime is owned by the caller of create/run; C# catches
        // exceptions and pins its reusable arrays through the synchronous copy.
        let status = unsafe { (self.build)(self.user, &frame, &mut batch) };
        self.stats.callbacks += 1;
        self.stats.callback_micros += start.elapsed().as_micros() as u64;
        if status != 0 {
            return Err(format!(
                "managed build failed ({status}); see managed stderr"
            ));
        }
        let start = Instant::now();
        let next = unsafe { OwnedBatch::copy(batch) }?;
        self.images.extend(
            self.mailbox
                .images
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(),
        );
        for command in &next.commands {
            if (command.kind == IMAGE || (command.kind == CANVAS && command.resource != 0))
                && !self.images.contains_key(&command.resource)
            {
                return Err("unknown image resource".into());
            }
        }
        if next.commands.iter().any(|c| {
            c.kind == IMAGE
                && self
                    .images
                    .get(&c.resource)
                    .is_none_or(|r| r.asset.is_none())
        }) {
            return Err("image resource is canvas-only".into());
        }
        let canvas = canvas::decode(&next)?;
        self.sync_canvas(canvas)?;
        // Adopt atomically: no buffer edit or tree mutation before validation.
        let input_ids: std::collections::HashSet<_> = next
            .commands
            .iter()
            .filter(|c| c.kind == INPUT)
            .map(|c| c.id)
            .collect();
        self.inputs.retain(|id, _| input_ids.contains(id));
        for command in &next.commands {
            if command.kind == INPUT {
                let buffer = self.inputs.entry(command.id).or_insert_with(|| {
                    if command.flags & (1 << 18) != 0 {
                        TextBuffer::secret()
                    } else if command.flags & (1 << 19) != 0 {
                        TextBuffer::multiline()
                    } else {
                        TextBuffer::new()
                    }
                });
                let secret = command.flags & (1 << 18) != 0;
                let multiline = command.flags & (1 << 19) != 0;
                if buffer.is_secret() != secret || buffer.is_multiline() != multiline {
                    *buffer = if secret {
                        TextBuffer::secret()
                    } else if multiline {
                        TextBuffer::multiline()
                    } else {
                        TextBuffer::new()
                    };
                }
                let value = next.text(command.value_start, command.value_len);
                if buffer.with_value(|current| current != value) {
                    buffer.set(value.to_owned().into());
                }
            }
        }
        self.stats.decode_micros += start.elapsed().as_micros() as u64;
        self.stats.batches += 1;
        self.stats.commands += next.commands.len() as u64;
        self.batch = Some(next);
        self.collect_images();
        self.dirty = false;
        Ok(())
    }
}
impl Drop for ManagedView {
    fn drop(&mut self) {
        mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}
impl HostView for ManagedView {
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        *self.mailbox.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake.clone());
        if self.mailbox.pending.load(Ordering::Acquire) {
            wake();
        }
    }
    fn poll_updates(&mut self) -> bool {
        self.drain_canvas_events();
        self.collect_images();
        self.images.extend(
            self.mailbox
                .images
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(),
        );
        if self.mailbox.pending.swap(false, Ordering::AcqRel) || !self.pending_replies.is_empty() {
            self.dirty = true;
            self.error = None;
            true
        } else {
            false
        }
    }
    fn on_unhandled_key(&mut self, key: &plico::KeyEvent) -> bool {
        if key.phase != plico::KeyPhase::Down
            || key.synthetic
            || key.modifiers.control()
            || key.modifiers.meta()
            || key.modifiers.alt()
            || key.modifiers.shift()
        {
            return false;
        }
        let Some(batch) = &self.batch else {
            return false;
        };
        let target = batch.commands.iter().find(|command| {
            if !batch
                .properties
                .get(&command.id)
                .is_some_and(|properties| properties.keys.contains(&key.key))
            {
                return false;
            }
            let mut node = *command;
            loop {
                if node.flags & DISABLED != 0 {
                    return false;
                }
                if node.parent == batch::ROOT {
                    break;
                }
                node = &batch.commands[node.parent as usize];
            }
            true
        });
        let Some(target) = target else {
            return false;
        };
        self.pending_replies.push(Reply {
            id: target.id,
            kind: 8,
            value: 0.,
            text: key.key.clone(),
        });
        self.dirty = true;
        true
    }
    fn theme(&self) -> Theme {
        if self.batch.as_ref().is_some_and(|b| b.dark) {
            Theme::dark()
        } else {
            Theme {
                surface: 0xffffff,
                ..Theme::light()
            }
        }
    }
    fn show(&mut self, cx: &mut UiContext) {
        self.poll_updates();
        if let Some(error) = &self.error {
            column(cx).padding(24).children(|cx| {
                text(cx, "C# UI build failed")
                    .font_size(24)
                    .color(Color::Danger);
                text(cx, error).color(Color::Muted);
            });
            return;
        }
        let pending = std::mem::take(&mut self.pending_replies);
        if (self.dirty || self.batch.is_none())
            && let Err(error) = self.rebuild(cx, &pending)
        {
            self.error = Some(error);
            cx.request_rebuild();
            return;
        }
        if self.theme() != cx.theme() {
            cx.request_rebuild();
        }
        let start = Instant::now();
        let mut events = Vec::new();
        if let Some(batch) = &self.batch {
            let mut render = Render {
                batch,
                inputs: &self.inputs,
                images: &self.images,
                events: &mut events,
            };
            for &root in &batch.roots {
                render.node(cx, root);
            }
        }
        self.stats.declarations += 1;
        self.stats.declare_micros += start.elapsed().as_micros() as u64;
        if !events.is_empty() {
            match self.rebuild(cx, &events) {
                Ok(()) => cx.request_rebuild(),
                Err(error) => {
                    self.error = Some(error);
                    cx.request_rebuild();
                }
            }
        }
        if self.batch.as_ref().is_some_and(|b| b.animate) {
            self.dirty = true;
            cx.request_animation_frame();
        }
    }
}
struct Render<'a> {
    batch: &'a OwnedBatch,
    inputs: &'a HashMap<u64, TextBuffer>,
    images: &'a HashMap<u64, ImageResource>,
    events: &'a mut Vec<Reply>,
}
// Both container and leaf builders share these style methods. Identity is
// explicit in the batch; no proc macro or source parsing crosses the ABI.
macro_rules! styled {
    ($builder:expr,$command:expr) => {{
        let c = $command;
        let mut b = $builder
            .key(c.id)
            .grow(c.grow)
            .shrink(c.shrink)
            .opacity(c.opacity)
            .min_width(c.min_width)
            .min_height(c.min_height);
        if c.flags & (1 << 15) != 0 {
            b = b.motion(plico::Motion::Smooth);
        }
        if c.border_width > 0. {
            b = b.border(c.border_width, Color::Literal(c.border_color));
        }
        if c.padding >= 0. {
            b = b.padding(c.padding);
        }
        if c.radius >= 0. {
            b = b.radius(c.radius);
        }
        if c.width >= 0. {
            b = if c.flags & (1 << 16) != 0 {
                b.width_percent(c.width)
            } else {
                b.width(c.width)
            };
        }
        if c.height >= 0. {
            b = b.height(c.height);
        }
        if c.flags & WIDTH_FILL != 0 {
            b = b.width_percent(100.);
        }
        if c.flags & HEIGHT_FILL != 0 {
            b = b.height_percent(100.);
        }
        if c.flags & BACKGROUND != 0 {
            b = b.background(Color::Literal(c.background));
        }
        b
    }};
}
fn primitive_style(c: abi::Command) -> plico::Style {
    plico::Style {
        layout: plico::LayoutStyle {
            position: plico::Position::Relative,
            ..Default::default()
        },
        opacity: c.opacity,
        width: (c.width >= 0. && c.flags & (1 << 16) == 0).then_some(c.width),
        height: (c.height >= 0.).then_some(c.height),
        width_percent: if c.flags & (1 << 16) != 0 {
            Some(c.width)
        } else {
            (c.flags & WIDTH_FILL != 0).then_some(100.)
        },
        height_percent: (c.flags & HEIGHT_FILL != 0).then_some(100.),
        min_width: Some(c.min_width),
        min_height: Some(c.min_height),
        grow: c.grow,
        shrink: c.shrink,
        gap: c.gap,
        padding: c.padding.max(0.),
        radius: c.radius.max(0.),
        border_width: c.border_width,
        border_color: Color::Literal(c.border_color),
        background: (c.flags & BACKGROUND != 0).then_some(Color::Literal(c.background)),
        foreground: if c.flags & FOREGROUND != 0 {
            Color::Literal(c.foreground)
        } else {
            Color::Foreground
        },
        hover_background: (c.flags & (1 << 22) != 0).then_some(Color::Literal(c.hover_background)),
        pressed_background: (c.flags & (1 << 23) != 0)
            .then_some(Color::Literal(c.pressed_background)),
        font_size: if c.font_size > 0 {
            c.font_size as u16
        } else {
            14
        },
        font_weight: if c.weight > 0 { c.weight as u16 } else { 400 },
        clip: c.kind == INPUT,
        disabled: c.flags & DISABLED != 0,
        checked: c.flags & (1 << 24) != 0,
        selected: (c.semantic_role == 4).then_some(c.flags & (1 << 24) != 0),
        anchor_id: (c.kind == NODE).then_some(c.id),
        overlay: c.kind == PORTAL,
        popup_anchor: (c.kind == PORTAL && c.resource != 0).then_some(c.resource),
        modal_open: c.kind == PORTAL,
        dismiss_on_escape: c.kind == PORTAL,
        dismiss_on_backdrop: c.kind == PORTAL,
        role: match c.semantic_role {
            1 => Some(plico::WidgetRole::CheckBox),
            2 => Some(plico::WidgetRole::Switch),
            3 => Some(plico::WidgetRole::ComboBox),
            4 => Some(plico::WidgetRole::Option),
            5 => Some(plico::WidgetRole::ListBox),
            _ => None,
        },
        align: Some(match (c.flags >> 9) & 3 {
            1 => Align::Center,
            2 => Align::End,
            3 => Align::Stretch,
            _ => Align::Start,
        }),
        justify: Some(if c.flags & (1 << 17) != 0 {
            Justify::Start
        } else {
            match (c.flags >> 11) & 3 {
                1 => Justify::Center,
                2 => Justify::End,
                3 => Justify::SpaceBetween,
                _ => Justify::Start,
            }
        }),
        ..Default::default()
    }
}
impl Render<'_> {
    fn style(&self, c: abi::Command) -> plico::Style {
        let mut style = primitive_style(c);
        style.gap = c.gap;
        style.wrap = c.flags & (1 << 13) != 0;
        style.scroll_y = c.flags & SCROLL != 0;
        if let Some(extra) = self.batch.properties.get(&c.id) {
            extra.apply(&mut style);
        }
        style
    }
    fn node(&mut self, cx: &mut UiContext, index: usize) {
        let batch = self.batch;
        let c = batch.commands[index];
        let label = batch.text(c.text_start, c.text_len);
        match c.kind {
            VIRTUAL_LIST => {
                let description = &batch.lists[&c.id];
                let children = &batch.children[index];
                let mut list =
                    plico::widgets::virtual_list(cx, description.count, description.row_height)
                        .key(c.id)
                        .height(if c.height > 0. { c.height } else { 240. });
                if c.grow > 0. || c.flags & HEIGHT_FILL != 0 {
                    list = list.fill();
                }
                if c.flags & WIDTH_FILL != 0 {
                    list = list.width_percent(100.);
                } else if c.width >= 0. {
                    list = if c.flags & (1 << 16) != 0 {
                        list.width_percent(c.width)
                    } else {
                        list.width(c.width)
                    };
                }
                let response = list.rows(
                    |i| {
                        if i >= description.start && i < description.end {
                            batch.commands[children[i - description.start]].id
                        } else {
                            i as u64
                        }
                    },
                    |cx, i| {
                        if i >= description.start && i < description.end {
                            self.node(cx, children[i - description.start]);
                        }
                    },
                );
                if response.rendered != (description.start..description.end) {
                    self.events.push(Reply {
                        id: c.id,
                        kind: 7,
                        value: 0.,
                        text: format!("{},{}", response.rendered.start, response.rendered.end),
                    });
                }
            }
            CANVAS => {
                styled!(column(cx), c)
                    .key("csharp-image-canvas")
                    .children(|_| {});
            }
            COLUMN | ROW => {
                let style = self.style(c);
                cx.element(
                    c.id.into(),
                    if c.kind == COLUMN {
                        plico::Kind::Column
                    } else {
                        plico::Kind::Row
                    },
                    style,
                    "",
                    |cx| {
                        for &child in &batch.children[index] {
                            self.node(cx, child);
                        }
                    },
                );
            }
            SHADER => {
                let mut style = self.style(c);
                if c.width < 0. && c.flags & WIDTH_FILL == 0 {
                    style.width = Some(320.);
                }
                if c.height < 0. && c.flags & HEIGHT_FILL == 0 {
                    style.height = Some(180.);
                }
                style.clip = true;
                let mut params = plico::ShaderParams {
                    animated: c.flags & (1 << 14) != 0,
                    ..Default::default()
                };
                if let Some(extra) = batch.properties.get(&c.id).and_then(|p| p.shader.as_ref()) {
                    params.values = extra.values.clone();
                    params.reads_backdrop = extra.backdrop;
                    params.module = extra.module;
                    params.time = extra.time;
                    params.paused = extra.paused;
                    if extra.pointer {
                        let (x, y) = cx.pointer_position().unwrap_or((-1., -1.));
                        params.values = vec![x as f32, y as f32];
                    }
                }
                cx.shader_element(c.id.into(), style, label, &params);
            }
            TEXT => {
                let spans: Vec<_> = batch
                    .properties
                    .get(&c.id)
                    .map(|p| p.spans.iter().map(|s| s.as_span()).collect())
                    .unwrap_or_default();
                cx.rich_text_element(c.id.into(), self.style(c), label, &spans);
            }
            NODE | PORTAL | PANEL => {
                // Primitive box + optional activation. Composition, colors and
                // interaction policy are supplied by the managed widget code.
                let style = self.style(c);
                let kind = if c.kind == PORTAL {
                    plico::Kind::Overlay
                } else if c.kind == PANEL {
                    plico::Kind::Dialog
                } else if c.flags & (1 << 21) != 0 {
                    plico::Kind::ButtonBase
                } else {
                    plico::Kind::Row
                };
                let name = label;
                let build = |cx: &mut UiContext<'_>| {
                    if batch.properties.get(&c.id).is_some_and(|p| p.draggable) {
                        // Backend-owned pointer anchor, disposed with this node.
                        let anchor = cx.state(|| None::<(f64, f64)>);
                        let next = cx.pressed().then(|| cx.pointer_position()).flatten();
                        let previous = *anchor.read();
                        anchor.update(|value| *value = next);
                        if let (Some((x, y)), Some((px, py))) = (next, previous)
                            && (x != px || y != py)
                        {
                            self.events.push(Reply {
                                id: c.id,
                                kind: 8,
                                value: x - px,
                                text: (y - py).to_string(),
                            });
                        }
                    }
                    for &child in &batch.children[index] {
                        self.node(cx, child);
                    }
                };
                let (_, clicked) = if c.kind == PORTAL {
                    cx.portal_element(c.id.into(), kind, style, name, build)
                } else {
                    cx.element(c.id.into(), kind, style, name, build)
                };
                if clicked {
                    self.events.push(Reply {
                        id: c.id,
                        kind: 1,
                        value: 0.,
                        text: String::new(),
                    });
                }
            }
            SVG => {
                cx.element(c.id.into(), plico::Kind::Svg, self.style(c), label, |_| {});
            }
            NUMERIC_INPUT => {
                // Invisible native range editing surface: pointer capture,
                // keyboard and accessibility only. Managed code paints the UI.
                let mut range = plico::NumericRange {
                    min: c.min,
                    max: c.max,
                    value: c.value,
                    high: None,
                    step: 0.,
                    vertical: false,
                    reversed: false,
                };
                let style = plico::Style {
                    absolute: Some([0., 0.]),
                    width_percent: Some(100.),
                    height_percent: Some(100.),
                    control_chrome: false,
                    disabled: c.flags & DISABLED != 0,
                    ..Default::default()
                };
                if let Some(value) = cx.slider_element(c.id.into(), style, label, &mut range) {
                    self.events.push(Reply {
                        id: c.id,
                        kind: 3,
                        value,
                        text: String::new(),
                    });
                }
            }
            INPUT => {
                let buffer = &self.inputs[&c.id];
                let extra = batch.properties.get(&c.id);
                let response = cx
                    .text_edit_surface(c.id.into(), self.style(c), label, buffer)
                    .placeholder(extra.and_then(|p| p.placeholder.as_deref()).unwrap_or(""))
                    .request_focus(extra.is_some_and(|p| p.focus))
                    .navigation(c.flags & (1 << 25) != 0)
                    .autofocus(c.flags & (1 << 26) != 0)
                    .disabled(c.flags & DISABLED != 0)
                    .response();
                if response.changed || response.submitted || response.navigation.is_some() {
                    self.events.push(Reply {
                        id: c.id,
                        kind: 4,
                        value: match response.navigation {
                            Some(plico::InputNavigation::Next) => 2.,
                            Some(plico::InputNavigation::Previous) => -2.,
                            Some(plico::InputNavigation::PageDown) => 3.,
                            Some(plico::InputNavigation::PageUp) => -3.,
                            Some(plico::InputNavigation::Cancel) => 4.,
                            None => f64::from(response.submitted),
                        },
                        text: buffer.with_value(str::to_owned),
                    });
                }
            }
            IMAGE => {
                let asset = self.images[&c.resource]
                    .asset
                    .as_ref()
                    .expect("validated image asset");
                cx.element(
                    c.id.into(),
                    plico::Kind::Image,
                    self.style(c),
                    asset.encoded_str(),
                    |_| {},
                );
            }
            _ => unreachable!("validated kind"),
        }
    }
}
fn boundary<T: Default>(operation: impl FnOnce() -> Result<T, String>) -> T {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => {
            ERROR.with(|e| e.borrow_mut().clear());
            value
        }
        result => {
            let message = match result {
                Ok(Err(message)) => message,
                Err(payload) => payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "native panic".into()),
                _ => unreachable!(),
            };
            ERROR.with(|e| *e.borrow_mut() = Error { message }.to_string());
            T::default()
        }
    }
}
fn status(operation: impl FnOnce() -> Result<(), String>) -> i32 {
    boundary(|| {
        operation()?;
        Ok(1)
    })
}
fn with_host<T>(id: u64, f: impl FnOnce(&mut Host) -> Result<T, String>) -> Result<T, String> {
    HOSTS.with(|hosts| {
        let mut hosts = hosts
            .try_borrow_mut()
            .map_err(|_| "reentrant native host access")?;
        f(hosts.get_mut(&id).ok_or("unknown host or wrong thread")?)
    })
}
unsafe fn slice<'a, T>(ptr: *const T, count: usize, max: usize) -> Result<&'a [T], String> {
    if count > max
        || count
            .checked_mul(std::mem::size_of::<T>())
            .is_none_or(|n| n > isize::MAX as usize)
    {
        return Err("foreign buffer exceeds budget".into());
    }
    if count == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err("null/misaligned foreign buffer".into());
    }
    // Caller guarantees readable memory. Resource bounds do not prove pointer validity.
    Ok(unsafe { std::slice::from_raw_parts(ptr, count) })
}
fn utf8<'a>(ptr: *const u8, len: u32) -> Result<&'a str, String> {
    std::str::from_utf8(unsafe { slice(ptr, len as usize, 1024 * 1024)? })
        .map_err(|e| e.to_string())
}
fn dimensions(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        Err("invalid viewport".into())
    } else {
        Ok(())
    }
}
fn create(build: Option<Build>, user: *mut c_void) -> u64 {
    create_with_clock(build, user, false)
}
fn create_with_clock(build: Option<Build>, user: *mut c_void, virtual_time: bool) -> u64 {
    boundary(|| {
        let build = build.ok_or("missing managed callback")?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mailbox = Arc::new(Mailbox::default());
        mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, mailbox.clone());
        let canvas_state = mailbox.canvas.clone();
        let view = ManagedView {
            canvas_input: None,
            canvas_resource: 0,
            canvas_node: None,
            pending_replies: Vec::new(),
            id,
            mailbox,
            build,
            user,
            batch: None,
            dirty: true,
            inputs: HashMap::new(),
            images: HashMap::new(),
            stats: Stats::default(),
            error: None,
        };
        let clock = virtual_time.then(plico::VirtualClock::default);
        HOSTS.with(|hosts| {
            hosts.borrow_mut().insert(
                id,
                Host {
                    source: match &clock {
                        Some(clock) => SurfaceSource::with_clock(
                            view,
                            ImageCanvas::new(canvas_state),
                            "csharp-image-canvas",
                            clock.clone(),
                        ),
                        None => SurfaceSource::new(
                            view,
                            ImageCanvas::new(canvas_state),
                            "csharp-image-canvas",
                        ),
                    },
                    clock,
                    text: TextContext::new(),
                },
            )
        });
        Ok(id)
    })
}
fn destroy(id: u64) -> i32 {
    status(|| {
        HOSTS
            .with(|hosts| {
                hosts
                    .try_borrow_mut()
                    .map_err(|_| "reentrant destroy")
                    .map(|mut h| {
                        h.remove(&id);
                    })
            })
            .map_err(str::to_owned)
    })
}
fn load_image(id: u64, path: *const u8, len: u32) -> u64 {
    boundary(|| {
        let path = utf8(path, len)?;
        let mailbox = mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or("host is closed")?;
        // Decode on the calling thread; C# async callers use a worker thread.
        let image = ::image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())?;
        if u64::from(image.width()) * u64::from(image.height()) > 32 * 1024 * 1024 {
            return Err("image exceeds 32 megapixels".into());
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, ::image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let asset = ImageAsset::from_bytes(png.get_ref());
        let key = NEXT.fetch_add(1, Ordering::Relaxed);
        mailbox
            .images
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                key,
                ImageResource {
                    asset: Some(asset),
                    raster: Arc::new(RasterImage::from_rgba(image.to_rgba8())),
                },
            );
        if wake(id) != 1 {
            return Err("host closed while loading image".into());
        }
        Ok(key)
    })
}
fn view_error(host: &Host) -> Result<(), String> {
    if let Some(error) = &host.source.source.view.error {
        Err(error.clone())
    } else if let Some(error) = host.source.source.last_error() {
        Err(error.to_string())
    } else {
        Ok(())
    }
}
fn capture(id: u64, path: *const u8, len: u32, w: u32, h: u32) -> i32 {
    status(|| {
        dimensions(w, h)?;
        let path = utf8(path, len)?;
        with_host(id, |host| {
            host.source.poll_async();
            // Snapshot the requested state, not an in-flight old-theme endpoint.
            host.source.source.set_reduced_motion(true);
            let result = plico_desktop::capture(&mut host.source).size(w, h).render();
            host.source.source.set_reduced_motion(false);
            let frame = result.map_err(|e| e.to_string())?;
            view_error(host)?;
            frame.write_png(path).map_err(|e| e.to_string())
        })
    })
}
fn run(id: u64, title: *const u8, len: u32, w: u32, h: u32) -> i32 {
    status(|| {
        dimensions(w, h)?;
        let options = plico_desktop::WindowOptions::new()
            .title(utf8(title, len)?)
            .initial_inner_size(w, h)
            .min_inner_size(640, 480);
        run_options(id, options)
    })
}
fn run_configured(id: u64, json: *const u8, len: u32) -> i32 {
    status(|| run_options(id, window::parse(utf8(json, len)?)?))
}
fn run_options(id: u64, options: plico_desktop::WindowOptions) -> Result<(), String> {
    let host = HOSTS
        .with(|hosts| {
            hosts
                .try_borrow_mut()
                .map_err(|_| "reentrant run")?
                .remove(&id)
                .ok_or("unknown host or wrong thread")
        })
        .map_err(str::to_owned)?;
    plico_desktop::run_source_with_options(host.source, options).map_err(|e| e.to_string())
}

fn invalidate(id: u64) -> i32 {
    status(|| {
        with_host(id, |h| {
            h.source.source.view.dirty = true;
            h.source.source.view.error = None;
            h.source.source.request_rebuild();
            Ok(())
        })
    })
}
fn test_frame(id: u64, w: u32, h: u32) -> i32 {
    status(|| {
        dimensions(w, h)?;
        with_host(id, |host| {
            host.source.poll_async();
            host.source.build_frame(&mut host.text, w, h);
            view_error(host)
        })
    })
}
fn test_click(id: u64, role: *const u8, rl: u32, label: *const u8, ll: u32) -> i32 {
    status(|| {
        let command = plico_runtime::testing::Command::Click {
            role: utf8(role, rl)?.into(),
            label: utf8(label, ll)?.into(),
            occurrence: None,
        };
        with_host(id, |host| {
            plico_runtime::testing::dispatch(&mut host.source.source, &mut host.text, &command)
                .map_err(|e| {
                    let tree = host
                        .source
                        .source
                        .adapter
                        .semantics(&host.source.source.engine, 0);
                    let labels: Vec<_> = tree
                        .exposed_nodes()
                        .into_iter()
                        .filter_map(|node| node.label.as_deref())
                        .take(80)
                        .collect();
                    format!("{e}; available labels: {labels:?}")
                })
        })
    })
}
fn stats(id: u64, out: *mut Stats) -> i32 {
    status(|| {
        if out.is_null() || !(out as usize).is_multiple_of(std::mem::align_of::<Stats>()) {
            return Err("null/misaligned stats pointer".into());
        }
        with_host(id, |h| {
            unsafe { out.write(h.source.source.view.stats) };
            Ok(())
        })
    })
}
fn last_error(dst: *mut u8, capacity: u32) -> u32 {
    ERROR.with(|e| {
        let e = e.borrow();
        if !dst.is_null() {
            let n = e.len().min(capacity as usize);
            unsafe { std::ptr::copy_nonoverlapping(e.as_ptr(), dst, n) };
        }
        e.len() as u32
    })
}

fn test_text(id: u64, label: *const u8, ll: u32, text: *const u8, tl: u32) -> i32 {
    status(|| {
        let label = utf8(label, ll)?.to_owned();
        let value = utf8(text, tl)?.to_owned();
        with_host(id, |host| {
            use plico_runtime::testing::{Command, dispatch};
            dispatch(
                &mut host.source.source,
                &mut host.text,
                &Command::Focus {
                    role: "textbox".into(),
                    label,
                    occurrence: None,
                },
            )
            .map_err(|e| e.to_string())?;
            let event = plico_renderer::KeyEvent {
                phase: plico_renderer::KeyPhase::Down,
                key: "a".into(),
                code: "KeyA".into(),
                key_without_modifiers: "a".into(),
                text: None,
                text_with_all_modifiers: None,
                location: plico_renderer::KeyLocation::Standard,
                modifiers: if cfg!(target_os = "macos") {
                    plico_renderer::Modifiers::META
                } else {
                    plico_renderer::Modifiers::CONTROL
                },
                repeat: false,
                synthetic: false,
            };
            dispatch(
                &mut host.source.source,
                &mut host.text,
                &Command::KeyEvent { event },
            )
            .map_err(|e| e.to_string())?;
            dispatch(
                &mut host.source.source,
                &mut host.text,
                &Command::Text { text: value },
            )
            .map_err(|e| e.to_string())
        })
    })
}
fn test_value(id: u64, label: *const u8, ll: u32, value: f64) -> i32 {
    status(|| {
        let command = plico_runtime::testing::Command::SetValue {
            label: utf8(label, ll)?.into(),
            value,
        };
        with_host(id, |host| {
            plico_runtime::testing::dispatch(&mut host.source.source, &mut host.text, &command)
                .map_err(|e| {
                    let tree = host
                        .source
                        .source
                        .adapter
                        .semantics(&host.source.source.engine, 0);
                    let labels: Vec<_> = tree
                        .exposed_nodes()
                        .into_iter()
                        .filter_map(|node| node.label.as_deref())
                        .take(80)
                        .collect();
                    format!("{e}; available labels: {labels:?}")
                })
        })
    })
}

// Safe from any thread, including while run owns the host outside HOSTS.
fn wake(id: u64) -> i32 {
    status(|| {
        let mailbox = mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or("host is closed")?;
        mailbox.pending.store(true, Ordering::Release);
        let callback = mailbox
            .wake
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(callback) = callback {
            callback();
        }
        Ok(())
    })
}

/// Native service integration: transfers a shared decoded raster without encoding
/// or copying pixels. The returned resource can be used by ImageCanvas only.
pub fn register_canvas_image(host: u64, raster: Arc<RasterImage>) -> Result<u64, String> {
    register_resource(
        host,
        ImageResource {
            asset: None,
            raster,
        },
    )
}
/// Registers encoded PNG/JPEG/WebP data for ordinary image declarations.
pub fn register_image_bytes(host: u64, bytes: &[u8]) -> Result<u64, String> {
    let image = ::image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    if u64::from(image.width()) * u64::from(image.height()) > 32 * 1024 * 1024 {
        return Err("image exceeds 32 megapixels".into());
    }
    register_resource(
        host,
        ImageResource {
            asset: Some(ImageAsset::from_bytes(bytes)),
            raster: Arc::new(RasterImage::from_rgba(image.to_rgba8())),
        },
    )
}
fn register_resource(host: u64, image: ImageResource) -> Result<u64, String> {
    let mailbox = mailboxes()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&host)
        .cloned()
        .ok_or("host is closed")?;
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    mailbox
        .images
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, image);
    if wake(host) != 1 {
        return Err("host closed while registering image".into());
    }
    Ok(key)
}

impl ManagedView {
    fn collect_images(&mut self) {
        let mut released = self
            .mailbox
            .released
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        released.retain(|id| {
            if self.batch.as_ref().is_some_and(|batch| {
                batch
                    .commands
                    .iter()
                    .any(|c| matches!(c.kind, IMAGE | CANVAS) && c.resource == *id)
            }) {
                true
            } else {
                self.images.remove(id);
                self.mailbox
                    .images
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(id);
                false
            }
        });
    }
}
fn release_image(host: u64, resource: u64) -> i32 {
    status(|| {
        let mailbox = mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&host)
            .cloned()
            .ok_or("host is closed")?;
        {
            let mut released = mailbox.released.lock().unwrap_or_else(|e| e.into_inner());
            if released.len() >= 100_000 {
                return Err("image release queue exceeds budget".into());
            }
            released.insert(resource);
        }
        if wake(host) != 1 {
            return Err("host is closed".into());
        }
        Ok(())
    })
}

fn validate_shader(source: *const u8, len: u32, flags: u32) -> i32 {
    status(|| {
        if len > 65536 || flags > 3 || flags == 3 {
            return Err("invalid WGSL source length or mode flags".into());
        }
        let source = utf8(source, len)?;
        if flags & 1 != 0 {
            plico_renderer::ShaderEffectSource::new_module(source).map(|_| ())
        } else {
            plico_renderer::ShaderEffectSource::with_backdrop(source, flags & 2 != 0).map(|_| ())
        }
    })
}

#[cfg(test)]
mod dispatcher_tests {
    use super::*;
    unsafe extern "C" fn unused(_: *mut c_void, _: *const Frame, _: *mut Batch) -> i32 {
        -1
    }
    #[test]
    fn image_release_waits_for_batch_then_drops_pending_or_adopted_raster() {
        let host_id = create(Some(unused), std::ptr::null_mut());
        for adopted in [false, true] {
            let raster = Arc::new(RasterImage::from_rgba(::image::RgbaImage::from_pixel(
                2,
                2,
                ::image::Rgba([255; 4]),
            )));
            let weak = Arc::downgrade(&raster);
            let resource = register_canvas_image(host_id, raster).unwrap();
            with_host(host_id, |host| {
                let view = &mut host.source.source.view;
                if adopted {
                    view.images
                        .extend(view.mailbox.images.lock().unwrap().drain());
                }
                view.batch = Some(OwnedBatch::validate(
                    vec![abi::Command {
                        id: 1,
                        parent: ROOT,
                        kind: IMAGE,
                        resource,
                        ..Default::default()
                    }],
                    Vec::new(),
                )?);
                Ok(())
            })
            .unwrap();
            assert_eq!(release_image(host_id, resource), 1);
            with_host(host_id, |host| {
                let view = &mut host.source.source.view;
                view.collect_images();
                assert!(weak.upgrade().is_some(), "submitted batch lost its image");
                view.batch = None;
                view.collect_images();
                assert!(weak.upgrade().is_none(), "unreferenced image was retained");
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(destroy(host_id), 1);
    }
    #[test]
    fn worker_wake_reaches_ui_and_closing_unregisters_mailbox() {
        let id = create(Some(unused), std::ptr::null_mut());
        assert_ne!(id, 0);
        let notifications = Arc::new(AtomicU64::new(0));
        with_host(id, |host| {
            let notifications = notifications.clone();
            host.source.source.view.set_wake_callback(Arc::new(move || {
                notifications.fetch_add(1, Ordering::Relaxed);
            }));
            Ok(())
        })
        .unwrap();
        assert_eq!(std::thread::spawn(move || wake(id)).join().unwrap(), 1);
        assert_eq!(notifications.load(Ordering::Relaxed), 1);
        with_host(id, |host| {
            assert!(host.source.source.view.poll_updates());
            assert!(!host.source.source.view.poll_updates());
            Ok(())
        })
        .unwrap();
        assert_eq!(destroy(id), 1);
        assert_eq!(std::thread::spawn(move || wake(id)).join().unwrap(), 0);
    }
}

#[cfg(test)]
mod primitive_tests {
    #[test]
    fn percentage_width_is_not_treated_as_pixels() {
        let style = super::primitive_style(super::abi::Command {
            width: 42.,
            flags: 1 << 16,
            ..Default::default()
        });
        assert_eq!(style.width, None);
        assert_eq!(style.width_percent, Some(42.));
    }
}
