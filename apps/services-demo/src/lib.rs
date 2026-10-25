//! UI + value protocol only. The worker is compiled into main.rs, never this module.
pub mod icons;
use plico_core::prelude::*;
use plico_dev::services::{ServiceApp, Services};
use serde::{Deserialize, Serialize};
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Cancelled,
    Failed,
}
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub instance: u64,
    pub task_id: u64,
    pub started: u64,
    pub completed: u64,
    pub progress: u8,
    pub status: TaskStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskCommand {
    Start,
    Cancel,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Application {
    pub counter: u64,
    #[cfg(feature = "schema-reset-fixture")]
    pub incompatible_fixture: bool,
}
impl ServiceApp for Application {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext, services: &mut Services<TaskSnapshot, TaskCommand>) {
        let task = services.snapshot_shared();
        let running = task.status == TaskStatus::Running;
        column(cx).padding(28).gap(20).width_percent(100.).height_percent(100.) + |cx| {
            row(cx).align(Align::Center).gap(12) + |cx| {
                icon(cx, &crate::icons::LAYERS).size(26., 26.).color(Color::Accent);
                column(cx).gap(5).children(|cx| {
                    text(cx, "Task workspace").font_size(24).weight(750);
                    text(cx, "Start a task, then edit this view while it runs.").font_size(12)
                        .color(Color::Muted);
                })
            };
            column(cx).padding(20).gap(16).radius(10.).background(Color::Panel).border(1., Color::Border) + |cx| {
                row(cx).justify(Justify::SpaceBetween) + |cx| {
                    text(cx, "Background task").font_size(16).weight(650);
                    text(
                        cx,
                        match task.status {
                            TaskStatus::Idle => "Ready",
                            TaskStatus::Running => "Running",
                            TaskStatus::Completed => "Completed",
                            TaskStatus::Cancelled => "Cancelled",
                            TaskStatus::Failed => "Could not start",
                        },
                    ).font_size(12).color(Color::Muted);
                };
                progress(cx, "Progress", f32::from(task.progress) / 100.);
                row(cx).gap(10) + |cx| {
                    if button(cx).text("Start task").primary().disabled(running).clicked()
                    {
                        services.send(TaskCommand::Start);
                    }
                    if button(cx).text("Cancel task").disabled(!running).clicked() {
                        services.send(TaskCommand::Cancel);
                    }
                };
                plico_core::text!(cx, "Completed tasks: {}", task.completed).font_size(12).color(Color::Muted);
            };
            column(cx).padding(20).gap(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |cx| {
                text(cx, "View state").font_size(16).weight(650);
                row(cx).gap(12).align(Align::Center) + |cx| {
                    if button(cx).text("Increment view counter").clicked() {
                        self.counter += 1;
                    }
                    plico_core::text!(cx, "{}", self.counter).font_size(18).weight(650);
                };
                reload_notes(cx);
            };
            text(cx, "The task is simulated; no files or network requests are made.").font_size(11).color(Color::Muted);
        };
    }
}
#[component]
#[rustfmt::skip]
fn reload_notes(cx: &mut UiContext) {
    let details = cx.persisted_state("expanded", || false);
    details.update(|open| {
        checkbox(cx, "Show reload notes", open);
    });
    if details.get() {
        text(cx,"Compatible edits preserve this counter. An incompatible view reset leaves the task running.")
            .font_size(12).color(Color::Muted);
    }
}
#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]
plico_dev::export_service_app!(Application);
