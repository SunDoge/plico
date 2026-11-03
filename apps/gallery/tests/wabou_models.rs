use gallery::{
    data_table::{Sort, TableState},
    tree_view::{TreeState, WORKSPACE},
};
#[test]
fn table_sort_filter_and_selection_are_business_keyed() {
    let mut state = TableState::default();
    assert_eq!(state.visible().first().unwrap().id, "router");
    state.sort_by(Sort::Project);
    assert_eq!(state.visible().first().unwrap().id, "a11y");
    state.sort_by(Sort::Project);
    assert_eq!(state.visible().first().unwrap().id, "virtual");
    state.query = "  mINA ".into();
    assert_eq!(state.visible().len(), 2);
    state.select_visible(true);
    state.query = "Arun".into();
    state.select_visible(true);
    assert_eq!(state.selected.len(), 4);
    state.select_visible(false);
    assert_eq!(
        state
            .selected
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a11y", "router"]
    );
    state.query = "not a project".into();
    assert!(state.visible().is_empty());
    state.select_visible(true);
    assert_eq!(state.selected.len(), 2);
    let restored: TableState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
}
#[test]
fn tree_collapse_keeps_selection_and_disabled_nodes_reject_changes() {
    let mut state = TreeState::default();
    assert!(state.visible(WORKSPACE).iter().any(|(n, _)| n.id == "core"));
    assert!(state.toggle(WORKSPACE, "crates"));
    assert!(!state.visible(WORKSPACE).iter().any(|(n, _)| n.id == "core"));
    assert_eq!(state.selected.as_deref(), Some("core"));
    assert!(!state.select(WORKSPACE, "target"));
    assert!(!state.select(WORKSPACE, "unknown"));
    assert!(!state.toggle(WORKSPACE, "core"));
    state.expand_all(WORKSPACE);
    assert!(
        state
            .visible(WORKSPACE)
            .iter()
            .any(|(n, _)| n.id == "gallery")
    );
    assert!(state.select(WORKSPACE, "gallery"));
    let restored: TreeState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
}
