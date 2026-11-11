//! The MyGO ports must project their roles into the semantic tree, where the
//! native accessibility bridges read them.
use plico::UiContext;
use plico::widgets::{
    MeterLevels, breadcrumbs, meter, progress, sidebar, sidebar_item, sidebar_section, split,
    stepper,
};
use plico_renderer::{FrameSource, SemanticRole, TextContext};
use plico_runtime::{HostView, Source};

struct Trails {
    disk: f64,
}

impl HostView for Trails {
    fn show(&mut self, cx: &mut UiContext) {
        let mut chosen = 1;
        breadcrumbs(cx, &["Home", "Users", "Documents"], &mut chosen);
        meter(
            cx,
            "Disk",
            self.disk,
            0.0..=100.0,
            MeterLevels {
                warning: Some(80.),
                critical: Some(95.),
            },
        );
        progress(cx, "Download", 0.25);
        let mut value = 4.0;
        stepper(cx, &mut value, 1.0..=99.0, 1.0);
        let mut pane = 120.0;
        split(
            cx,
            &mut pane,
            |cx| {
                cx.text("Left");
            },
            |cx| {
                cx.text("Right");
            },
        );
        let mut open = true;
        let _ = sidebar(cx).width(200.).height(120.)
            + |cx| {
                sidebar_section(cx, "Section", Some(&mut open), |cx| {
                    let _ = sidebar_item(cx, "Item", true, None);
                });
            };
    }
}

#[test]
fn breadcrumbs_and_meter_project_their_semantic_roles() {
    let mut source = Source::new(Trails { disk: 90. });
    let mut text = TextContext::new();
    let _ = source.build_frame(&mut text, 500, 500);
    let semantics = source.adapter.semantics(&source.engine, 0);
    let count = |role: SemanticRole| semantics.nodes.iter().filter(|n| n.role == role).count();
    assert_eq!(count(SemanticRole::Navigation), 1);
    assert_eq!(count(SemanticRole::Link), 2);
    assert_eq!(count(SemanticRole::Meter), 1);
    assert_eq!(count(SemanticRole::ProgressBar), 1);
    assert!(count(SemanticRole::SpinButton) >= 1);
    assert_eq!(count(SemanticRole::Separator), 1);
    assert_eq!(count(SemanticRole::Tree), 1);
    assert!(count(SemanticRole::TreeItem) >= 2);
    assert!(
        semantics
            .nodes
            .iter()
            .any(|n| n.role == SemanticRole::Link && n.label.as_deref() == Some("Users")),
        "breadcrumb links keep their accessible name"
    );
    let meter = semantics
        .nodes
        .iter()
        .find(|node| node.role == SemanticRole::Meter)
        .unwrap();
    assert_eq!(meter.label.as_deref(), Some("Disk"));
    assert_eq!(meter.numeric_value, Some(90.));
    assert_eq!(meter.min_numeric_value, Some(0.));
    assert_eq!(meter.max_numeric_value, Some(100.));
    assert!(
        meter.children.is_empty(),
        "visual labels are not announced twice"
    );
    let progress = semantics
        .nodes
        .iter()
        .find(|node| node.role == SemanticRole::ProgressBar)
        .unwrap();
    assert_eq!(progress.label.as_deref(), Some("Download"));
    assert_eq!(progress.numeric_value, Some(0.25));
    assert_eq!(progress.min_numeric_value, Some(0.));
    assert_eq!(progress.max_numeric_value, Some(1.));
    assert!(
        progress.children.is_empty(),
        "visual labels are not announced twice"
    );

    source.view.disk = 25.;
    source.request_rebuild();
    let _ = source.build_frame(&mut text, 500, 500);
    let semantics = source.adapter.semantics(&source.engine, 0);
    assert_eq!(
        semantics
            .nodes
            .iter()
            .find(|node| node.role == SemanticRole::Meter)
            .unwrap()
            .numeric_value,
        Some(25.),
        "retained semantic caches receive numeric-only updates"
    );
}
