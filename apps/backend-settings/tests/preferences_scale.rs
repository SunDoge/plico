use backend_settings::preferences_regions::scale::{
    FullRegionPanels, ImmediatePanels, region_panels,
};
use plico_renderer::{
    FrameSource, SemanticAction, SemanticRole, SemanticSnapshot, TextContext, UiEvent,
};
use plico_runtime::{Source, View};
fn prepare<V: View>(s: &mut Source<V>, text: &mut TextContext, n: usize) {
    s.experiment_prepare_without_scene(text, 900, (n * 780) as u32);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
fn targets(tree: &SemanticSnapshot, role: SemanticRole, label: &str) -> Vec<u64> {
    tree.exposed_nodes()
        .into_iter()
        .filter(|n| n.role == role && n.label.as_deref() == Some(label))
        .map(|n| n.id)
        .collect()
}
fn equal<A: View, B: View>(a: &Source<A>, b: &Source<B>) {
    let leaves = |tree: SemanticSnapshot| {
        tree.exposed_nodes()
            .into_iter()
            .filter(|n| {
                matches!(
                    n.role,
                    SemanticRole::Label
                        | SemanticRole::Button
                        | SemanticRole::CheckBox
                        | SemanticRole::TextInput
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
    let a_leaves = leaves(a.adapter.semantics(&a.engine, 0));
    let b_leaves = leaves(b.adapter.semantics(&b.engine, 0));
    assert_eq!(a_leaves.len(), b_leaves.len());
    for (a, b) in a_leaves.iter().zip(b_leaves.iter()) {
        assert_eq!((a.0, &a.1, &a.2, a.3, a.4), (b.0, &b.1, &b.2, b.3, b.4));
        assert!(
            a.5.iter().zip(b.5).all(|(a, b)| (*a - b).abs() < 1.),
            "{:?}: {:?} != {:?}",
            a.1,
            a.5,
            b.5
        );
    }
    assert_eq!(a.engine.theme(), b.engine.theme());
}
fn click<V: View>(source: &mut Source<V>, role: SemanticRole, label: &str, k: usize) {
    let ids = targets(&source.adapter.semantics(&source.engine, 0), role, label);
    assert!(ids.len() >= k);
    for id in ids.into_iter().take(k) {
        assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
    }
}
#[test]
fn batched_updates_match_all_forms_including_unmodified_siblings() {
    for n in [1, 4, 16] {
        let mut a = Source::new(ImmediatePanels::new(n));
        let mut b = Source::new(region_panels(n));
        a.set_reduced_motion(true);
        b.set_reduced_motion(true);
        let (mut at, mut bt) = (TextContext::new(), TextContext::new());
        prepare(&mut a, &mut at, n);
        prepare(&mut b, &mut bt, n);
        equal(&a, &b);
        let initial_nodes = b.experiment_live_counts();
        let original_inputs = targets(
            &b.adapter.semantics(&b.engine, 0),
            SemanticRole::TextInput,
            "Workspace name",
        );
        assert_eq!(original_inputs.len(), n);
        let mut counts = vec![1, n.div_ceil(2), n];
        counts.dedup();
        for k in counts {
            for label in ["Enable notifications", "Show advanced options"] {
                for turn in 0..2 {
                    click(&mut a, SemanticRole::CheckBox, label, k);
                    click(&mut b, SemanticRole::CheckBox, label, k);
                    prepare(&mut a, &mut at, n);
                    prepare(&mut b, &mut bt, n);
                    equal(&a, &b);
                    let tree = b.adapter.semantics(&b.engine, 0);
                    let checked = tree
                        .exposed_nodes()
                        .into_iter()
                        .filter(|node| {
                            node.role == SemanticRole::CheckBox
                                && node.label.as_deref() == Some(label)
                                && node.states.checked
                                    == Some(plico_renderer::SemanticToggleState::On)
                        })
                        .count();
                    let expected = match (label, turn) {
                        ("Enable notifications", 0) => n - k,
                        ("Enable notifications", _) => n,
                        (_, 0) => k,
                        _ => 0,
                    };
                    assert_eq!(checked, expected, "the changed subset must be exact");
                }
            }
            // Multiple edited buffers before a single frame; only one is focused.
            let ai = targets(
                &a.adapter.semantics(&a.engine, 0),
                SemanticRole::TextInput,
                "Workspace name",
            );
            let bi = targets(
                &b.adapter.semantics(&b.engine, 0),
                SemanticRole::TextInput,
                "Workspace name",
            );
            for index in 0..k {
                assert!(a.handle_semantic_action(SemanticAction::Focus { target: ai[index] }));
                assert!(b.handle_semantic_action(SemanticAction::Focus { target: bi[index] }));
                assert!(a.handle_event(UiEvent::TextInput("x".into())).handled);
                assert!(b.handle_event(UiEvent::TextInput("x".into())).handled);
            }
            prepare(&mut a, &mut at, n);
            prepare(&mut b, &mut bt, n);
            equal(&a, &b);
            click(&mut a, SemanticRole::Button, "Save changes", k);
            click(&mut b, SemanticRole::Button, "Save changes", k);
            prepare(&mut a, &mut at, n);
            prepare(&mut b, &mut bt, n);
            equal(&a, &b);
            assert_eq!(
                targets(
                    &b.adapter.semantics(&b.engine, 0),
                    SemanticRole::TextInput,
                    "Workspace name"
                ),
                original_inputs
            );
            assert_eq!(b.experiment_live_counts(), initial_nodes);
        }
        for _ in 0..2 {
            click(&mut a, SemanticRole::CheckBox, "Dark theme", 1);
            click(&mut b, SemanticRole::CheckBox, "Dark theme", 1);
            prepare(&mut a, &mut at, n);
            prepare(&mut b, &mut bt, n);
            equal(&a, &b);
        }
    }
}

#[test]
fn full_region_control_has_identical_tree_and_batch_results() {
    let n = 4;
    let mut local = Source::new(region_panels(n));
    let mut full = Source::new(FullRegionPanels::new(n));
    local.set_reduced_motion(true);
    full.set_reduced_motion(true);
    let (mut lt, mut ft) = (TextContext::new(), TextContext::new());
    prepare(&mut local, &mut lt, n);
    prepare(&mut full, &mut ft, n);
    assert_eq!(
        local.experiment_live_counts(),
        full.experiment_live_counts()
    );
    for label in [
        "Enable notifications",
        "Show advanced options",
        "Show advanced options",
    ] {
        click(&mut local, SemanticRole::CheckBox, label, 3);
        click(&mut full, SemanticRole::CheckBox, label, 3);
        prepare(&mut local, &mut lt, n);
        prepare(&mut full, &mut ft, n);
        equal(&local, &full);
        assert_eq!(
            local.experiment_live_counts(),
            full.experiment_live_counts()
        );
    }
    click(&mut local, SemanticRole::Button, "Save changes", 4);
    click(&mut full, SemanticRole::Button, "Save changes", 4);
    prepare(&mut local, &mut lt, n);
    prepare(&mut full, &mut ft, n);
    equal(&local, &full);
}
