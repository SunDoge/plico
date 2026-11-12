//! Overlay scrollbar visibility is presentation state, driven by the host clock.
use crate::Adapter;
use plico::Engine;
use plico_renderer::{
    PointerPhase, UiEvent,
    scrollbar::{self, ScrollAxis},
};
use std::{collections::HashMap, time::Duration};
use taffy::NodeId;

const IDLE: Duration = Duration::from_millis(800);
const FADE: Duration = Duration::from_millis(200);
#[derive(Default)]
pub(super) struct Scrollbars {
    pub(super) states: HashMap<NodeId, Visibility>,
    pointer: Option<(f64, f64)>,
    dirty: bool,
    pub(super) reduced_motion: bool,
}
pub(super) struct Visibility {
    offset: [f32; 2],
    range: [f32; 2],
    inside: bool,
    pinned: bool,
    since: Option<Duration>,
    opacity: f32,
}
impl Visibility {
    fn new(offset: [f32; 2], range: [f32; 2]) -> Self {
        Self {
            offset,
            range,
            inside: false,
            pinned: false,
            since: None,
            opacity: 0.,
        }
    }
    fn reveal(&mut self, now: Duration) {
        self.since = Some(now);
    }
    fn sample(
        &mut self,
        now: Duration,
        offset: [f32; 2],
        range: [f32; 2],
        inside: bool,
        pinned: bool,
        moving: bool,
    ) -> f32 {
        if self.offset != offset
            || self.range != range
            || self.inside != inside
            || self.pinned != pinned
            || moving
            || pinned
        {
            self.reveal(now);
        }
        self.offset = offset;
        self.range = range;
        self.inside = inside;
        self.pinned = pinned;
        self.opacity = self.since.map_or(0., |since| {
            let age = now.saturating_sub(since);
            let t = (age.saturating_sub(IDLE).as_secs_f32() / FADE.as_secs_f32()).clamp(0., 1.);
            1. - t * t * (3. - 2. * t)
        });
        self.opacity
    }
}
impl Scrollbars {
    pub(super) fn needs_frame(&self, now: Duration) -> bool {
        self.dirty
            || self.states.values().any(|s| {
                !s.pinned
                    && s.opacity > 0.
                    && s.since
                        .is_some_and(|since| now.saturating_sub(since) >= IDLE)
            })
    }
    pub(super) fn wake_deadline(&self, now: Duration) -> Option<Duration> {
        self.states
            .values()
            .filter(|s| !s.pinned && s.opacity > 0.)
            .filter_map(|s| s.since?.checked_add(IDLE))
            .filter(|deadline| *deadline > now)
            .min()
    }
    pub(super) fn event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Pointer(p) => {
                self.pointer = match p.phase {
                    PointerPhase::Leave | PointerPhase::Cancel => None,
                    _ => Some((p.position.x, p.position.y)),
                }
            }
            UiEvent::Wheel(w) => self.pointer = Some((w.position.x, w.position.y)),
            UiEvent::Focus(false) => self.pointer = None,
            _ => {}
        }
    }
    pub(super) fn reveal(&mut self, id: NodeId, now: Duration) {
        if let Some(state) = self.states.get_mut(&id) {
            state.reveal(now);
            self.dirty = true;
        }
    }
}
impl Adapter {
    fn scrollbar_pointer(&self, engine: &Engine, index: usize) -> (bool, bool) {
        let node = &self.placed[index];
        let dragging = self
            .scrollbar_drag
            .is_some_and(|(id, ..)| id == node.node_id);
        let Some((x, y)) = self.scrollbars.pointer else {
            return (false, dragging);
        };
        if !self.modal_allows(engine, self.reverse[&node.node_id])
            || !self.visual_contains(node.node_id, x, y)
        {
            return (false, dragging);
        }
        let Some(point) = self.layout_point(node.node_id, x, y) else {
            return (false, dragging);
        };
        let [x0, y0, x1, y1] = node.scroll.port;
        let inside = point.x >= f64::from(x0)
            && point.x < f64::from(x1)
            && point.y >= f64::from(y0)
            && point.y < f64::from(y1);
        let edge = inside
            && [ScrollAxis::Vertical, ScrollAxis::Horizontal]
                .into_iter()
                .any(|axis| scrollbar::track(node, axis).is_some_and(|rect| rect.contains(point)));
        (inside, dragging || edge)
    }
    pub(super) fn scrollbar_pointer_changed(&mut self, engine: &Engine) -> bool {
        let changed = self.scroll_indices.iter().any(|&index| {
            let (inside, pinned) = self.scrollbar_pointer(engine, index);
            self.scrollbars
                .states
                .get(&self.placed[index].node_id)
                .is_some_and(|state| (state.inside, state.pinned) != (inside, pinned))
        });
        self.scrollbars.dirty |= changed;
        self.scrollbars.dirty
    }
    pub(super) fn update_scrollbars(&mut self, engine: &Engine) {
        let now = engine.time().now;
        for &index in &self.scroll_indices {
            self.stats.scroll_visits += 1;
            let (inside, pinned) = self.scrollbar_pointer(engine, index);
            let p = &mut self.placed[index];
            let scroll = p.scroll;
            let state = self
                .scrollbars
                .states
                .entry(p.node_id)
                .or_insert_with(|| Visibility::new(scroll.offset, scroll.range));
            let opacity = if scroll.range.iter().any(|&r| r > 0.5) {
                state.sample(
                    now,
                    scroll.offset,
                    scroll.range,
                    inside,
                    pinned,
                    self.scroll_motions.contains_key(&p.node_id),
                )
            } else {
                state.offset = scroll.offset;
                state.range = scroll.range;
                state.inside = inside;
                state.since = None;
                state.opacity = 0.;
                state.pinned = false;
                0.
            };
            let opacity = if self.scrollbars.reduced_motion
                && !pinned
                && state
                    .since
                    .is_some_and(|since| now.saturating_sub(since) >= IDLE)
            {
                state.opacity = 0.;
                0.
            } else {
                opacity
            };
            let interaction = if self.scrollbar_drag.is_some_and(|(id, ..)| id == p.node_id) {
                2
            } else if pinned {
                1
            } else {
                0
            };
            if scroll.opacity != opacity || scroll.interaction != interaction {
                p.scroll.opacity = opacity;
                p.scroll.interaction = interaction;
                self.stats.paint_writes += 1;
                self.scene_dirty_nodes.insert(p.node_id);
            }
        }
        self.scrollbars.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_fade_hover_drag_and_offset_changes_use_injected_time() {
        let mut v = Visibility::new([0.; 2], [0., 100.]);
        let ms = Duration::from_millis;
        assert_eq!(
            v.sample(ms(0), [0.; 2], [0., 100.], false, false, false),
            0.
        );
        assert_eq!(
            v.sample(ms(100), [0.; 2], [0., 100.], true, false, false),
            1.
        );
        assert_eq!(
            v.sample(ms(899), [0.; 2], [0., 100.], true, false, false),
            1.
        );
        assert!((v.sample(ms(1000), [0.; 2], [0., 100.], true, false, false) - 0.5).abs() < 1e-6);
        assert_eq!(
            v.sample(ms(1100), [0.; 2], [0., 100.], true, false, false),
            0.
        );
        assert_eq!(
            v.sample(ms(1200), [0., 10.], [0., 100.], true, false, false),
            1.
        );
        assert_eq!(
            v.sample(ms(5000), [0., 10.], [0., 100.], true, true, false),
            1.
        );
        assert_eq!(
            v.sample(ms(9000), [0., 10.], [0., 100.], false, true, false),
            1.
        );
        assert_eq!(
            v.sample(ms(9100), [0., 10.], [0., 100.], false, false, false),
            1.
        );
        assert_eq!(
            v.sample(ms(10100), [0., 10.], [0., 100.], false, false, false),
            0.
        );
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::{HostView, Source};
    use kurbo::Point;
    use plico::{VirtualClock, prelude::*};
    use plico_renderer::{
        FrameSource, Modifiers, PointerButton, PointerEvent, PointerProperties, Scene, TextContext,
    };
    struct Demo {
        calls: usize,
        mounted: bool,
        scrollable: bool,
        observe: bool,
    }
    impl HostView for Demo {
        fn show(&mut self, cx: &mut UiContext<'_>) {
            self.calls += 1;
            if self.mounted {
                column(cx)
                    .width(200.)
                    .height(160.)
                    .scroll_y(self.scrollable)
                    .children(|cx| {
                        if self.observe {
                            let _ = cx.scroll_viewport();
                        }
                        for i in 0..20 {
                            text(cx, format!("Row {i}")).height(30.);
                        }
                    });
            }
        }
    }
    fn frame(s: &mut Source<Demo>, text: &mut TextContext) -> Scene {
        let mut scene = Scene::new();
        s.render_retained_frame(&mut scene, text, (320, 240), 1.)
            .unwrap();
        scene
    }
    fn pointer(s: &mut Source<Demo>, phase: PointerPhase, point: Point) {
        s.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position: plico_renderer::Point {
                x: point.x,
                y: point.y,
            },
            button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
                .then_some(PointerButton::Primary),
            buttons: u32::from(matches!(phase, PointerPhase::Down)),
            modifiers: Modifiers::empty(),
            properties: PointerProperties::default(),
        }));
    }
    fn scroll(s: &Source<Demo>) -> plico_renderer::layout::ScrollMetrics {
        s.adapter
            .placed
            .iter()
            .find(|n| n.scroll.range[1] > 0.)
            .unwrap()
            .scroll
    }
    #[test]
    fn overlay_scrollbar_uses_the_edge_lane_without_narrowing_content() {
        struct Overlay;
        impl HostView for Overlay {
            fn show(&mut self, cx: &mut UiContext<'_>) {
                column(cx)
                    .width(200.)
                    .height(160.)
                    .scroll_y(true)
                    .children(|cx| {
                        column(cx)
                            .width_percent(100.)
                            .height(320.)
                            .shrink(0.)
                            .children(|_| {});
                    });
            }
        }
        let mut source = Source::with_clock(Overlay, VirtualClock::default());
        source
            .render_retained_frame(&mut Scene::new(), &mut TextContext::new(), (320, 240), 1.)
            .unwrap();
        let scroll = source
            .adapter
            .placed
            .iter()
            .find(|node| node.scroll.range[1] > 0.)
            .unwrap();
        let content = source
            .adapter
            .placed
            .iter()
            .find(|node| node.parent_node_id == Some(scroll.node_id))
            .unwrap();
        assert_eq!(content.rect[2] - content.rect[0], 200.);
        let thumb = scrollbar::thumb(scroll, ScrollAxis::Vertical).unwrap();
        assert_eq!((thumb.x0, thumb.x1), (191., 197.));
    }
    #[test]
    fn fade_changes_scene_without_declaring_or_layout_and_stops_when_hidden() {
        let clock = VirtualClock::default();
        let mut s = Source::with_clock(
            Demo {
                calls: 0,
                mounted: true,
                scrollable: true,
                observe: false,
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 0.);
        assert!(!s.has_anim());
        pointer(&mut s, PointerPhase::Move, Point::new(50., 50.));
        let shown = frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 1.);
        assert!(
            !s.has_anim(),
            "the scrollbar hold is a timer, not an animation"
        );
        let hide = s.animation_deadline().expect("fade needs a delayed wake");
        clock.set(Duration::from_millis(799)).unwrap();
        frame(&mut s, &mut text);
        assert!(!s.has_anim());
        assert_eq!(s.animation_deadline(), Some(hide));
        clock.set(Duration::from_millis(800)).unwrap();
        frame(&mut s, &mut text);
        assert!(s.has_anim(), "the fade itself samples display frames");
        let calls = s.view.calls;
        clock.set(Duration::from_millis(900)).unwrap();
        let fading = frame(&mut s, &mut text);
        assert!((scroll(&s).opacity - 0.5).abs() < 1e-5);
        assert_ne!(shown, fading, "cached scene must reflect scrollbar opacity");
        assert_eq!(s.view.calls, calls);
        assert_eq!(s.adapter.stats.layout_runs, 0);
        clock.set(Duration::from_millis(1000)).unwrap();
        let hidden = frame(&mut s, &mut text);
        assert_ne!(fading, hidden);
        assert_eq!(scroll(&s).opacity, 0.);
        assert!(!s.has_anim());
        assert_eq!(s.animation_deadline(), None);
        let n = s
            .adapter
            .placed
            .iter()
            .find(|n| n.scroll.range[1] > 0.)
            .unwrap();
        let thumb = scrollbar::thumb(n, ScrollAxis::Vertical).unwrap();
        assert!(
            scrollbar::hit(n, thumb.center()).is_none(),
            "hidden thumb cannot capture a click"
        );
        pointer(&mut s, PointerPhase::Move, thumb.center());
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 1.);
        assert_eq!(scroll(&s).interaction, 1);
        assert!(
            !s.has_anim(),
            "stationary hover does not need animation frames"
        );
        pointer(&mut s, PointerPhase::Down, thumb.center());
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).interaction, 2);
        clock.set(Duration::from_secs(5)).unwrap();
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 1.);
        assert_eq!(
            s.view.calls, calls,
            "drag/hover presentation must not run show"
        );
        pointer(&mut s, PointerPhase::Up, thumb.center());
        pointer(&mut s, PointerPhase::Leave, Point::new(310., 230.));
        frame(&mut s, &mut text);
        clock.set(Duration::from_secs(6)).unwrap();
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 0.);
        assert!(!s.has_anim());
        s.set_reduced_motion(true);
        pointer(&mut s, PointerPhase::Move, Point::new(50., 50.));
        frame(&mut s, &mut text);
        assert_eq!(scroll(&s).opacity, 1.);
        clock.set(Duration::from_millis(6900)).unwrap();
        frame(&mut s, &mut text);
        assert_eq!(
            scroll(&s).opacity,
            0.,
            "reduced motion skips the fading interval"
        );
        pointer(&mut s, PointerPhase::Move, Point::new(195., 50.));
        frame(&mut s, &mut text);
        s.update(|view| view.scrollable = false);
        frame(&mut s, &mut text);
        assert!(
            s.adapter.scrollbars.states.is_empty(),
            "turning scrolling off drops its timer"
        );
        assert!(!s.has_anim());
        s.update(|view| view.mounted = false);
        frame(&mut s, &mut text);
        assert!(s.adapter.scrollbars.states.is_empty());
    }
    #[test]
    fn dragging_keeps_explicit_scroll_observers_updated() {
        let mut s = Source::with_clock(
            Demo {
                calls: 0,
                mounted: true,
                scrollable: true,
                observe: true,
            },
            VirtualClock::default(),
        );
        let mut text = TextContext::new();
        frame(&mut s, &mut text);
        let node = s
            .adapter
            .placed
            .iter()
            .find(|n| n.scroll.range[1] > 0.)
            .unwrap();
        let at = scrollbar::thumb(node, ScrollAxis::Vertical)
            .unwrap()
            .center();
        pointer(&mut s, PointerPhase::Move, at);
        frame(&mut s, &mut text);
        pointer(&mut s, PointerPhase::Down, at);
        frame(&mut s, &mut text);
        let calls = s.view.calls;
        pointer(&mut s, PointerPhase::Move, at + (0., 20.));
        frame(&mut s, &mut text);
        assert!(scroll(&s).offset[1] > 0.);
        assert!(
            s.view.calls > calls,
            "virtual rows must still observe a drag offset change"
        );
    }
}
