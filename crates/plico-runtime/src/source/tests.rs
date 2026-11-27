use super::*;
use plico::widgets::{column, text};

#[test]
fn retained_scene_rerasterizes_at_exact_scale_when_motion_stops() {
    use plico_renderer::paragraph::{
        NativeParagraph, ParagraphEngine, ParagraphError, ParagraphMetrics, ParagraphRequest,
    };
    use std::rc::Rc;
    struct NativeText;
    impl NativeParagraph for NativeText {
        fn metrics(&self) -> ParagraphMetrics {
            ParagraphMetrics {
                width: 80.,
                height: 20.,
                ..Default::default()
            }
        }
        fn hit_test(&self, _: f32, _: f32) -> usize {
            0
        }
        fn selection_rects(&self, _: std::ops::Range<usize>) -> Vec<[f32; 4]> {
            Vec::new()
        }
    }
    impl ParagraphEngine for NativeText {
        fn name(&self) -> &'static str {
            "Test text"
        }
        fn layout(
            &mut self,
            _: Arc<ParagraphRequest>,
        ) -> Result<Rc<dyn NativeParagraph>, ParagraphError> {
            Ok(Rc::new(NativeText))
        }
        fn register_font(&mut self, _: &FontAsset) {}
        fn clear(&mut self) {}
    }
    struct Scaled(f32);
    impl HostView for Scaled {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            column(ui).scale(self.0, self.0).children(|ui| {
                text(ui, "Scale watch");
            });
        }
    }
    let mut source = Source::new(Scaled(1.));
    let mut text = TextContext::new();
    text.set_paragraph_engine(Box::new(NativeText));
    let mut scene = Scene::new();
    let mut paint = |source: &mut Source<Scaled>| {
        source
            .render_frame(&mut scene, &mut text, (320, 200), 1.)
            .unwrap();
        scene
            .commands
            .iter()
            .find_map(|command| match command {
                plico_renderer::PaintCommand::Paragraph { request, transform } => {
                    Some((request.raster_scale_hint, *transform))
                }
                _ => None,
            })
            .unwrap()
    };
    assert!(paint(&mut source).0.is_none());
    source.update(|view| view.0 = 1.14);
    let (motion, transform) = paint(&mut source);
    assert!(motion.unwrap() >= 1.14);
    assert!(source.has_anim());
    let (rest, settled_transform) = paint(&mut source);
    assert_eq!(rest, None);
    assert_eq!(transform, settled_transform);
    assert!(!source.has_anim());

    let mut parts = plico_renderer::scene::SceneParts::default();
    let mut scratch = plico_renderer::scene::SceneScratch::default();
    let mut paint_fragments = |source: &mut Source<Scaled>, dirty| {
        source.prepare_layout(&mut text, (320, 200)).unwrap();
        plico_renderer::scene::patch_presented_scene(
            &mut scene,
            &source.adapter.placed,
            &[],
            &mut text,
            (320, 200),
            peniko::Color::TRANSPARENT,
            1.,
            &mut parts,
            &mut scratch,
            dirty,
            &[],
        );
        let bucketed = scene.flattened().commands.iter().any(|command| {
            matches!(
                command,
                plico_renderer::PaintCommand::Paragraph { request, .. }
                    if request.raster_scale_hint.is_some()
            )
        });
        (bucketed, scratch.text_raster_needs_frame())
    };
    assert_eq!(paint_fragments(&mut source, None), (false, false));
    source.update(|view| view.0 = 1.22);
    assert_eq!(paint_fragments(&mut source, None), (true, true));
    let unchanged = HashSet::new();
    assert_eq!(
        paint_fragments(&mut source, Some(&unchanged)),
        (false, false)
    );
}

#[test]
fn checked_frames_return_declaration_errors_without_side_channels() {
    struct DuplicateKeys;

    impl HostView for DuplicateKeys {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            text(ui, "First").key("duplicate");
            text(ui, "Second").key("duplicate");
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
        FrameSource::render_frame(&mut source, &mut scene, &mut text, (320, 200), 1.).unwrap_err();
    assert!(error.to_string().contains("duplicate UI key"));
}

#[test]
fn public_frame_stats_correlate_runtime_work_and_clear_it_on_retained_frames() {
    struct Label;

    impl HostView for Label {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            text(ui, "Frame diagnostics");
        }
    }

    let mut source = Source::new(Label);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_frame(&mut scene, &mut text, (320, 200), 1.0)
        .unwrap();

    let mut stats = plico_renderer::FrameStats::default();
    source.augment_frame_stats(&mut stats);
    assert!(stats.declaration_passes > 0);
    assert!(stats.layout_runs > 0);
    assert!(stats.flatten_visits > 0);
    assert_eq!(stats.scene_commands, scene.commands.len() as u64);
    assert!(!stats.scene_reused);

    source
        .render_frame(&mut scene, &mut text, (320, 200), 1.0)
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

#[test]
fn measured_layout_schedules_one_followup_frame() {
    #[derive(Default)]
    struct Measured(Option<[f32; 2]>);
    impl HostView for Measured {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            column(ui).width(80.).height(40.).children(|ui| {
                self.0 = ui.layout_size();
            });
        }
    }
    let mut source = Source::new(Measured::default());
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_frame(&mut scene, &mut text, (320, 200), 1.)
        .unwrap();
    assert!(source.has_anim());
    source
        .render_frame(&mut scene, &mut text, (320, 200), 1.)
        .unwrap();
    assert_eq!(source.view().0, Some([80., 40.]));
    assert!(!source.has_anim());
}
