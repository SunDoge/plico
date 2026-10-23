//! Verify a compiled gallery module through its real ABI and ReloadView.
use plico_backend::{Source, reload::ReloadView};
use plico_core::{Clock, VirtualClock};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
use std::{path::PathBuf, time::Duration};
#[derive(usage_rs::Cli)]
#[usage(
    bin = "2048-reload-check",
    about = "Verify gallery keyboard input and state across module activation"
)]
struct Options {
    #[usage(long)]
    module: PathBuf,
}
fn key(value: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: value.into(),
        key_without_modifiers: value.into(),
        code: value.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse();
    let directory = std::env::temp_dir().join(format!("plico-2048-check-{}", std::process::id()));
    std::fs::create_dir(&directory)?;
    // Only trusted application code is loaded. The path is explicit, never downloaded.
    let library = std::fs::canonicalize(options.module)?;
    let descriptor = directory.join("app.json");
    std::fs::write(
        &descriptor,
        serde_json::to_vec(&plico_dev::host::Descriptor {
            version: plico_dev::PROTOCOL_VERSION,
            library: library.clone(),
        })?,
    )?;
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(ReloadView::new(descriptor.clone())?, clock.clone());
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 640, 740);
    let nav = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "2048", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: nav }));
    source.build_frame(&mut tcx, 640, 740);
    assert_eq!(source.view.state()["page"], "Game2048");
    let before = source.view.state()["game2048"]["tiles"].clone();
    for value in ["ArrowLeft", "ArrowDown", "ArrowRight"] {
        assert!(source.handle_event(key(value)).handled);
        source.build_frame(&mut tcx, 640, 740);
        clock.advance(Duration::from_millis(150))?;
    }
    clock.advance(Duration::from_millis(600))?;
    source.build_frame(&mut tcx, 640, 740);
    assert_ne!(source.view.state()["game2048"]["tiles"], before);
    assert!(!source.has_anim());
    let state = source.view.state().clone();
    let revision = directory.join(format!("revision.{}", std::env::consts::DLL_EXTENSION));
    std::fs::copy(library, &revision)?;
    std::fs::write(
        &descriptor,
        serde_json::to_vec(&plico_dev::host::Descriptor {
            version: plico_dev::PROTOCOL_VERSION,
            library: revision,
        })?,
    )?;
    assert!(source.view.activate()?);
    source.build_frame(&mut tcx, 640, 740);
    assert_eq!(
        source.view.state()["game2048"]["tiles"],
        state["game2048"]["tiles"]
    );
    assert_eq!(
        source.view.state()["game2048"]["score"],
        state["game2048"]["score"]
    );
    assert!(source.last_error.is_none());
    assert!(source.view.last_error.is_none());
    println!(
        "PASS: real module keys, animation, 640px viewport and activation preserve board/score at {:?}",
        clock.now()
    );
    drop(source);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
