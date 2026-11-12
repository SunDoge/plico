#![cfg(feature = "wgpu")]
use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, TextContext, UiEvent, WheelDeltaMode,
    WheelEvent,
};
struct ScrollGlass {
    glass: bool,
}
impl HostView for ScrollGlass {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(200).height(160).background(Color::Literal(0xe2e8ec)) + |cx| {
            column(cx).absolute(24., 24.).width(120).height(100).clip(true).scroll_y(true).radius(16.)
                .corner_smoothing(1.).padding(8).gap(10).background(Color::Panel) + |cx|
            {
                for i in 0usize..8 {
                    cx.keyed(i, |cx| {
                        let box_ = column(cx).width(100).height(40).shrink(0.).radius(18.);
                        if self.glass {
                            box_.material(LiquidGlass::default()).elevation(Elevation::CONTROL).opacity(0.8)
                                .end();
                        } else {
                            box_.end();
                        }
                    });
                }
            };
        };
    }
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn scrolling_glass_and_shadows_cannot_escape_the_scrollport_at_multiple_scales() {
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    for scale in [1., 1.5, 2.] {
        let mut source =
            Source::with_clock(ScrollGlass { glass: false }, plico::VirtualClock::default());
        let mut text = TextContext::new();
        let size = [(200. * scale) as u32, (160. * scale) as u32];
        let mut scene = Scene::new();
        source
            .render_retained_frame(&mut scene, &mut text, (200, 160), scale)
            .unwrap();
        let baseline = renderer.render_scene(&scene, size, [0.; 4], false).unwrap();
        source.update(|view| view.glass = true);
        for delta in [0., -30., -70., 40., -200., 200.] {
            source.prepare_for_event(&mut text).unwrap();
            source.handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 60., y: 60. },
                delta_x: 0.,
                delta_y: delta,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Modifiers::empty(),
            }));
            source
                .render_retained_frame(&mut scene, &mut text, (200, 160), scale)
                .unwrap();
            let actual = renderer.render_scene(&scene, size, [0.; 4], false).unwrap();
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let lx = (x as f64 + 0.5) / scale;
                    let ly = (y as f64 + 0.5) / scale;
                    let outside = lx < 24. || lx >= 144. || ly < 24. || ly >= 124.;
                    let corner = (lx - 24.).min(144. - lx) < 3. && (ly - 24.).min(124. - ly) < 3.;
                    if outside || corner {
                        let index = ((y * size[0] + x) * 4) as usize;
                        assert_eq!(
                            &actual[index..index + 4],
                            &baseline[index..index + 4],
                            "escaped scrollport: scale={scale} delta={delta} at ({lx},{ly})"
                        );
                    }
                }
            }
        }
        assert!(
            source
                .placed_nodes()
                .iter()
                .any(|n| n.scroll.offset[1] > 0.)
        );
    }
}
