use plico::{SharedElementGeometry, SharedElementMotion, VirtualClock, prelude::*};
use plico_renderer::{Scene, TextContext};
use plico_runtime::{HostView, Source};
use std::time::Duration;

const TILE: SharedElementGeometry = SharedElementGeometry::rect([20., 30., 80., 40.]);
const DETAIL: SharedElementGeometry = SharedElementGeometry::rect([130., 70., 240., 120.]);

#[derive(Default)]
struct Picture {
    open: bool,
    motion: SharedElementMotion,
}

impl HostView for Picture {
    fn show(&mut self, cx: &mut UiContext) {
        let target = if self.open { DETAIL } else { TILE };
        let Some(frame) = self
            .motion
            .sample(cx, 1_u64, TILE, target, Animation::SMOOTH)
        else {
            return;
        };
        let (clip, content) = frame.geometry.pinned(DETAIL).unwrap();
        column(cx).pinned_rect(clip).clip(true).children(|cx| {
            column(cx).pinned_rect(content).children(|cx| {
                text(cx, "Picture");
            });
        });
    }
}

#[test]
fn a_uniform_flight_updates_presentation_without_relaying_out_the_subtree() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Picture::default(), clock.clone());
    let mut scene = Scene::new();
    let mut text = TextContext::new();
    let mut render = |source: &mut Source<Picture>| {
        source
            .render_frame(&mut scene, &mut text, (420, 230), 1.)
            .unwrap();
    };
    let x = |source: &Source<Picture>| {
        let node = source
            .placed_nodes()
            .iter()
            .find(|node| node.paint.text.as_deref() == Some("Picture"))
            .unwrap();
        source.visual_bounds(node.node_id).unwrap()[0]
    };
    render(&mut source);
    let start = x(&source);
    source.update(|view| view.open = true);
    render(&mut source);
    assert_eq!(x(&source), start);
    for _ in 0..8 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let previous = x(&source);
        render(&mut source);
        assert!(x(&source) > previous);
        assert_eq!(source.frame_diagnostics().backend.layout_runs, 0);
    }

    clock.advance(Duration::from_secs(1)).unwrap();
    render(&mut source);
    assert!((x(&source) - DETAIL.clip[0]).abs() < 0.01);
}

#[derive(Default)]
struct Matched {
    side: bool,
    shows: usize,
}

impl HostView for Matched {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        // Both destination boxes have a tight parent clip. At handoff the
        // incoming subtree starts entirely outside its destination parent.
        if self.side {
            column(cx)
                .absolute(230., 45.)
                .width(150.)
                .height(100.)
                .clip(true)
                .children(|cx| {
                    matched_button(cx, 150., 100.);
                });
        } else {
            column(cx)
                .absolute(30., 30.)
                .width(90.)
                .height(60.)
                .clip(true)
                .children(|cx| {
                    matched_button(cx, 90., 60.);
                });
        }
        button(cx).text("Unrelated").response();
    }
}

#[component]
fn matched_button(cx: &mut UiContext<'_>, width: f32, height: f32) {
    column(cx)
        .width(width)
        .height(height)
        .shared_element("photo", Animation::SMOOTH)
        .children(|cx| {
            button(cx).text("Matched").width(width).height(height);
        });
}

#[test]
fn automatic_match_lifts_across_clips_and_reuses_declarations_until_landing() {
    use plico_renderer::{PaintCommand, Point, PrimitiveShape};

    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Matched::default(), clock.clone());
    let mut scene = Scene::new();
    let mut tcx = TextContext::new();
    let bounds = |source: &Source<Matched>| {
        source
            .semantics()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Matched"))
            .unwrap()
            .bounds
    };
    let render = |source: &mut Source<Matched>, scene: &mut Scene, tcx: &mut TextContext| {
        source.render_frame(scene, tcx, (440, 230), 1.).unwrap();
    };
    render(&mut source, &mut scene, &mut tcx);
    let origin = bounds(&source);
    source.update(|view| view.side = true);
    render(&mut source, &mut scene, &mut tcx);
    assert!(
        bounds(&source)
            .into_iter()
            .zip(origin)
            .all(|(a, b)| (a - b).abs() < 0.01)
    );
    assert!(source.hit_test(Point { x: 55., y: 55. }).is_some());
    assert!(source.hit_test(Point { x: 250., y: 90. }).is_none());
    let flat = scene.flattened();
    assert!(flat.commands.iter().any(|command| {
        matches!(command, PaintCommand::PushClip { clip, .. }
            if matches!(clip.primitive(), Some(PrimitiveShape::Rect(rect))
                if (rect.x0 - f64::from(origin[0])).abs() < 0.01))
    }));
    let shows = source.view().shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    render(&mut source, &mut scene, &mut tcx);
    let middle = bounds(&source);
    assert!(middle[0] > origin[0] && middle[0] < 230.);
    let backend = source.frame_diagnostics().backend;
    assert_eq!(backend.layout_runs, 0);
    assert!(backend.scene_parts_reused > 0);
    assert_eq!(source.view().shows, shows);

    source.update(|view| view.side = false);
    render(&mut source, &mut scene, &mut tcx);
    assert!(
        bounds(&source)
            .into_iter()
            .zip(middle)
            .all(|(a, b)| (a - b).abs() < 0.01)
    );

    clock.advance(Duration::from_secs(2)).unwrap();
    render(&mut source, &mut scene, &mut tcx);
    assert_eq!(bounds(&source), origin);
    assert!(!plico_renderer::FrameSource::has_anim(&source));

    source.set_reduced_motion(true);
    source.update(|view| view.side = true);
    render(&mut source, &mut scene, &mut tcx);
    assert_eq!(bounds(&source), [230., 45., 380., 145.]);
    assert!(!plico_renderer::FrameSource::has_anim(&source));
}
