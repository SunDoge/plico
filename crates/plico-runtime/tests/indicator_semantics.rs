//! The MyGO ports must project their roles into the semantic tree, where the
//! native accessibility bridges read them.
use plico::UiContext;
use plico::widgets::{MeterLevels, breadcrumbs, meter};
use plico_renderer::{FrameSource, SemanticRole, TextContext};
use plico_runtime::{HostView, Source};

struct Trails;

impl HostView for Trails {
    fn show(&mut self, cx: &mut UiContext) {
        let mut chosen = 1;
        breadcrumbs(cx, &["Home", "Users", "Documents"], &mut chosen);
        meter(
            cx,
            "Disk",
            90.,
            0.0..=100.0,
            MeterLevels {
                warning: Some(80.),
                critical: Some(95.),
            },
        );
    }
}

#[test]
fn breadcrumbs_and_meter_project_their_semantic_roles() {
    let mut source = Source::new(Trails);
    let mut text = TextContext::new();
    let _ = source.build_frame(&mut text, 500, 200);
    let semantics = source.adapter.semantics(&source.engine, 0);
    let count = |role: SemanticRole| semantics.nodes.iter().filter(|n| n.role == role).count();
    assert_eq!(count(SemanticRole::Navigation), 1);
    assert_eq!(count(SemanticRole::Link), 2);
    assert_eq!(count(SemanticRole::Meter), 1);
    assert!(
        semantics
            .nodes
            .iter()
            .any(|n| n.role == SemanticRole::Link && n.label.as_deref() == Some("Users")),
        "breadcrumb links keep their accessible name"
    );
    assert!(
        semantics
            .nodes
            .iter()
            .any(|n| n.role == SemanticRole::Label && n.label.as_deref() == Some("Disk")),
        "the meter label stays readable text"
    );
}
