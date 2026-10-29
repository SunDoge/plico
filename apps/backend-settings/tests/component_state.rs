use backend_settings::{Page, Settings};
use plico::{Engine, PathKey, ViewNode};
fn find(nodes: &[ViewNode], label: &str, path: &mut Vec<PathKey>) -> Option<Vec<PathKey>> {
    for node in nodes {
        path.push(node.key.clone());
        if node.text == label {
            return Some(path.clone());
        }
        if let Some(found) = find(&node.children, label, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}
fn events(nodes: &[ViewNode], result: &mut Vec<String>) {
    for node in nodes {
        if node.text.starts_with("Sync checkpoint ") {
            result.push(node.text.clone());
        }
        events(&node.children, result);
    }
}
#[test]
fn activity_sort_survives_page_unmount() {
    let mut engine = Engine::default();
    let mut app = Settings::default();
    app.go(Page::Activity);
    engine.declare_until_settled(&mut app).unwrap();
    let path = find(&engine.snapshot(), "Reverse order", &mut Vec::new()).unwrap();
    engine.queue_click_path(path);
    engine.declare_until_settled(&mut app).unwrap();
    let mut reversed = Vec::new();
    events(&engine.snapshot(), &mut reversed);
    assert_eq!(reversed.first().unwrap(), "Sync checkpoint 30");
    app.go(Page::Preferences);
    engine.declare_until_settled(&mut app).unwrap();
    app.go(Page::Activity);
    engine.declare_until_settled(&mut app).unwrap();
    let mut after = Vec::new();
    events(&engine.snapshot(), &mut after);
    assert_eq!(after, reversed);
}
