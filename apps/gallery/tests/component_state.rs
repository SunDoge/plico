//! Local presentation values survive real route unmounts and module checkpoints.
use gallery::{Gallery, Page};
use plico::{PathKey, Router, ViewNode};
use plico_dev::{Frame, Request, StateSnapshot, render};
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
fn has(frame: &Frame, label: &str) -> bool {
    find(&frame.nodes, label, &mut Vec::new()).is_some()
}
fn click(frame: &Frame, label: &str) -> Frame {
    let path =
        find(&frame.nodes, label, &mut Vec::new()).unwrap_or_else(|| panic!("missing {label}"));
    render::<Gallery>(Request {
        resume: true,
        clicks: vec![path],
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn channel_and_shader_controls_survive_navigation_and_reload() {
    let app = Gallery {
        router: Router::new(Page::Controls),
        ..Default::default()
    };
    let frame = render::<Gallery>(Request {
        state: StateSnapshot::encode(&app).unwrap(),
        ..Default::default()
    })
    .unwrap();
    assert!(has(&frame, "Recommended"));
    let frame = click(&frame, "Beta");
    assert!(has(&frame, "Preview"));
    let frame = click(&frame, "Layout");
    let frame = click(&frame, "Controls");
    assert!(has(&frame, "Preview"));
    let frame = render::<Gallery>(Request {
        state: frame.state,
        ..Default::default()
    })
    .unwrap();
    assert!(has(&frame, "Preview"));
    let frame = click(&frame, "Motion");
    let frame = click(&frame, "Pause shader");
    assert!(has(&frame, "Resume shader"));
    let frame = click(&frame, "Layout");
    let frame = click(&frame, "Motion");
    assert!(has(&frame, "Resume shader"));
}
