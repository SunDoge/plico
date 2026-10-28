//! Follow actual retained smooth scrolling from fractional geometry to text pixels.
use plico::{VirtualClock, prelude::*};
use plico_desktop::{Source, View};
use plico_renderer::{
    FrameSource, GesturePhase, Modifiers, Point, Scene, TextContext, UiEvent, WheelDeltaMode,
    WheelEvent,
};
use std::{path::PathBuf, time::Duration};

struct Demo;
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .width(120.)
            .height(100.)
            .scroll_y(true)
            .smooth_scroll(true)
            .background(Color::Literal(0x000000))
            + |cx| {
                column(cx).height(300.).shrink(0.).padding(20.)
                    + |cx| {
                        text(cx, "H").font_size(14).color(Color::Literal(0xffffff));
                    };
            };
    }
}

fn main() {
    let output = PathBuf::from("reports/retained-text-scroll");
    std::fs::create_dir_all(&output).unwrap();
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Demo, clock.clone());
    let mut text = TextContext::new();
    text.load_font(std::fs::read("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf").unwrap());
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (120, 100), 1.)
        .unwrap();
    assert!(
        source
            .handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x: 40., y: 40. },
                delta_x: 0.,
                delta_y: 1.,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Modifiers::empty(),
            }))
            .handled
    );
    let mut records = Vec::new();
    let mut previous = [None, None];
    for tick in 0..33 {
        if tick > 0 {
            clock.advance(Duration::from_millis(4)).unwrap();
        }
        source
            .render_retained_frame(&mut scene, &mut text, (120, 100), 1.)
            .unwrap();
        let positions: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|command| {
                let plico_renderer::PaintCommand::GlyphRun {
                    transform,
                    glyphs,
                    hint,
                    ..
                } = command
                else {
                    return None;
                };
                let glyph = glyphs.first()?;
                let p = *transform * kurbo::Point::new(glyph.x.into(), glyph.y.into());
                Some((p.x, p.y, *hint))
            })
            .collect();
        assert_eq!(positions.len(), 1);
        for (index, mode, hinted) in [(0, "hint", true), (1, "nohint", false)] {
            let mut paint_scene = scene.clone();
            if !hinted {
                for command in paint_scene.commands.iter_mut() {
                    if let plico_renderer::PaintCommand::GlyphRun { hint, .. } = command {
                        *hint = false;
                    }
                }
            }
            let path = output.join(format!("{mode}-{tick:02}.png"));
            plico_shell::renderer::render_to_png(
                &paint_scene,
                120,
                100,
                source.base_color(),
                path.to_str().unwrap(),
            )
            .unwrap();
            let image = image::open(path).unwrap().to_rgba8();
            let mut pixels = Vec::new();
            let (mut mass, mut moment) = (0_u64, 0_u64);
            for y in 10..55 {
                for x in 10..50 {
                    let value = image.get_pixel(x, y).0[0];
                    pixels.push(value);
                    mass += u64::from(value);
                    moment += u64::from(value) * u64::from(y);
                }
            }
            assert!(mass > 0);
            records.push(serde_json::json!({"tick": tick, "time_ms": tick*4,
            "glyph_y": positions[0].1, "hint": hinted, "mode": mode,
            "pixel_centroid_y": moment as f64 / mass as f64,
            "same_pixels_as_previous": previous[index].as_ref().map(|v| v == &pixels),
            "layout_runs": source.adapter.stats.layout_runs}));
            previous[index] = Some(pixels);
        }
    }
    std::fs::write(
        output.join("measurements.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
    println!("{}", output.display());
}
