//! The desktop host's persisted-state file survives a full app lifecycle,
//! exactly as MyGO's WindowStateStore does for window geometry: one file
//! under the app data directory, restored before the first frame, saved
//! atomically at close and quit.
#![cfg(all(not(target_arch = "wasm32"), any(feature = "skia", feature = "wgpu")))]
use plico::Engine;
use plico::prelude::*;
use plico_renderer::FrameSource;
use plico_runtime::HostView;

#[test]
fn persisted_bytes_survive_a_dump_load_cycle_across_engines() {
    // The engine-level contract the host builds on: bytes from one engine
    // restore into a fresh one, and damaged bytes keep the defaults.
    let mut first = Engine::default();
    first
        .frame_declarations(|cx| {
            let state = cx.persisted_state("remembered", || 0u64);
            state.set(41);
        })
        .unwrap();
    let bytes = first.dump_persisted().unwrap();

    let mut second = Engine::default();
    second.load_persisted(&bytes).unwrap();
    second
        .frame_declarations(|cx| {
            let state = cx.persisted_state("remembered", || 0u64);
            assert_eq!(*state.read(), 41);
        })
        .unwrap();

    // Damaged input is atomic: live storage keeps its own state.
    let mut third = Engine::default();
    third
        .frame_declarations(|cx| {
            let state = cx.persisted_state("remembered", || 7u64);
            state.set(9);
        })
        .unwrap();
    assert!(third.load_persisted(b"not msgpack").is_err());
    third
        .frame_declarations(|cx| {
            let state = cx.persisted_state("remembered", || 0u64);
            assert_eq!(*state.read(), 9, "a failed restore keeps live state");
        })
        .unwrap();
}

/// The full host contract through the runtime source: bytes a source dumps
/// restore into a fresh source, and the view declares the restored value.
#[test]
fn runtime_source_bytes_restore_into_a_fresh_process() {
    #[derive(Clone, Copy)]
    struct Args {
        value: u64,
        write: bool,
    }
    impl HostView for Args {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            let state = cx.persisted_state("cross-process", || 0u64);
            let mut current = *state.read();
            if self.write && current != self.value {
                state.set(self.value);
                current = self.value;
            }
            text!(cx, "value {current}");
        }
        fn theme(&self) -> Theme {
            Theme::light()
        }
    }

    let mut tcx = plico_renderer::TextContext::new();
    // Process one: the view writes 1234 through persisted_state.
    let mut writer = plico_runtime::Source::new(Args {
        value: 1234,
        write: true,
    });
    writer.build_frame(&mut tcx, 400, 300);
    let bytes = writer.persisted_snapshot().expect("source has state");
    assert!(!bytes.is_empty());

    // Process two: a fresh source restores the bytes and reads them back.
    let mut reader = plico_runtime::Source::new(Args {
        value: 0,
        write: false,
    });
    reader.restore_persisted(&bytes);
    reader.build_frame(&mut tcx, 400, 300);
    let text = reader
        .placed_nodes()
        .iter()
        .find_map(|node| node.paint.text.as_deref())
        .expect("a declared text node");
    assert_eq!(text, "value 1234");
}
