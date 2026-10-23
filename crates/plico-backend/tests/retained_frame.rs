use plico_backend::Source;
use plico_gallery::{Gallery, Page};
use plico_renderer::{FrameSource, Scene, SemanticAction, SemanticRole, TextContext};
#[test]
fn retained_and_snapshot_paths_render_identical_pixels_for_widgets_shader_and_theme_retarget() {
    let clock = plico_core::VirtualClock::default();
    let mut text = TextContext::new();
    for page in [Page::Controls, Page::Motion] {
        let app = Gallery {
            router: plico_core::Router::new(page),
            ..Default::default()
        };
        let mut slow = Source::with_clock(app.clone(), clock.clone());
        let mut fast = Source::with_clock(app.clone(), clock.clone());
        let mut combined = Source::with_clock(app, clock.clone());
        for phase in 0..5 {
            if phase == 1 && page == Page::Controls {
                for s in [&mut slow, &mut fast, &mut combined] {
                    let target = s
                        .adapter
                        .semantics(&s.engine, 0)
                        .node_by_role(SemanticRole::Button, "Increment", None)
                        .unwrap()
                        .id;
                    s.handle_semantic_action(SemanticAction::Focus { target });
                }
            }
            if phase == 2 {
                slow.view.dark = true;
                fast.view.dark = true;
                combined.view.dark = true;
                slow.request_rebuild();
                fast.request_rebuild();
                combined.request_rebuild();
            }
            let nodes = slow.build_frame(&mut text, 640, 740);
            let mut left = Scene::new();
            slow.paint_scene(&mut left, &nodes, &mut text, (640, 740), 1.);
            slow.paint_debug_overlay(&mut left, &nodes, &mut text, 1.);
            let mut right = Scene::new();
            let count = fast
                .render_retained_frame(&mut right, &mut text, (640, 740), 1.)
                .unwrap()
                .0;

            assert_eq!(count, nodes.len());
            assert_eq!(fast.adapter.stats.placed_node_clones, 0);
            // The combined host path paints straight from retained state; it
            // must match the exported-node path pixel for pixel and never
            // materialize the placed list.
            let mut merged = Scene::new();
            let merged_count = combined
                .build_and_paint(&mut merged, &mut text, 640, 740, 1.)
                .0;
            assert_eq!(merged_count, nodes.len());
            assert_eq!(combined.adapter.stats.placed_node_clones, 0);
            let left =
                plico_shell::renderer::render_to_image(&left, 640, 740, slow.base_color()).unwrap();
            let right = plico_shell::renderer::render_to_image(&right, 640, 740, fast.base_color())
                .unwrap();
            let merged =
                plico_shell::renderer::render_to_image(&merged, 640, 740, combined.base_color())
                    .unwrap();
            if std::env::var("CMDDBG").is_ok() && left.as_raw() != right.as_raw() {
                let ls: Vec<String> = left.iter().take(0).map(|_| String::new()).collect();
                let _ = ls;
                // Compare scene command streams directly.
                eprintln!("CMDDBG left={} right={}", left.len(), right.len());
            }

            assert_eq!(left.as_raw(), right.as_raw(), "{page:?} phase {phase}");
            assert_eq!(left.as_raw(), merged.as_raw(), "{page:?} phase {phase}");
            clock
                .advance(std::time::Duration::from_millis(100))
                .unwrap();
        }
    }
}
