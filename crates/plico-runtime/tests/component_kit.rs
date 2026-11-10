use plico::prelude::*;
use plico_runtime::{
    HostView,
    testing::{Command, Driver},
};
struct Kit {
    page: usize,
    selected: bool,
    notice: bool,
    tag: bool,
    actions: usize,
}
impl Default for Kit {
    fn default() -> Self {
        Self {
            page: usize::MAX - 1,
            selected: false,
            notice: true,
            tag: true,
            actions: 0,
        }
    }
}
impl HostView for Kit {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).children(|cx| {
            pagination(cx, &mut self.page, usize::MAX);
            chip(cx, "Pinned", &mut self.selected);
            if self.tag && tag(cx, "Rust") {
                self.tag = false;
            }
            notification(cx, "Saved", &mut self.notice, Tone::Success);
            alert(cx, "Information", "Ordinary immediate children", Tone::Info);
            badge_tone(cx, "Ready", Tone::Success);
            if empty_state(cx, "No results", "Try another query", Some("Clear query")) {
                self.actions += 1;
            }
            skeleton(cx, 160., 16., false);
        });
    }
}
fn click(ui: &mut Driver<Kit>, label: &str) {
    ui.execute(Command::Click {
        role: "button".into(),
        label: label.into(),
        occurrence: None,
    })
    .unwrap();
}
#[test]
fn managed_kit_counterparts_have_bounded_pagination_and_native_actions() {
    let mut ui = Driver::new(Kit::default(), 1000, 800).unwrap();
    let pages = ui
        .snapshot()
        .exposed_nodes()
        .iter()
        .filter(|node| {
            node.label
                .as_ref()
                .is_some_and(|label| label.starts_with("Page "))
        })
        .count();
    assert!(pages <= 5);
    click(&mut ui, "Next page");
    assert_eq!(ui.source.view.page, usize::MAX);
    let next = ui
        .snapshot()
        .exposed_nodes()
        .into_iter()
        .find(|node| node.label.as_deref() == Some("Next page"))
        .unwrap()
        .clone();
    assert!(next.disabled);
    click(&mut ui, "Previous page");
    assert_eq!(ui.source.view.page, usize::MAX - 1);
    click(&mut ui, "Page 1");
    assert_eq!(ui.source.view.page, 1);
    click(&mut ui, "Pinned");
    assert!(ui.source.view.selected);
    click(&mut ui, "Pinned");
    assert!(!ui.source.view.selected);
    click(&mut ui, "Remove Rust");
    assert!(!ui.source.view.tag);
    click(&mut ui, "Dismiss notification");
    assert!(!ui.source.view.notice);
    click(&mut ui, "Clear query");
    assert_eq!(ui.source.view.actions, 1);
}
#[test]
fn info_tone_is_theme_resolved_and_old_theme_json_remains_compatible() {
    let light = plico::Theme::light();
    let dark = plico::Theme::dark();
    assert_eq!(light.resolve(Tone::Info.color()), 0x1d4ed8);
    assert_eq!(dark.resolve(Tone::Info.color()), 0x93c5fd);
    let old: plico::Theme = serde_json::from_str("{}").unwrap();
    assert_eq!(old.info, light.info);
}

#[derive(Default)]
struct Loading {
    animated: bool,
    shows: usize,
}
impl HostView for Loading {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        skeleton(cx, 160., 16., self.animated);
    }
}
#[test]
fn skeleton_uses_native_animation_without_redeclaring_the_view_and_can_go_idle() {
    use plico::{Clock, VirtualClock};
    use plico_renderer::{FrameSource, Scene, TextContext};
    use std::time::Duration;
    let clock = VirtualClock::default();
    let mut source = plico_runtime::Source::with_clock(
        Loading {
            animated: true,
            shows: 0,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    source.build_frame(&mut text, 320, 200);
    assert!(source.has_anim());
    let shows = source.view.shows;
    for _ in 0..3 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let mut scene = Scene::new();
        source.render_retained_frame(&mut scene, &mut text, (320, 200), 1.);
    }
    assert_eq!(source.view.shows, shows);
    assert!(clock.now() >= Duration::from_millis(48));
    source.view.animated = false;
    source.request_rebuild();
    source.build_frame(&mut text, 320, 200);
    assert!(!source.has_anim());
}

#[test]
fn migrated_semantic_status_tones_remain_distinguishable_on_default_surfaces() {
    fn luminance(rgb: u32) -> f64 {
        let linear = |shift: u32| {
            let channel = ((rgb >> shift) & 255u32) as f64 / 255.;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(16) + 0.7152 * linear(8) + 0.0722 * linear(0)
    }
    for theme in [plico::Theme::light(), plico::Theme::dark()] {
        for tone in [Tone::Info, Tone::Success, Tone::Warning, Tone::Danger] {
            let ink = luminance(theme.resolve(tone.color()));
            let surface = luminance(theme.control);
            let contrast = (ink.max(surface) + 0.05) / (ink.min(surface) + 0.05);
            // These are MyGO's exact status accents. They are used primarily
            // for indicators and short labels, not as a body-text ink; the
            // light success and warning accents intentionally do not meet the
            // WCAG 4.5:1 body-text threshold on the control face.
            assert!(contrast >= 2.75, "{tone:?}: {contrast}");
        }
    }
}
