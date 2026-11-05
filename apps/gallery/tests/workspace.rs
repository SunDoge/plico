use gallery::workspace::Workspace;
use plico::{Engine, Entity, Kind};
fn find(engine: &Engine, root: Entity, kind: Kind, label: &str) -> Option<Entity> {
    let node = engine.node(root)?;
    let name = engine.accessible_name(root);
    if node.kind == kind && name.as_deref() == Some(label) {
        return Some(root);
    }
    node.children
        .iter()
        .find_map(|&id| find(engine, id, kind, label))
}
fn click(engine: &mut Engine, workspace: &mut Workspace, kind: Kind, label: &str) {
    let target = find(engine, engine.root(), kind, label).expect(label);
    engine.queue_click(target);
    engine.frame(|cx| workspace.show(cx)).unwrap();
}
#[test]
fn preferences_remain_project_local_and_archive_restore_keeps_saved_values() {
    let mut engine = Engine::default();
    engine.set_viewport(1120., 820.).unwrap();
    let mut workspace = Workspace::default();
    engine.frame(|cx| workspace.show(cx)).unwrap();
    click(
        &mut engine,
        &mut workspace,
        Kind::Checkbox,
        "Save automatically",
    );
    assert!(workspace.projects[0].draft.autosave);
    assert!(!workspace.projects[0].saved.autosave);
    click(
        &mut engine,
        &mut workspace,
        Kind::ButtonBase,
        "Save changes",
    );
    assert!(workspace.projects[0].saved.autosave);
    click(&mut engine, &mut workspace, Kind::ButtonBase, "Docs");
    assert_eq!(workspace.selected, 2);
    assert!(!workspace.projects[1].draft.autosave);
    for _ in 0..3 {
        click(
            &mut engine,
            &mut workspace,
            Kind::ButtonBase,
            "Archive project",
        );
    }
    engine.frame(|cx| workspace.show(cx)).unwrap();
    assert!(workspace.projects.iter().all(|p| p.archived));
    click(
        &mut engine,
        &mut workspace,
        Kind::ButtonBase,
        "Restore projects",
    );
    assert!(workspace.projects.iter().all(|p| !p.archived));
    assert!(workspace.projects[0].saved.autosave);
    assert_eq!(workspace.selected, 1);
}
