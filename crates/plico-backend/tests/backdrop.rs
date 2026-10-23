use plico_backend::{Source, View};
use plico_core::prelude::*;
use plico_renderer::{FrameSource, PaintCommand, Scene, TextContext};
struct Demo {
    blur: f32,
    shift: f32,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .width(200.)
            .height(140.)
            .background(Color::Literal(0xee2222))
            .children(|cx| {
                column(cx)
                    .absolute(100. + self.shift, 0.)
                    .width(100.)
                    .height(140.)
                    .background(Color::Literal(0x2244ee))
                    .children(|_cx| {});
                column(cx)
                    .absolute(40., 20.)
                    .width(120.)
                    .height(100.)
                    .radius(20.)
                    .backdrop_blur(self.blur)
                    .background(Color::Surface)
                    .background_alpha(0.)
                    .children(|_cx| {});
                column(cx)
                    .absolute(96., 60.)
                    .width(8.)
                    .height(8.)
                    .background(Color::Literal(0x00ee00))
                    .children(|_cx| {})
            })
    }
}
fn scene(source: &mut Source<Demo>, tcx: &mut TextContext, scale: f64) -> Scene {
    let nodes = source.build_frame(tcx, 200, 140);
    assert!(source.last_error.is_none());
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, tcx, (200, 140), scale);
    scene
}
#[test]
fn blur_filters_only_prior_pixels_and_respects_panel_clip_at_two_scales() {
    for scale in [1., 2.] {
        let mut source = Source::new(Demo {
            blur: 10.,
            shift: 0.,
        });
        let mut tcx = TextContext::new();
        let blurred = scene(&mut source, &mut tcx, scale);
        assert!(blurred.commands.iter().any(|c| matches!(
            c,
            PaintCommand::PushLayer {
                filter: Some(_),
                ..
            }
        )));
        source.view.blur = 0.;
        let sharp = scene(&mut source, &mut tcx, scale);
        assert_eq!(source.adapter.stats.layout_runs, 0);
        let w = (200. * scale) as u32;
        let h = (140. * scale) as u32;
        let blurred =
            plico_shell::renderer::render_to_image(&blurred, w, h, source.base_color()).unwrap();
        let sharp =
            plico_shell::renderer::render_to_image(&sharp, w, h, source.base_color()).unwrap();
        let pixel = |x: f64, y: f64| ((x * scale) as u32, (y * scale) as u32);
        assert_eq!(
            blurred.get_pixel(pixel(95., 10.).0, pixel(95., 10.).1).0,
            sharp.get_pixel(pixel(95., 10.).0, pixel(95., 10.).1).0,
            "outside panel must stay sharp"
        );
        assert_eq!(
            blurred.get_pixel(pixel(41., 21.).0, pixel(41., 21.).1).0,
            sharp.get_pixel(pixel(41., 21.).0, pixel(41., 21.).1).0,
            "rounded corner must not filter"
        );
        let inside = blurred.get_pixel(pixel(95., 45.).0, pixel(95., 45.).1).0;
        let original = sharp.get_pixel(pixel(95., 45.).0, pixel(95., 45.).1).0;
        assert!(
            inside[2] > original[2] + 15,
            "expected actual red/blue mixing: {inside:?} vs {original:?}"
        );
        assert_eq!(
            blurred.get_pixel(pixel(99., 63.).0, pixel(99., 63.).1).0,
            sharp.get_pixel(pixel(99., 63.).0, pixel(99., 63.).1).0,
            "foreground must stay sharp"
        );
    }
}
#[test]
fn idle_scene_is_reused_and_moving_background_invalidates_it() {
    let mut source = Source::new(Demo {
        blur: 10.,
        shift: 0.,
    });
    let mut tcx = TextContext::new();
    let a = scene(&mut source, &mut tcx, 1.);
    let b = scene(&mut source, &mut tcx, 1.);
    assert_eq!(a.commands.as_ptr(), b.commands.as_ptr());
    assert_eq!(source.adapter.stats.paint_writes, 0);
    source.view.shift = 20.;
    let c = scene(&mut source, &mut tcx, 1.);
    assert_ne!(b.commands.as_ptr(), c.commands.as_ptr());
    let a = plico_shell::renderer::render_to_image(&b, 200, 140, source.base_color()).unwrap();
    let c = plico_shell::renderer::render_to_image(&c, 200, 140, source.base_color()).unwrap();
    assert_ne!(a.get_pixel(105, 45), c.get_pixel(105, 45));
}
