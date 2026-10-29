#![cfg(feature = "regions")]
use backend_settings::preferences_regions::{ImmediatePreferences, RegionPreferences};
use plico::{Theme, ViewNode};
use plico_renderer::{SemanticRole, TextContext};
use plico_runtime::{
    Source, View,
    testing::{Command, dispatch},
};
struct Harness<V: View> {
    source: Source<V>,
    text: TextContext,
    width: u32,
}
impl<V: View + Default> Harness<V> {
    fn new(width: u32) -> Self {
        let mut source = Source::new(V::default());
        source.set_reduced_motion(true);
        let mut app = Self {
            source,
            text: TextContext::new(),
            width,
        };
        app.frame();
        app
    }
    fn frame(&mut self) {
        self.source
            .experiment_prepare_without_scene(&mut self.text, self.width, 780);
        assert!(
            self.source.last_error.is_none(),
            "{:?}",
            self.source.last_error
        );
    }
    fn command(&mut self, command: Command) {
        dispatch(&mut self.source, &mut self.text, &command).unwrap();
        self.frame();
    }
    fn has(&self, label: &str) -> bool {
        fn find(nodes: &[ViewNode], label: &str) -> bool {
            nodes
                .iter()
                .any(|n| n.text == label || find(&n.children, label))
        }
        find(&self.source.engine.snapshot(), label)
    }
}
fn click(role: &str, label: &str) -> Command {
    Command::Click {
        role: role.into(),
        label: label.into(),
        occurrence: None,
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
fn equivalent(a: &Harness<ImmediatePreferences>, b: &Harness<RegionPreferences>) {
    let left = a.source.adapter.semantics(&a.source.engine, 0);
    let right = b.source.adapter.semantics(&b.source.engine, 0);
    let leaves = |tree: &plico_renderer::SemanticSnapshot| {
        tree.exposed_nodes()
            .into_iter()
            .filter(|n| {
                matches!(
                    n.role,
                    SemanticRole::Label
                        | SemanticRole::Heading
                        | SemanticRole::TextInput
                        | SemanticRole::Button
                        | SemanticRole::CheckBox
                )
            })
            .map(|n| {
                (
                    n.role,
                    n.label.clone(),
                    n.value.clone(),
                    n.disabled,
                    n.states,
                    n.bounds,
                )
            })
            .collect::<Vec<_>>()
    };
    let left = leaves(&left);
    let right = leaves(&right);
    assert_eq!(left.len(), right.len());
    for (a, b) in left.iter().zip(&right) {
        assert_eq!((a.0, &a.1, &a.2, a.3, a.4), (b.0, &b.1, &b.2, b.3, b.4));
        for (x, y) in a.5.into_iter().zip(b.5) {
            assert!(
                (x - y).abs() < 1.,
                "geometry {:?}: {:?} vs {:?}",
                a.1,
                a.5,
                b.5
            );
        }
    }
    assert_eq!(a.source.engine.theme(), b.source.engine.theme());
}
#[test]
fn complete_preferences_page_matches_sync_controls_layout_and_state() {
    for width in [900, 480] {
        let mut immediate = Harness::<ImmediatePreferences>::new(width);
        let mut regions = Harness::<RegionPreferences>::new(width);
        equivalent(&immediate, &regions);
        for command in [
            click("checkbox", "Enable notifications"),
            click("checkbox", "Show advanced options"),
            Command::Focus {
                role: "textbox".into(),
                label: "Workspace name".into(),
                occurrence: None,
            },
            select_all(),
            Command::Text {
                text: "本地 workspace".into(),
            },
            click("button", "Save changes"),
            click("button", "Save changes"),
            click("button", "Reset name"),
            click("checkbox", "Show advanced options"),
            click("checkbox", "Reduce motion"),
            click("checkbox", "Dark theme"),
            click("checkbox", "Dark theme"),
        ] {
            immediate.command(command.clone());
            regions.command(command);
            equivalent(&immediate, &regions);
        }
        assert!(regions.has("Current name: Plico workspace"));
        assert!(regions.has("Saved 2: 本地 workspace"));
        assert!(!regions.has("Advanced options"));
    }
}
#[test]
fn ordinary_updates_skip_page_roots_and_keep_editor_identity() {
    let mut regions = Harness::<RegionPreferences>::new(900);
    let before: std::collections::HashMap<_, _> = regions
        .source
        .view
        .regions
        .region_runs()
        .into_iter()
        .collect();
    let input = regions
        .source
        .adapter
        .semantics(&regions.source.engine, 0)
        .node_by_role(SemanticRole::TextInput, "Workspace name", None)
        .unwrap()
        .id;
    regions.command(click("checkbox", "Enable notifications"));
    let changed = regions
        .source
        .view
        .regions
        .region_runs()
        .into_iter()
        .filter(|(id, runs)| *runs != before[id])
        .count();
    assert_eq!(changed, 1, "only the toggle's watch executes");
    let after = regions
        .source
        .adapter
        .semantics(&regions.source.engine, 0)
        .node_by_role(SemanticRole::TextInput, "Workspace name", None)
        .unwrap()
        .id;
    assert_eq!(input, after);
    regions.command(Command::Focus {
        role: "textbox".into(),
        label: "Workspace name".into(),
        occurrence: None,
    });
    regions.command(select_all());
    let before: std::collections::HashMap<_, _> = regions
        .source
        .view
        .regions
        .region_runs()
        .into_iter()
        .collect();
    regions.command(Command::Text {
        text: "New name".into(),
    });
    let changed: Vec<_> = regions
        .source
        .view
        .regions
        .region_runs()
        .into_iter()
        .filter(|(id, runs)| *runs != before[id])
        .map(|(id, _)| regions.source.engine.node(id).unwrap().kind)
        .collect();
    assert_eq!(
        changed.len(),
        2,
        "only editor watch and preview binding execute"
    );
    assert_eq!(
        changed
            .iter()
            .filter(|kind| **kind == plico::Kind::Text)
            .count(),
        1
    );
    assert_eq!(
        changed
            .iter()
            .filter(|kind| **kind == plico::Kind::Column)
            .count(),
        1
    );

    assert!(regions.has("Current name: New name"));
}
#[test]
fn theme_event_commits_new_theme_without_a_second_external_frame() {
    let mut regions = Harness::<RegionPreferences>::new(900);
    regions.command(click("checkbox", "Dark theme"));
    assert_eq!(regions.source.engine.theme(), Theme::dark());
    assert!(regions.has("Profile / 工作区"));
    regions.command(click("checkbox", "Dark theme"));
    assert_eq!(regions.source.engine.theme(), Theme::light());
}

#[test]
fn page_checkpoint_restores_marked_state_without_persisting_editor_buffer() {
    let mut original = Harness::<RegionPreferences>::new(900);
    original.command(Command::Focus {
        role: "textbox".into(),
        label: "Workspace name".into(),
        occurrence: None,
    });
    original.command(select_all());
    original.command(Command::Text {
        text: "Saved workspace".into(),
    });
    original.command(click("button", "Save changes"));
    original.command(click("checkbox", "Show advanced options"));
    original.command(click("checkbox", "Enable notifications"));
    original.command(click("checkbox", "Dark theme"));
    let checkpoint = original.source.view.regions.checkpoint().unwrap();
    drop(original);
    let mut view = RegionPreferences::default();
    view.regions.restore_checkpoint(checkpoint);
    let mut restored = Harness {
        source: Source::new(view),
        text: TextContext::new(),
        width: 900,
    };
    restored.source.set_reduced_motion(true);
    restored.frame();
    assert_eq!(restored.source.engine.theme(), Theme::dark());
    assert!(restored.has("Saved 1: Saved workspace"));
    assert!(restored.has("Current name: Plico workspace"));
    assert!(restored.has("Advanced options"));
    assert_eq!(restored.source.view.regions.restore_stats().restored, 5);
}
