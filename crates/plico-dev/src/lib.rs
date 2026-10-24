//! Versioned JSON/C boundary with MessagePack state; no Rust ABI crosses it.
use plico_core::{Engine, PathKey, Theme, Ui, ViewNode};
use serde::{Serialize, de::DeserializeOwned};
use snafu::{ResultExt, Snafu};
use zeroize::Zeroizing;
pub mod services;
mod validation;
pub use validation::validate_nodes;
pub const PROTOCOL_VERSION: u32 = 3;
mod snapshot;
pub use snapshot::StateSnapshot;
#[cfg(not(target_arch = "wasm32"))]
pub use stabby;
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("invalid application tree: {message}"))]
    Tree { message: String },
    #[snafu(display("invalid application data: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("application checkpoint encoding failed: {source}"))]
    Checkpoint { source: rmp_serde::encode::Error },
    #[snafu(display("unsupported application protocol {actual}"))]
    Protocol { actual: u32 },
    #[snafu(display("view declaration failed: {source}"))]
    Frame { source: plico_core::FrameError },
    #[snafu(display("application did not settle after eight passes"))]
    DidNotSettle,
    #[snafu(display("application panicked during declaration: {message}"))]
    Panic { message: String },
    #[snafu(display("application module failed: {message}"))]
    Module { message: String },
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub version: u32,
    pub state: StateSnapshot,
    #[serde(default)]
    pub services: serde_json::Value,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub viewport: Option<[f32; 2]>,
    #[serde(default)]
    pub clicks: Vec<Vec<PathKey>>,
    #[serde(default)]
    pub values: Vec<(Vec<PathKey>, f64)>,
    #[serde(default)]
    pub time: plico_core::FrameTime,
    #[serde(default)]
    pub scroll_viewports: Vec<(Vec<PathKey>, [f32; 2])>,
    /// The module retains its application instance: skip the per-frame state
    /// serialize/deserialize roundtrip. Full frames (events, activation) still
    /// exchange state for crash-safe recovery and reload migration.
    #[serde(default)]
    pub lean: bool,
    /// Continue this module's UI-thread session; state is only a recovery checkpoint.
    #[serde(default)]
    pub resume: bool,
    /// Read the current checkpoint without running application code.
    #[serde(default)]
    pub checkpoint: bool,
    /// Submission counts already committed before restoring input buffers.
    #[serde(default)]
    pub text_baselines: Vec<(Vec<PathKey>, u64)>,
    /// Host-edited native input content by logical path: text and submit
    /// counter. Applied to the module-side application buffers before
    /// declarations run.
    #[serde(default)]
    pub texts: Vec<(Vec<PathKey>, String, u64)>,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            version: PROTOCOL_VERSION,
            state: StateSnapshot::default(),
            services: serde_json::Value::Null,
            clicks: Vec::new(),
            keys: Vec::new(),
            viewport: None,
            values: Vec::new(),
            time: Default::default(),
            scroll_viewports: Vec::new(),
            lean: false,
            resume: false,
            checkpoint: false,
            text_baselines: Vec::new(),
            texts: Vec::new(),
        }
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub version: u32,
    pub state: StateSnapshot,
    pub theme: Theme,
    pub nodes: Vec<ViewNode>,
    pub reset: Option<String>,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
    #[serde(default)]
    pub key_handled: bool,
    #[serde(default)]
    pub requests: plico_core::Changes,
    /// Path-addressed input events no declaration consumed, so drift after an
    /// edit is visible to the host instead of silently dropping input.
    #[serde(default)]
    pub unmatched_events: usize,
    /// Native input state for host display: logical path, buffer revision and
    /// content. Secret buffers withhold content (None).
    #[serde(default)]
    pub texts: Vec<(Vec<PathKey>, Option<String>, u64)>,
    #[serde(default)]
    pub text_hints: Vec<(Vec<PathKey>, String)>,
}
/// Local adapter: an `App` driven by the engine's settle loop.
struct AppView<'a, A: App>(&'a mut A);
impl<A: App> plico_core::DeclareView for AppView<'_, A> {
    fn theme(&self) -> plico_core::Theme {
        App::theme(self.0)
    }
    fn show(&mut self, ui: &mut plico_core::Ui<'_>) {
        App::show(self.0, ui)
    }
}
pub trait App: Default + Serialize + DeserializeOwned {
    #[doc(hidden)]
    fn set_services(&mut self, _snapshot: serde_json::Value) -> Result<()> {
        Ok(())
    }
    #[doc(hidden)]
    fn take_service_commands(&mut self) -> Result<Vec<serde_json::Value>> {
        Ok(Vec::new())
    }
    fn set_frame_time(&mut self, _time: plico_core::FrameTime) {}
    /// Unmodified window-local fallback keys, after controls/IME decline them.
    fn on_key(&mut self, _key: &str) -> bool {
        false
    }
    fn show(&mut self, ui: &mut Ui<'_>);
    fn theme(&self) -> Theme {
        Theme::default()
    }
}
struct Session<A> {
    app: A,
    engine: Engine,
}

pub fn render<A: App + 'static>(request: Request) -> Result<Frame> {
    snafu::ensure!(
        request.version == PROTOCOL_VERSION,
        ProtocolSnafu {
            actual: request.version
        }
    );
    // One application session per loaded image/UI thread. No Rust object crosses the ABI.
    thread_local! {
        static RETAINED: std::cell::RefCell<Option<Box<dyn std::any::Any>>> =
            std::cell::RefCell::new(None);
    }
    let retained = RETAINED.with(std::cell::RefCell::take);
    let continuing = request.resume || request.lean || request.checkpoint;
    let retained = if continuing {
        retained.and_then(|session| session.downcast::<Session<A>>().ok())
    } else {
        None
    };
    let fresh = retained.is_none();
    let (mut session, reset) = if let Some(session) = retained {
        (session, None)
    } else {
        snafu::ensure!(
            !request.lean && !request.checkpoint,
            ModuleSnafu {
                message: "session request arrived without retained application state"
            }
        );
        let (app, reset) = if request.state.is_empty() {
            (A::default(), None)
        } else {
            match request.state.decode() {
                Ok(app) => (app, None),
                Err(error) => (A::default(), Some(error.to_string())),
            }
        };
        (
            Box::new(Session {
                app,
                engine: Engine::default(),
            }),
            reset,
        )
    };
    let mut key_handled = false;
    if !request.checkpoint {
        let Session { app, engine } = session.as_mut();
        app.set_services(request.services)?;
        app.set_frame_time(request.time);
        for key in &request.keys {
            key_handled |= app.on_key(key);
        }
        if let Some([width, height]) = request.viewport {
            engine.set_viewport(width, height).context(FrameSnafu)?;
        }
        engine.set_time(request.time);
        engine.set_scroll_viewport_paths(request.scroll_viewports);
        // Only a fresh session needs a registration pass. Queue clicks afterwards:
        // controls must never act on pre-edit text in that pass.
        if fresh && !request.texts.is_empty() {
            engine.set_theme(app.theme());
            engine
                .frame_declarations(|ui| app.show(ui))
                .context(FrameSnafu)?;
        }
        for (path, count) in request.text_baselines {
            if fresh {
                engine.restore_input_submit(&path, count);
            }
        }
        for (path, content, submitted) in request.texts {
            if let Some((_, buffer)) = engine.input_bindings().find(|(p, _)| *p == path) {
                // Secret editors are explicitly unavailable in the dev transport.
                if buffer.is_secret() {
                    continue;
                }
                if buffer.text().as_deref() != Some(content.as_str()) {
                    buffer.set(Zeroizing::new(content));
                }
                // Native frames coalesce submission into one response, while advancing
                // the absolute counter in O(1), never looping on untrusted counts.
                buffer.restore_submit_count(submitted.max(buffer.submitted_count()));
            }
        }
        for (path, value) in request.values {
            engine.queue_value_path(path, value);
        }
        for path in request.clicks {
            engine.queue_click_path(path);
        }
        engine
            .declare_until_settled(&mut AppView(app))
            .map_err(|source| match source {
                plico_core::FrameError::DidNotSettle => Error::DidNotSettle,
                source => Error::Frame { source },
            })?;
    }
    let Session { app, engine } = session.as_mut();
    let commands = if request.checkpoint {
        Vec::new()
    } else {
        app.take_service_commands()?
    };
    let unmatched = engine.take_unmatched_paths();
    let texts = engine
        .input_bindings()
        .map(|(path, buffer)| (path, buffer.text(), buffer.revision()))
        .collect();
    let text_hints = engine
        .input_bindings()
        .filter_map(|(path, _)| {
            engine
                .input_hint_path(&path)
                .map(|hint| (path, hint.to_string()))
        })
        .collect();
    let state = if request.lean {
        StateSnapshot::default()
    } else {
        StateSnapshot::encode(app).context(CheckpointSnafu)?
    };
    let frame = Frame {
        version: PROTOCOL_VERSION,
        state,
        theme: app.theme(),
        nodes: engine.snapshot(),
        reset,
        commands,
        key_handled,
        requests: engine.changes() & plico_core::Changes::ANIMATE,
        unmatched_events: unmatched.clicks.len() + unmatched.values.len(),
        texts,
        text_hints,
    };
    // This consumer sends complete snapshots, not a retained backend journal.
    // Keep a persistent module Engine from accumulating every unmounted virtual row.
    drop(engine.drain_removed());
    // Error/panic paths drop the entire in-progress session. Only successful work is retained.
    RETAINED.with(|slot| *slot.borrow_mut() = Some(session));
    Ok(frame)
}
/// Only this small transport type has a stable native layout. The JSON envelope carries opaque MessagePack state bytes.
#[cfg(not(target_arch = "wasm32"))]
#[stabby::stabby]
pub struct Reply {
    pub status: u32,
    pub bytes: stabby::vec::Vec<u8>,
}
#[cfg(not(target_arch = "wasm32"))]
pub type RenderFn = extern "C" fn(stabby::vec::Vec<u8>) -> Reply;
/// Reactive modules register a checked C callback carrying an opaque host token.
#[cfg(not(target_arch = "wasm32"))]
pub type WakeFn = extern "C" fn(u64);
#[cfg(not(target_arch = "wasm32"))]
pub type SetWakeFn = extern "C" fn(u64, WakeFn) -> u32;
/// A panic may not unwind through the exported function boundary.
#[cfg(not(target_arch = "wasm32"))]
pub fn dispatch<A: App + 'static>(input: stabby::vec::Vec<u8>) -> Reply {
    dispatch_with(input, render::<A>)
}
#[cfg(not(target_arch = "wasm32"))]
fn dispatch_with(input: stabby::vec::Vec<u8>, render: fn(Request) -> Result<Frame>) -> Reply {
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let request = serde_json::from_slice(&input).context(JsonSnafu)?;
        render(request)
    })) {
        Ok(result) => result,
        Err(payload) => Err(Error::Panic {
            message: panic_message(payload.as_ref()),
        }),
    };
    let (status, bytes) = match result {
        Ok(frame) => match serde_json::to_vec(&frame) {
            Ok(bytes) => (0, bytes),
            Err(e) => (1, e.to_string().into_bytes()),
        },
        Err(error) => (1, error.to_string().into_bytes()),
    };
    Reply {
        status,
        bytes: bytes.into_iter().collect(),
    }
}
/// Applications also declare a direct stabby dependency for its export macro's ABI reports.
#[macro_export]
macro_rules! export_app {
    ($app:ty) => {
        #[$crate::stabby::export]
        pub extern "C" fn plico_app_render(input: $crate::stabby::vec::Vec<u8>) -> $crate::Reply {
            $crate::dispatch::<$app>(input)
        }
    };
}
/// Panic payload as a reportable message; keeps the developer-facing reason a
/// module failed instead of discarding it at the boundary.
#[cfg(not(target_arch = "wasm32"))]
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".into()
    }
}
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub mod host;
