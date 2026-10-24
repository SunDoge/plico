fn snapshot(value: &impl serde::Serialize) -> plico_dev::StateSnapshot {
    plico_dev::StateSnapshot::encode(value).unwrap()
}
use plico_core::{PathKey, ViewNode, prelude::*};
use plico_dev::{
    Request,
    services::{ServiceApp, Services, render},
};
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
struct Projection {
    progress: u8,
}
#[derive(Debug, Serialize, Deserialize)]
enum Command {
    Start,
}
#[derive(Default, Serialize, Deserialize)]
struct App {
    count: u32,
}
impl ServiceApp for App {
    type Snapshot = Projection;
    type Command = Command;
    #[component]
    fn show(&mut self, cx: &mut UiContext, services: &mut Services<Projection, Command>) {
        plico_core::text!(cx, "Progress: {}", services.snapshot().progress);
        plico_core::text!(cx, "Count: {}", self.count);
        if button(cx).text("Start").clicked() {
            self.count += 1;
            services.send(Command::Start);
        }
    }
}
fn find(nodes: &[ViewNode], path: &mut Vec<PathKey>) -> Option<Vec<PathKey>> {
    for n in nodes {
        path.push(n.key.clone());
        if n.text == "Start" {
            return Some(path.clone());
        }
        if let Some(found) = find(&n.children, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}
#[test]
fn typed_snapshot_is_not_checkpointed_and_commands_survive_settling_exactly_once() {
    let first = render::<App>(Request {
        services: serde_json::json!({"progress":73}),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        first.state.decode::<serde_json::Value>().unwrap(),
        serde_json::json!({"count":0})
    );
    assert!(first.commands.is_empty());
    let path = find(&first.nodes, &mut Vec::new()).unwrap();
    let clicked = render::<App>(Request {
        state: first.state,
        services: serde_json::json!({"progress":73}),
        clicks: vec![path],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        clicked.state.decode::<serde_json::Value>().unwrap(),
        serde_json::json!({"count":1})
    );
    assert_eq!(clicked.commands, vec![serde_json::json!("Start")]);
    assert!(clicked.nodes.iter().any(|n| n.text == "Count: 1"));
    let next = render::<App>(Request {
        state: clicked.state,
        services: serde_json::json!({"progress":100}),
        ..Default::default()
    })
    .unwrap();
    assert!(next.commands.is_empty());
    assert!(next.nodes.iter().any(|n| n.text == "Progress: 100"));
}
#[test]
fn incompatible_ui_resets_independently_of_host_snapshot_and_invalid_snapshot_fails() {
    let reset = render::<App>(Request {
        state: snapshot(&serde_json::json!({"count":"old schema"})),
        services: serde_json::json!({"progress":48}),
        ..Default::default()
    })
    .unwrap();
    assert!(reset.reset.is_some());
    assert_eq!(
        reset.state.decode::<serde_json::Value>().unwrap()["count"],
        0
    );
    assert!(reset.nodes.iter().any(|n| n.text == "Progress: 48"));
    assert!(reset.commands.is_empty());
    assert!(
        render::<App>(Request {
            services: serde_json::json!({"progress":"bad"}),
            ..Default::default()
        })
        .is_err()
    );
}

#[derive(Default, Serialize, Deserialize)]
struct FailingApp;
impl ServiceApp for FailingApp {
    type Snapshot = Projection;
    type Command = Command;
    fn show(&mut self, _cx: &mut UiContext, services: &mut Services<Projection, Command>) {
        services.send(Command::Start);
        panic!("synthetic declaration failure");
    }
}
#[test]
fn a_panicking_declaration_cannot_publish_its_outbox() {
    let bytes = serde_json::to_vec(&Request::default()).unwrap();
    let reply = plico_dev::services::dispatch::<FailingApp>(bytes.into_iter().collect());
    assert_eq!(reply.status, 1);
    let message = String::from_utf8(reply.bytes.into_iter().collect()).unwrap();
    assert!(message.contains("panicked"));
    assert!(!message.contains("Start"));
}
