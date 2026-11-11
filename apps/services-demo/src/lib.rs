//! UI + value protocol only. The worker is compiled into main.rs, never this module.
pub mod icons;
use plico::prelude::*;
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

/// Ordinary Rust UI state. The host owns the live worker and projects a
/// read-only snapshot into each declaration.
#[derive(Default, Serialize, Deserialize)]
pub struct Application {
    counter: u64,
    details: bool,
}
impl plico_dev::services::ServiceApp for Application {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    fn show(
        &mut self,
        cx: &mut UiContext<'_>,
        services: &mut plico_dev::services::Services<TaskSnapshot, TaskCommand>,
    ) {
        workspace(cx, services, &mut self.counter, &mut self.details);
    }
}

#[component]
#[rustfmt::skip]
fn workspace(
    cx: &mut UiContext,
    services: &mut plico_dev::services::Services<TaskSnapshot, TaskCommand>,
    counter: &mut u64,
    details: &mut bool,
) {
    column(cx).padding(24).gap(16).width_percent(100.).height_percent(100.).scroll_y(true) + |cx| {
        row(cx).align(Align::Center).gap(12) + |cx| {
            icon(cx, &crate::icons::LAYERS).size(26., 26.).color(Color::Accent);
            column(cx).width(0.).grow(1.).gap(5) + |cx| {
                text(cx, "Task workspace").font_size(24).weight(750);
                text(cx, "Start or cancel a background task while using the interface.").color(Color::Muted);
            };
        };
        {
            let task = services.snapshot().clone();
            let running = task.status == TaskStatus::Running;
            panel(cx, None) + |cx| {
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
                    ).color(Color::Muted);
                };
                progress(cx, "Progress", f32::from(task.progress) / 100.);
                toolbar(cx).padding_x(0).wrap(true) + |cx| {
                    if button(cx).text("Start task").primary().disabled(running).clicked() {
                        services.send(TaskCommand::Start);
                    }
                    if button(cx).text("Cancel task").disabled(!running).clicked() {
                        services.send(TaskCommand::Cancel);
                    }
                };
                text!(cx, "Completed tasks: {}", task.completed).color(Color::Muted);
            };
        }
        {
            panel(cx, "View state").description("Local interface state stays independent of the background task.")
            + |cx|
            {
                row(cx).gap(12).align(Align::Center) + |cx| {
                    if button(cx).text("Increment view counter").clicked() { *counter += 1; }
                    text!(cx, "View counter: {}", *counter).font_size(18).weight(650);
                };
                state_details(cx, details);
            };
        }
        text(cx, "The task is simulated; no files or network requests are made.").font_size(14).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn state_details(cx: &mut UiContext, details: &mut bool) {
        checkbox(cx, "Show state details", details);
        if *details {
            text(cx, "This counter belongs to the interface. Starting or cancelling a task does not reset it.")
                .color(Color::Muted);
        }
}
