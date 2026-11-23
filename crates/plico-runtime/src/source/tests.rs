use super::*;

#[test]
fn checked_frames_return_declaration_errors_without_side_channels() {
    struct DuplicateKeys;

    impl HostView for DuplicateKeys {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            ui.text("First").key("duplicate");
            ui.text("Second").key("duplicate");
        }
    }

    let mut source = Source::new(DuplicateKeys);
    let mut text = TextContext::new();
    assert!(source.try_build_frame(&mut text, 320, 200).is_err());

    source.request_rebuild();
    let error = source.try_prepare_for_event(&mut text).unwrap_err();
    assert!(error.to_string().contains("duplicate UI key"));

    let mut scene = Scene::new();
    let error =
        FrameSource::render_retained_frame(&mut source, &mut scene, &mut text, (320, 200), 1.)
            .unwrap_err();
    assert!(error.to_string().contains("duplicate UI key"));
}

#[test]
fn public_frame_stats_correlate_runtime_work_and_clear_it_on_retained_frames() {
    struct Label;

    impl HostView for Label {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            ui.text("Frame diagnostics");
        }
    }

    let mut source = Source::new(Label);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .try_render_retained_frame(&mut scene, &mut text, (320, 200), 1.0)
        .unwrap();

    let mut stats = plico_renderer::FrameStats::default();
    source.augment_frame_stats(&mut stats);
    assert!(stats.declaration_passes > 0);
    assert!(stats.layout_runs > 0);
    assert!(stats.flatten_visits > 0);
    assert_eq!(stats.scene_commands, scene.commands.len() as u64);
    assert!(!stats.scene_reused);

    source
        .try_render_retained_frame(&mut scene, &mut text, (320, 200), 1.0)
        .unwrap();
    source.augment_frame_stats(&mut stats);
    assert_eq!(stats.sample_declaration_ms, 0.0);
    assert_eq!(stats.sample_sync_ms, 0.0);
    assert_eq!(stats.sample_backend_ms, 0.0);
    assert_eq!(stats.declaration_passes, 0);
    assert_eq!(stats.layout_runs, 0);
    assert_eq!(stats.flatten_visits, 0);
    assert_eq!(stats.scene_commands, scene.commands.len() as u64);
    assert!(stats.scene_reused);
}
