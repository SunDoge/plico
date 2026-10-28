#[allow(dead_code)]
#[path = "support/icons.rs"]
mod icons;
use plico::{VirtualClock, prelude::*};
use plico_desktop::{Source, View};
use plico_renderer::svg::{SvgImage, usvg};
use plico_renderer::{FrameSource, PaintCommand, SvgDocument, TextContext};
use std::{sync::Arc, time::Duration};
fn stroke(group: &usvg::Group) -> Option<u32> {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) => {
                if let Some(stroke) = path.stroke()
                    && let usvg::Paint::Color(c) = stroke.paint()
                {
                    return Some((c.red as u32) << 16 | (c.green as u32) << 8 | c.blue as u32);
                }
            }
            usvg::Node::Group(g) => {
                if let Some(c) = stroke(g) {
                    return Some(c);
                }
            }
            _ => {}
        }
    }
    None
}
#[derive(Default)]
struct Demo {
    checked: bool,
    dark: bool,
    compact: bool,
    visible: bool,
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        if self.visible {
            checkbox(cx, "Choice", &mut self.checked);
            button(cx).text("Save").primary().children(|cx| {
                icon(cx, &icons::SAVE);
                icon(cx, &icons::SAVE);
                text(cx, "Save");
            });
            icon(cx, &icons::SAVE).color(Color::Accent);
        }
    }
    fn theme(&self) -> Theme {
        let mut theme = if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        if self.compact {
            theme.metrics = Metrics::compact();
        }
        theme
    }
}
fn check(s: &Source<Demo>) -> Option<&SvgDocument> {
    let scene = s
        .adapter
        .placed
        .iter()
        .find(|n| n.paint.text.as_deref() == Some("Choice"))?
        .paint
        .widget
        .as_ref()?;
    scene.commands.iter().find_map(|c| {
        if let PaintCommand::Svg { document, .. } = c {
            Some(document)
        } else {
            None
        }
    })
}
#[test]
fn representative_lucide_icons_parse_and_current_color_preserves_explicit_paints() {
    for asset in [
        icons::CHECK,
        icons::PLUS,
        icons::ROTATE_CCW,
        icons::SAVE,
        icons::ARCHIVE,
        icons::ARCHIVE_RESTORE,
        icons::FOLDER,
        icons::LAYOUT_GRID,
        icons::SLIDERS_HORIZONTAL,
        icons::SPARKLES,
        icons::CHEVRON_RIGHT,
        icons::MOON,
        icons::SUN,
        icons::LOCK,
    ] {
        let parsed = SvgImage::parse_with_current_color(asset.source(), 0x123456).unwrap();
        assert_eq!(parsed.size(), [24., 24.]);
        assert_eq!(stroke(parsed.document().0.root()), Some(0x123456));
    }
    let fixed=SvgImage::parse_with_current_color(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M1 1h20" stroke="#abcdef"/></svg>"##,0x123456).unwrap();
    assert_eq!(stroke(fixed.document().0.root()), Some(0xabcdef));
}
#[test]
fn icons_inherit_primary_foreground_and_share_only_matching_color_variants() {
    let mut s = Source::new(Demo {
        visible: true,
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 480, 400);
    let images: Vec<_> = s
        .adapter
        .placed
        .iter()
        .filter_map(|n| n.paint.svg.as_ref())
        .collect();
    assert_eq!(images.len(), 3);
    assert!(Arc::ptr_eq(images[0], images[1]));
    assert!(!Arc::ptr_eq(images[0], images[2]));
    assert_eq!(
        stroke(images[0].document().0.root()),
        Some(Theme::light().on_accent)
    );
    assert_eq!(
        stroke(images[2].document().0.root()),
        Some(Theme::light().accent)
    );
    assert_eq!(s.adapter.stats.svg_parses, 2);
    s.view.dark = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 400);
    let image = s
        .adapter
        .placed
        .iter()
        .find_map(|n| n.paint.svg.as_ref())
        .unwrap();
    assert_eq!(
        stroke(image.document().0.root()),
        Some(Theme::dark().on_accent)
    );
}
#[test]
fn checkbox_uses_animated_svg_check_with_cached_theme_color_and_keeps_layout() {
    let clock = VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            visible: true,
            compact: true,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.set_theme_transition_duration(Duration::ZERO);
    s.build_frame(&mut tcx, 480, 400);
    assert!(check(&s).is_none());
    s.view.checked = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 400);
    clock.set(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 480, 400);
    let mark = check(&s)
        .expect("checked indicator must draw an SVG check")
        .clone();
    assert_eq!(stroke(mark.0.root()), Some(Theme::light().on_accent));
    assert_eq!(s.adapter.stats.layout_runs, 0);
    clock.set(Duration::from_millis(140)).unwrap();
    s.build_frame(&mut tcx, 480, 400);
    assert!(Arc::ptr_eq(&mark.0, &check(&s).unwrap().0));
    assert_eq!(s.adapter.stats.svg_parses, 0);
    assert!(!s.has_anim());
    s.view.dark = true;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 400);
    assert_eq!(
        stroke(check(&s).unwrap().0.root()),
        Some(Theme::dark().on_accent)
    );
    s.view.checked = false;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 400);
    clock.set(Duration::from_millis(280)).unwrap();
    s.build_frame(&mut tcx, 480, 400);
    assert!(check(&s).is_none());
    s.view.visible = false;
    s.request_rebuild();
    s.build_frame(&mut tcx, 480, 400);
    assert!(s.adapter.svg_diagnostics().next().is_none());
}

#[test]
fn generated_control_icons_parse_without_a_runtime_registry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("plico/src");
    let source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("icons.rs.json")).unwrap()).unwrap();
    // Parse the committed generated literals, never the optional download cache.
    // This is test-only enumeration; applications have no all-icons registry.
    let rust = include_str!("../../plico/src/icons.rs");
    let mut remaining = rust;
    let mut count = 0;
    while let Some((_, literal)) = remaining.split_once("SvgAsset::new(") {
        let literal = literal.trim_start().strip_prefix('r').unwrap();
        let (hashes, literal) = literal.split_once('"').unwrap();
        assert!(!hashes.is_empty() && hashes.chars().all(|c| c == '#'));
        let ending = format!("\"{hashes}");
        let (xml, rest) = literal.split_once(&ending).unwrap();
        remaining = rest;
        let svg = SvgImage::parse_with_current_color(xml, 0x123456)
            .unwrap_or_else(|error| panic!("icon {count}: {error}"));
        assert_eq!(svg.size(), [24., 24.], "icon {count}");
        count += 1;
    }
    assert_eq!(count, source["files"].as_object().unwrap().len());
}
