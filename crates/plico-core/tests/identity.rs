use plico_core::{Engine, Entity, Ui, component};
fn named(engine: &Engine, label: &str) -> Entity {
    fn visit(engine: &Engine, e: Entity, label: &str) -> Option<Entity> {
        let node = engine.node(e)?;
        if node.text == label {
            return Some(e);
        }
        node.children.iter().find_map(|e| visit(engine, *e, label))
    }
    visit(engine, engine.root(), label).unwrap()
}
#[component]
fn conditional(ui: &mut Ui<'_>, extra: bool, clicks: &mut u32, enabled: &mut bool) {
    ui.column().children(|ui| {
        if extra {
            ui.text("Optional");
        }
        if ui.button("Keep").clicked() {
            *clicks += 1;
        }
        ui.switch("Enabled", enabled);
        if extra {
            let _ = ui.row().children(|ui| {
                ui.text("Optional row");
            });
        }
        let _ = ui.row().children(|ui| {
            ui.text("Last");
        });
    })
}
#[test]
fn conditional_siblings_and_nested_views_keep_identity_and_queued_clicks() {
    let mut e = Engine::default();
    let mut clicks = 0;
    let mut enabled = false;
    e.frame_declarations(|ui| conditional(ui, false, &mut clicks, &mut enabled))
        .unwrap();
    let keep = named(&e, "Keep");
    let last = named(&e, "Last");
    let toggle = named(&e, "Enabled");
    e.queue_click(keep);
    e.queue_click(toggle);
    e.frame_declarations(|ui| conditional(ui, true, &mut clicks, &mut enabled))
        .unwrap();
    assert_eq!(named(&e, "Keep"), keep);
    assert_eq!(named(&e, "Last"), last);
    assert_eq!(clicks, 1);
    assert!(enabled);
    e.frame_declarations(|ui| conditional(ui, false, &mut clicks, &mut enabled))
        .unwrap();
    assert_eq!(clicks, 1);
}
#[component]
fn child(ui: &mut Ui<'_>, label: &str) {
    ui.column().children(|ui| {
        ui.button(label);
    })
}
#[component]
fn repeated(ui: &mut Ui<'_>, first: bool) {
    if first {
        child(ui, "A");
    }
    child(ui, "B");
    child(ui, "C");
}
#[test]
fn repeated_component_calls_have_distinct_scopes_and_survive_branches() {
    let mut e = Engine::default();
    e.frame_declarations(|ui| repeated(ui, true)).unwrap();
    let b = named(&e, "B");
    let c = named(&e, "C");
    assert_ne!(b, c);
    e.frame_declarations(|ui| repeated(ui, false)).unwrap();
    assert_eq!(named(&e, "B"), b);
    assert_eq!(named(&e, "C"), c);
}
#[component]
fn list(ui: &mut Ui<'_>, ids: &[u64]) {
    for id in ids {
        ui.keyed(*id, |ui| child(ui, if *id == 1 { "One" } else { "Two" }));
    }
}
#[test]
fn explicit_business_keys_keep_component_instances_when_list_reorders() {
    let mut e = Engine::default();
    e.frame_declarations(|ui| list(ui, &[1, 2])).unwrap();
    let one = named(&e, "One");
    let two = named(&e, "Two");
    e.frame_declarations(|ui| list(ui, &[2, 1])).unwrap();
    assert_eq!(named(&e, "One"), one);
    assert_eq!(named(&e, "Two"), two);
    e.frame_declarations(|ui| list(ui, &[2])).unwrap();
    assert!(e.node(one).is_none());
    e.frame_declarations(|ui| list(ui, &[2, 1])).unwrap();
    assert_ne!(named(&e, "One"), one);
}
#[component]
fn manual(ui: &mut Ui<'_>, ids: &[u64]) {
    for id in ids {
        let _ = ui.row().key(*id).children(|ui| {
            ui.text(if *id == 1 { "One" } else { "Two" });
        });
    }
}
#[test]
fn builder_key_overrides_generated_id_and_survives_reorder() {
    let mut e = Engine::default();
    e.frame_declarations(|ui| manual(ui, &[1, 2])).unwrap();
    let one = named(&e, "One");
    assert!(e.find(e.root(), 1u64).is_some());
    e.frame_declarations(|ui| manual(ui, &[2, 1])).unwrap();
    assert_eq!(named(&e, "One"), one);
}
#[component]
fn missing_key(ui: &mut Ui<'_>) {
    for _ in 0..2 {
        ui.text("Repeated");
    }
}
#[test]
fn unkeyed_loop_reports_duplicate_instead_of_silently_aliasing_nodes() {
    let mut e = Engine::default();
    assert!(e.frame_declarations(missing_key).is_err());
}
#[test]
fn snapshot_keys_restore_clicks_in_a_fresh_engine() {
    let mut e = Engine::default();
    let mut clicks = 0;
    let mut enabled = false;
    e.frame_declarations(|ui| conditional(ui, false, &mut clicks, &mut enabled))
        .unwrap();
    let tree = e.snapshot();
    let mut path = vec![tree[0].key.clone()];
    path.push(
        tree[0]
            .children
            .iter()
            .find(|n| n.text == "Keep")
            .unwrap()
            .key
            .clone(),
    );
    let mut fresh = Engine::default();
    fresh.queue_click_path(path);
    fresh
        .frame_declarations(|ui| conditional(ui, true, &mut clicks, &mut enabled))
        .unwrap();
    assert_eq!(clicks, 1);
}

struct Sections;
impl Sections {
    #[component]
    fn item(&self, ui: &mut Ui<'_>, label: &str) {
        ui.text(label);
    }
    #[component]
    fn show(&self, ui: &mut Ui<'_>) {
        self.item(ui, "Method A");
        self.item(ui, "Method B");
    }
}
#[test]
fn repeated_component_methods_are_scoped_at_their_invocations() {
    let mut e = Engine::default();
    e.frame_declarations(|ui| Sections.show(ui)).unwrap();
    assert_ne!(named(&e, "Method A"), named(&e, "Method B"));
}
#[test]
fn component_children_keep_static_nodes_when_a_branch_changes() {
    #[component]
    fn render(ui: &mut Ui<'_>, extra: bool) {
        ui.column().children(|ui| {
            if extra {
                ui.text("Extra");
            }
            ui.text("Stable");
        })
    }
    let mut e = Engine::default();
    e.frame_declarations(|ui| render(ui, false)).unwrap();
    let id = named(&e, "Stable");
    e.frame_declarations(|ui| render(ui, true)).unwrap();
    assert_eq!(named(&e, "Stable"), id);
}

#[test]
fn changes_distinguish_view_requests_from_geometry_and_paint_diff() {
    use plico_core::{Changes, Color};
    let mut e = Engine::default();
    fn content(ui: &mut Ui<'_>, height: f32, color: u32) {
        ui.text("Box")
            .key("box")
            .height(height)
            .background(Color::Literal(color));
    }
    e.frame_declarations(|ui| content(ui, 40., 0x112233))
        .unwrap();
    assert!(
        e.changes()
            .contains(Changes::LAYOUT | Changes::PAINT | Changes::SEMANTICS)
    );
    assert!(!e.changes().contains(Changes::VIEW));
    e.frame_declarations(|ui| content(ui, 40., 0x112233))
        .unwrap();
    assert!(e.changes().is_empty());
    e.frame_declarations(|ui| content(ui, 40., 0x445566))
        .unwrap();
    assert!(e.changes().contains(Changes::PAINT));
    assert!(!e.changes().intersects(Changes::LAYOUT | Changes::VIEW));
    e.frame_declarations(|ui| content(ui, 60., 0x445566))
        .unwrap();
    assert!(e.changes().contains(Changes::LAYOUT));
}
