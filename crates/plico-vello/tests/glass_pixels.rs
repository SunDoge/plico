//! Pixel-level regression tests for the Glass page backdrop blur.
//!
//! Verifies, across light/dark and 1x/2x scale, that (a) toggling blur only
//! changes pixels inside the glass card (plus the toggling checkbox itself),
//! and (b) during continuous background motion the blurred output tracks the
//! moving background instead of freezing.
use plico_gallery::{Gallery, Page};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_vello::Source;
use std::time::Duration;

fn glass_source(dark: bool, blur: bool) -> (Source<Gallery>, plico_core::VirtualClock) {
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            dark,
            router: plico_core::Router::new(Page::Glass),
            ..Default::default()
        },
        clock.clone(),
    );
    if !blur {
        source.view.glass.enabled = false;
    }
    (source, clock)
}

fn render(source: &mut Source<Gallery>, tcx: &mut TextContext) -> image::RgbaImage {
    let nodes = source.build_frame(tcx, 1120, 820);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, tcx, (1120, 820), 1.);
    plico_shell::renderer::render_to_image(&scene, 1120, 820, source.base_color())
        .expect("offscreen render")
}

/// Glass card geometry in the default 1120x820 capture, with a tolerance for
/// the blur halo bleeding up to the filter extent outside... no: the clip must
/// confine the output, but the card's own rounded corners keep a 1-2px AA band.
const CARD: (u32, u32, u32, u32) = (222, 326, 556, 570);
/// The "Blur background" checkbox indicator legitimately differs.
const CHECKBOX: (u32, u32, u32, u32) = (190, 165, 220, 200);

fn outside(a: (u32, u32)) -> bool {
    let (cx0, cy0, cx1, cy1) = CARD;
    let (bx0, by0, bx1, by1) = CHECKBOX;
    !(cx0 <= a.0 && a.0 <= cx1 && cy0 <= a.1 && a.1 <= cy1)
        && !(bx0 <= a.0 && a.0 <= bx1 && by0 <= a.1 && a.1 <= by1)
}

#[test]
fn blur_only_changes_pixels_inside_the_glass_card() {
    for dark in [false, true] {
        let mut tcx = TextContext::new();
        let (mut on, clock_on) = glass_source(dark, true);
        let (mut off, clock_off) = glass_source(dark, false);
        // Identical clock progression for both.
        render(&mut on, &mut tcx);
        render(&mut off, &mut tcx);
        clock_on.advance(Duration::from_millis(1000)).unwrap();
        clock_off.advance(Duration::from_millis(1000)).unwrap();
        let on = render(&mut on, &mut tcx);
        let off = render(&mut off, &mut tcx);
        let mut outside_diffs = Vec::new();
        for y in (0..820).step_by(2) {
            for x in (0..1120).step_by(2) {
                if outside((x, y)) {
                    let a = on.get_pixel(x, y).0;
                    let b = off.get_pixel(x, y).0;
                    if a != b {
                        outside_diffs.push((x, y, a, b));
                    }
                }
            }
        }
        assert!(
            outside_diffs.is_empty(),
            "dark={dark}: blur changed pixels outside the card: {:?}",
            &outside_diffs[..outside_diffs.len().min(4)]
        );
    }
}

#[test]
fn blur_tracks_the_moving_background_across_frames() {
    let mut tcx = TextContext::new();
    let (mut source, clock) = glass_source(false, true);
    render(&mut source, &mut tcx);
    // Sample a fixed point inside the card where the drifting circle edge
    // sweeps across; the blurred sample must change as the background moves.
    let probe = (330u32, 420u32);
    let mut seen = Vec::new();
    for _ in 0..12 {
        clock.advance(Duration::from_millis(120)).unwrap();
        let image = render(&mut source, &mut tcx);
        seen.push(image.get_pixel(probe.0, probe.1).0);
    }
    let distinct: std::collections::HashSet<[u8; 4]> = seen.iter().copied().collect();
    assert!(
        distinct.len() > 4,
        "blurred output froze while the background moved: {seen:?}"
    );
}

#[test]
fn blur_stays_inside_the_card_at_hidpi() {
    // scene.rs warns that some GPU backends do not reliably carry a parent
    // clip across an appended scene at HiDPI; the backdrop blur is exactly
    // that shape, so verify the clip holds at 2x.
    for dark in [false, true] {
        let mut tcx = TextContext::new();
        let (mut on, clock_on) = glass_source(dark, true);
        let (mut off, clock_off) = glass_source(dark, false);
        let mut frame = |source: &mut Source<Gallery>, clock: &plico_core::VirtualClock| {
            let nodes = source.build_frame(&mut tcx, 2240, 1640);
            assert!(source.last_error.is_none());
            let mut scene = Scene::new();
            source.paint_scene(&mut scene, &nodes, &mut tcx, (2240, 1640), 2.);
            clock.advance(Duration::from_millis(1000)).unwrap();
            let nodes = source.build_frame(&mut tcx, 2240, 1640);
            let mut scene = Scene::new();
            source.paint_scene(&mut scene, &nodes, &mut tcx, (2240, 1640), 2.);
            plico_shell::renderer::render_to_image(&scene, 2240, 1640, source.base_color())
                .expect("offscreen render")
        };
        let on = frame(&mut on, &clock_on);
        let off = frame(&mut off, &clock_off);
        // Card and checkbox geometry double at 2x.
        let (cx0, cy0, cx1, cy1) = (444u32, 652u32, 1112u32, 1140u32);
        let (bx0, by0, bx1, by1) = (380u32, 330u32, 440u32, 400u32);
        let mut outside_diffs = Vec::new();
        for y in (0..1640).step_by(4) {
            for x in (0..2240).step_by(4) {
                let inside_card = cx0 <= x && x <= cx1 && cy0 <= y && y <= cy1;
                let inside_box = bx0 <= x && x <= bx1 && by0 <= y && y <= by1;
                if !inside_card && !inside_box {
                    let a = on.get_pixel(x, y).0;
                    let b = off.get_pixel(x, y).0;
                    if a != b {
                        outside_diffs.push((x, y, a, b));
                    }
                }
            }
        }
        assert!(
            outside_diffs.is_empty(),
            "dark={dark}: 2x blur changed pixels outside the card: {:?}",
            &outside_diffs[..outside_diffs.len().min(4)]
        );
    }
}

/// Every animation frame through the retained (cached-scene) path must match
/// a direct render — not just the first one after an event.
#[test]
fn retained_frame_matches_direct_across_animation_frames() {
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Glass),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    for step in 0..10 {
        clock.advance(Duration::from_millis(120)).unwrap();
        let nodes = source.build_frame(&mut tcx, 1120, 820);
        assert!(source.last_error.is_none());
        let mut direct = Scene::new();
        source.paint_scene(&mut direct, &nodes, &mut tcx, (1120, 820), 1.);
        let direct_image =
            plico_shell::renderer::render_to_image(&direct, 1120, 820, source.base_color())
                .unwrap();
        let mut retained = Scene::new();
        source.render_retained_frame(&mut retained, &mut tcx, (1120, 820), 1.);
        let retained_image =
            plico_shell::renderer::render_to_image(&retained, 1120, 820, source.base_color())
                .unwrap();
        let mut diff = 0;
        let mut first = None;
        for y in 0..820 {
            for x in 0..1120 {
                let a = direct_image.get_pixel(x, y).0;
                let b = retained_image.get_pixel(x, y).0;
                if a != b {
                    diff += 1;
                    if first.is_none() {
                        first = Some((x, y, a, b));
                    }
                }
            }
        }
        assert_eq!(
            diff, 0,
            "step {step}: retained/direct differ at {diff} px, first {first:?}"
        );
    }
}

/// The glass panel during a theme transition (GPU clip reveal composites over
/// the blurred scene) must keep its blur confined to the card.
#[test]
fn blur_stays_confined_during_theme_transition() {
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            dark: false,
            router: plico_core::Router::new(Page::Glass),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    clock.advance(Duration::from_millis(1000)).unwrap();
    source.build_frame(&mut tcx, 1120, 820);
    // Flip the theme mid-animation; sample during the reveal.
    source.view.dark = true;
    for step in 0..8 {
        clock.advance(Duration::from_millis(40)).unwrap();
        let mut scene = Scene::new();
        source.render_retained_frame(&mut scene, &mut tcx, (1120, 820), 1.);
        let image =
            plico_shell::renderer::render_to_image(&scene, 1120, 820, source.base_color()).unwrap();
        // The blurred card region must remain interior to the card rect: the
        // border color of the card stays a theme border tone, not a background
        // circle color bleeding through unblurred outside the clip.
        // Pixels just outside the right edge must never become light-tinted
        // glass colors while the theme flips.
        for y in [340u32, 420u32, 500u32] {
            let outside_px = image.get_pixel(586, y).0;
            let is_glass_tint = outside_px[0] > outside_px[2] && outside_px[1] > 90;
            assert!(
                !is_glass_tint,
                "step {step}: glass tint leaked outside the card at y={y}: {outside_px:?}"
            );
        }
    }
}

/// The backdrop filter layer must be clipped to the card's own extent, not
/// the whole viewport. This checks command bounds, not actual GPU work or
/// whether the renderer avoids processing the rest of the replayed prefix.
#[test]
fn backdrop_filter_layer_is_clipped_to_the_card() {
    let clock = plico_core::VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Glass),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    clock.advance(Duration::from_millis(1000)).unwrap();
    let nodes = source.build_frame(&mut tcx, 1120, 820);
    let mut scene = Scene::new();
    source.paint_scene(&mut scene, &nodes, &mut tcx, (1120, 820), 1.);
    use plico_renderer::PaintCommand;
    use vello_common::kurbo::Shape as _;
    let filter_clips: Vec<_> = scene
        .commands
        .iter()
        .filter_map(|c| match c {
            PaintCommand::PushLayer {
                filter: Some(_),
                clip,
                ..
            } => {
                let bounds = clip.bounding_box();
                Some((bounds.x0, bounds.y0, bounds.x1, bounds.y1))
            }
            _ => None,
        })
        .collect();
    assert_eq!(filter_clips.len(), 1, "one glass panel, one filter layer");
    let (x0, y0, x1, y1) = filter_clips[0];
    // Card rect (logical px) with rounding tolerance; must be far from viewport-sized.
    assert!(
        (x1 - x0) < 400. && (y1 - y0) < 300.,
        "filter clip {filter_clips:?} covers the card, not the 1120x820 viewport"
    );
    assert!((220. ..230.).contains(&x0) && (310. ..335.).contains(&y0));
}
