use backend_settings::{ImmediateSettings, Page, Settings};
use plico::{ViewNode, VirtualClock};
use plico_renderer::{SemanticRole, SemanticSnapshot, TextContext};
use plico_runtime::{
    Source,
    testing::{Command, dispatch},
};
use std::time::Duration;

struct Harness {
    source: Source<Settings>,
    text: TextContext,
    clock: VirtualClock,
    width: u32,
}
impl Harness {
    fn new(width: u32) -> Self {
        let clock = VirtualClock::default();
        let mut app = Self {
            source: Source::with_clock(Settings::default(), clock.clone()),
            text: TextContext::new(),
            clock,
            width,
        };
        app.frame();
        app
    }
    fn frame(&mut self) {
        self.source
            .experiment_prepare_without_scene(&mut self.text, self.width, 820);
        assert!(
            self.source.last_error.is_none(),
            "{:?}",
            self.source.last_error
        );
    }
    fn at(&mut self, millis: u64) {
        self.clock.set(Duration::from_millis(millis)).unwrap();
        self.frame();
    }
    fn command(&mut self, command: Command) {
        dispatch(&mut self.source, &mut self.text, &command).unwrap();
        self.frame();
    }
    fn click(&mut self, role: &str, label: &str) {
        self.command(Command::Click {
            role: role.into(),
            label: label.into(),
            occurrence: None,
        });
    }
    fn has(&self, label: &str) -> bool {
        fn find(nodes: &[ViewNode], label: &str) -> bool {
            nodes
                .iter()
                .any(|node| node.text == label || find(&node.children, label))
        }
        find(&self.source.engine.snapshot(), label)
    }
    fn events(&self) -> Vec<String> {
        fn collect(nodes: &[ViewNode], output: &mut Vec<String>) {
            for node in nodes {
                if node.text.starts_with("Sync checkpoint ") {
                    output.push(node.text.clone());
                }
                collect(&node.children, output);
            }
        }
        let mut output = Vec::new();
        collect(&self.source.engine.snapshot(), &mut output);
        output
    }
}
fn select_all() -> Command {
    Command::KeyEvent {
        event: plico::KeyEvent {
            phase: plico::KeyPhase::Down,
            key: "a".into(),
            code: "KeyA".into(),
            key_without_modifiers: "a".into(),
            text: None,
            text_with_all_modifiers: None,
            location: plico::KeyLocation::Standard,
            modifiers: plico::Modifiers::CONTROL,
            repeat: false,
            synthetic: false,
        },
    }
}

#[test]
fn route_pages_keep_editor_and_activity_state() {
    let mut app = Harness::new(1100);
    app.click("button", "Activity");
    assert_eq!(app.source.view.current_page(), Page::Activity);
    app.click("button", "Reverse order");
    assert_eq!(app.events().first().unwrap(), "Sync checkpoint 30");

    app.click("button", "Preferences");
    app.command(Command::Focus {
        role: "textbox".into(),
        label: "Workspace name".into(),
        occurrence: None,
    });
    app.command(select_all());
    app.command(Command::Text {
        text: "Reactive workspace".into(),
    });
    app.click("button", "Save changes");
    assert!(app.has("Saved 1: Reactive workspace"));

    app.click("button", "Activity");
    assert_eq!(app.events().first().unwrap(), "Sync checkpoint 30");
    app.click("button", "Preferences");
    assert!(app.has("Current name: Reactive workspace"));
    assert!(app.has("Saved 1: Reactive workspace"));

    app.click("button", "Back");
    assert_eq!(app.source.view.current_page(), Page::Activity);
    app.click("button", "Forward");
    assert_eq!(app.source.view.current_page(), Page::Preferences);
}

#[test]
fn animation_policy_advances_without_relying_on_root_declaration() {
    let mut app = Harness::new(1100);
    app.click("button", "Details");
    app.at(130);
    assert!(app.source.view.inspector_width() > 0.);
    assert!(app.source.view.inspector_width() < 248.);
    app.at(300);
    assert_eq!(app.source.view.inspector_width(), 248.);

    app.click("button", "Run preview");
    app.at(1500);
    assert!((app.source.view.progress() - 0.5).abs() < 0.001);
    app.at(2700);
    assert_eq!(app.source.view.progress(), 1.);
    assert!(!app.source.view.animation_active());

    app.click("button", "Toggle sidebar");
    app.at(2830);
    assert!((app.source.view.sidebar_width() - 88.).abs() < 0.01);
    app.at(2960);
    assert_eq!(app.source.view.sidebar_width(), 72.);
    assert!(app.source.view.compact());
    assert!(!app.source.view.animation_active());
}

#[test]
fn reduced_motion_and_narrow_viewport_keep_the_saved_preference() {
    let mut app = Harness::new(1100);
    app.source.view.set_reduced_motion(true);
    app.click("button", "Toggle sidebar");
    assert_eq!(app.source.view.sidebar_width(), 72.);
    assert!(app.source.view.compact());

    app.click("button", "Toggle sidebar");
    assert_eq!(app.source.view.sidebar_width(), 200.);
    app.width = 700;
    app.frame();
    assert_eq!(app.source.view.sidebar_width(), 72.);
    assert!(!app.source.view.compact());
    app.width = 1100;
    app.frame();
    assert_eq!(app.source.view.sidebar_width(), 200.);
}

fn semantic_leaves(
    snapshot: &SemanticSnapshot,
) -> Vec<(SemanticRole, Option<String>, Option<String>, bool, [f32; 4])> {
    snapshot
        .exposed_nodes()
        .into_iter()
        .filter(|node| {
            matches!(
                node.role,
                SemanticRole::Label
                    | SemanticRole::Heading
                    | SemanticRole::TextInput
                    | SemanticRole::Button
                    | SemanticRole::CheckBox
            )
        })
        .map(|node| {
            (
                node.role,
                node.label.clone(),
                node.value.clone(),
                node.disabled,
                node.bounds,
            )
        })
        .collect()
}

#[test]
fn reactive_pages_match_immediate_semantics_and_leaf_geometry() {
    for page in [Page::Overview, Page::Activity, Page::Preferences] {
        for width in [1100, 520] {
            let clock = VirtualClock::default();
            let mut immediate = Source::with_clock(ImmediateSettings::default(), clock.clone());
            let mut reactive = Source::with_clock(Settings::default(), clock);
            immediate.view.reduced_motion = true;
            immediate.view.advanced = true;
            immediate.view.inspector = true;
            immediate.view.router.go(page);
            reactive.view.set_reduced_motion(true);
            reactive.view.set_advanced(true);
            reactive.view.set_inspector(true);
            reactive.view.go(page);
            let mut immediate_text = TextContext::new();
            let mut reactive_text = TextContext::new();
            immediate.experiment_prepare_without_scene(&mut immediate_text, width, 820);
            reactive.experiment_prepare_without_scene(&mut reactive_text, width, 820);
            assert!(immediate.last_error.is_none(), "{:?}", immediate.last_error);
            assert!(reactive.last_error.is_none(), "{:?}", reactive.last_error);
            let left = semantic_leaves(&immediate.adapter.semantics(&immediate.engine, 0));
            let right = semantic_leaves(&reactive.adapter.semantics(&reactive.engine, 0));
            assert_eq!(left.len(), right.len(), "{page:?} width={width}");
            for (left, right) in left.iter().zip(&right) {
                assert_eq!(
                    (left.0, &left.1, &left.2, left.3),
                    (right.0, &right.1, &right.2, right.3),
                    "{page:?} width={width}"
                );
                for (a, b) in left.4.into_iter().zip(right.4) {
                    assert!(
                        (a - b).abs() < 1.,
                        "{page:?} width={width} {:?}: {:?} vs {:?}",
                        left.1,
                        left.4,
                        right.4
                    );
                }
            }
        }
    }
}
