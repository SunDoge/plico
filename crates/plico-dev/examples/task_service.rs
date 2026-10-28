//! Headless end-to-end UI/task bridge. Replace the queue with your executor's
//! spawn method and the mock operation with an actual async request.
use plico::{PathKey, ViewNode, prelude::*};
use plico_dev::{
    Request,
    services::{HostService, ServiceApp, Services, render},
    tasks::{TaskCommand, TaskFuture, TaskService, TaskStatus},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default, Serialize, Deserialize)]
pub struct Demo;
impl ServiceApp for Demo {
    type Snapshot = TaskStatus<String, String>;
    type Command = TaskCommand<String>;

    #[component]
    fn show(&mut self, cx: &mut UiContext, services: &mut Services<Self::Snapshot, Self::Command>) {
        let mut save = services.task();
        if button(cx).text("Save").disabled(save.pending()).clicked() {
            save.start("document".to_owned());
        }
        match save.status() {
            TaskStatus::Idle => {
                text(cx, "Ready to save");
            }
            TaskStatus::Pending => {
                text(cx, "Saving...");
                if button(cx).text("Cancel").clicked() {
                    save.cancel();
                }
            }
            TaskStatus::Ready(Ok(result)) => {
                text(cx, result);
            }
            TaskStatus::Ready(Err(error)) => {
                text(cx, error);
            }
            TaskStatus::Cancelled => {
                text(cx, "Cancelled");
            }
        }
    }
}
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
fn has(nodes: &[ViewNode], label: &str) -> bool {
    find(nodes, label, &mut Vec::new()).is_some()
}

pub fn run() {
    let jobs = Rc::new(RefCell::new(Vec::<TaskFuture>::new()));
    let queue = jobs.clone();
    let mut host = TaskService::new(
        |input: String| async move {
            if input == "fail" {
                Err("Save failed".to_owned())
            } else {
                Ok(format!("Saved {input}"))
            }
        },
        move |job| queue.borrow_mut().push(job),
    );
    let draw = |host: &TaskService<String, String, String>, clicks| {
        render::<Demo>(Request {
            resume: true,
            services: serde_json::to_value(host.snapshot()).unwrap(),
            clicks,
            ..Default::default()
        })
        .unwrap()
    };
    let flush = || {
        for mut job in jobs.borrow_mut().drain(..) {
            // Mock operations are immediately ready; a real executor schedules
            // pending Futures by their Waker rather than using this demo pump.
            assert_eq!(
                job.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(())
            );
        }
    };
    let frame = draw(&host, vec![]);
    assert!(has(&frame.nodes, "Ready to save"));
    let path = find(&frame.nodes, "Save", &mut Vec::new()).unwrap();
    let frame = draw(&host, vec![path]);
    assert_eq!(frame.commands.len(), 1);
    host.send(serde_json::from_value(frame.commands[0].clone()).unwrap());
    let frame = draw(&host, vec![]);
    assert!(has(&frame.nodes, "Saving..."));
    flush();
    assert!(host.poll());
    assert!(has(&draw(&host, vec![]).nodes, "Saved document"));

    host.send(TaskCommand::Start("fail".into()));
    flush();
    assert!(host.poll());
    assert!(has(&draw(&host, vec![]).nodes, "Save failed"));

    host.send(TaskCommand::Start("late".into()));
    let frame = draw(&host, vec![]);
    let path = find(&frame.nodes, "Cancel", &mut Vec::new()).unwrap();
    let frame = draw(&host, vec![path]);
    assert_eq!(frame.commands.len(), 1);
    host.send(serde_json::from_value(frame.commands[0].clone()).unwrap());
    flush();
    assert!(!host.poll());
    assert!(has(&draw(&host, vec![]).nodes, "Cancelled"));

    // Recreate the UI session while the host task remains pending.
    host.send(TaskCommand::Start("after reload".into()));
    let frame = render::<Demo>(Request {
        services: serde_json::to_value(host.snapshot()).unwrap(),
        ..Default::default()
    })
    .unwrap();
    assert!(has(&frame.nodes, "Saving..."));
    flush();
    assert!(host.poll());
    assert!(has(&draw(&host, vec![]).nodes, "Saved after reload"));
    println!("UI task bridge: start, pending, success, failure, cancel and reload passed.");
}
fn main() {
    run();
}
#[test]
fn end_to_end() {
    run();
}
