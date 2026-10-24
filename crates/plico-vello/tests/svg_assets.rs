use plico_core::{Motion, SvgAsset, UiContext, VirtualClock, component};
use plico_renderer::{FrameSource, TextContext};
use plico_vello::{Source, View};
use std::{sync::Arc, time::Duration};
static ICON: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2Z" fill="#285bd6"/></svg>"##,
);
static BAD: SvgAsset = SvgAsset::new("not an SVG");
struct Icons {
    visible: bool,
    invalid: bool,
    wide: bool,
}
impl View for Icons {
    #[component]
    fn show(&mut self, ui: &mut UiContext) {
        if self.visible {
            let asset = if self.invalid { &BAD } else { &ICON };
            ui.svg(asset)
                .size(if self.wide { 48. } else { 24. }, 24.)
                .motion(Motion::Smooth);
            ui.svg(asset).size(24., 24.);
        }
    }
}
#[test]
fn identical_svg_assets_share_parsed_resource_across_nodes_and_animation_frames() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(
        Icons {
            visible: true,
            invalid: false,
            wide: false,
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(s.adapter.stats.svg_parses, 1);
    let svg: Vec<_> = s
        .adapter
        .placed
        .iter()
        .filter_map(|n| n.paint.svg.as_ref())
        .collect();
    assert_eq!(svg.len(), 2);
    assert!(Arc::ptr_eq(svg[0], svg[1]));
    assert!(s.adapter.svg_diagnostics().next().is_none());
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .nodes
            .iter()
            .all(|n| n.label.as_ref().is_none_or(|t| !t.contains("<svg")))
    );
    s.view.wide = true;
    s.build_frame(&mut tcx, 300, 100);
    let parsed = s
        .adapter
        .placed
        .iter()
        .find_map(|n| n.paint.svg.clone())
        .unwrap();
    clock.set(Duration::from_millis(120)).unwrap();
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(s.adapter.stats.svg_parses, 0);
    assert!(
        s.adapter
            .placed
            .iter()
            .filter_map(|n| n.paint.svg.as_ref())
            .all(|v| Arc::ptr_eq(v, &parsed))
    );
}
#[test]
fn malformed_assets_report_once_and_release_diagnostics_on_unmount() {
    let mut s = Source::new(Icons {
        visible: true,
        invalid: true,
        wide: false,
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 300, 100);
    assert!(s.last_error.is_none());
    assert_eq!(s.adapter.stats.svg_parses, 1);
    assert_eq!(s.adapter.svg_diagnostics().count(), 1);
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(s.adapter.stats.svg_parses, 0);
    s.view.visible = false;
    s.build_frame(&mut tcx, 300, 100);
    assert_eq!(s.adapter.svg_diagnostics().count(), 0);
}
