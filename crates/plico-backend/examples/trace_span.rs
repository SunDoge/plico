//! End-to-end check that profiling spans reach a Chrome/Perfetto trace file.
//!
//! Installs the shell's tracing subscriber exactly like the window entry
//! point, drives some frames through a source with shaping work, then drops
//! the guard — which is what flushes the trace JSON.
use plico_backend::{Source, View};
use plico_core::{UiContext, VirtualClock};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::Duration;
struct Demo {
    revision: u32,
}
impl View for Demo {
    fn show(&mut self, cx: &mut UiContext) {
        cx.column().children(|cx| {
            for i in 0..200 {
                cx.text(format!("Row {i} rev {}", self.revision))
                    .key(i as u64);
            }
        });
    }
}
fn main() {
    let path = std::env::var_os("PLICO_TRACE_FILE")
        .unwrap_or_else(|| "/tmp/plico-trace-verify.json".into());
    let _tracing = plico_shell::telemetry::init_tracing();
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Demo { revision: 0 }, clock.clone());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source.build_and_paint(&mut scene, &mut text, 800, 600, 1.);
    for frame in 0..8 {
        clock.advance(Duration::from_millis(16)).unwrap();
        source.view.revision = frame;
        source.request_rebuild();
        source.build_and_paint(&mut scene, &mut text, 800, 600, 1.);
    }
    drop(_tracing);
    let bytes = std::fs::read(&path).expect("trace file written");
    let content = String::from_utf8(bytes).unwrap();
    assert!(content.contains("text.shape.cache_miss"), "spans missing");
    assert!(content.trim_end().ends_with(']'), "trace not closed");
    println!("{} bytes of Chrome trace at {}", content.len(), path.display());
}
