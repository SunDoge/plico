//! UI + value protocol only. The worker is compiled into main.rs, never this module.
pub mod icons;
use plico::prelude::*;
use plico_runtime::regions::{
    BindState, OnClick, Reactive, RegionApp, RegionServiceProgram, RegionUi, Regions, State,
};
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

/// Regions owns only UI state and a transient command outbox. The host retains
/// the live worker and supplies its non-persistent snapshot on each frame.
pub struct Program;
impl plico_runtime::regions::RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let task = app.state(TaskSnapshot::default);
            let commands = app.state(Vec::<TaskCommand>::new);
            app.service_state(task, commands);
            #[cfg(feature = "schema-reset-fixture")]
            let counter = app.persisted_state(1001, || 0_u64);
            #[cfg(not(feature = "schema-reset-fixture"))]
            let counter = app.persisted_state(1, || 0_u64);
            app.root(
                (task, commands, counter),
                |cx, (task, commands, counter)| {
                    workspace(cx, task.handle(), commands.handle(), counter.handle());
                },
            );
        })
    }
}
impl RegionServiceProgram for Program {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
}
pub type Application = RegionApp<Program>;

#[component]
#[rustfmt::skip]
fn workspace(
    cx: &mut UiContext,
    task: State<TaskSnapshot>,
    commands: State<Vec<TaskCommand>>,
    counter: State<u64>,
) {
    column(cx).padding(28).gap(20).width_percent(100.).height_percent(100.) + |cx| {
        row(cx).align(Align::Center).gap(12) + |cx| {
            icon(cx, &crate::icons::LAYERS).size(26., 26.).color(Color::Accent);
            column(cx).gap(5) + |cx| {
                text(cx, "Task workspace").font_size(24).weight(750);
                text(cx, "Start a task, then edit this view while it runs.").font_size(12)
                    .color(Color::Muted);
            };
        };
        cx.region((task, commands), |cx, (task, commands)| {
            let task = task.read();
            let running = task.status == TaskStatus::Running;
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
                    button(cx).text("Start task").primary().disabled(running)
                        .on_click(commands.handle(), |commands| {
                        commands.update(|commands| commands.push(TaskCommand::Start));
                    });
                    button(cx).text("Cancel task").disabled(!running).on_click(
                        commands.handle(),
                        |commands| {
                            commands.update(|commands| commands.push(TaskCommand::Cancel));
                        },
                    );
                };
                text!(cx, "Completed tasks: {}", task.completed).font_size(12).color(Color::Muted);
            };
        });
        cx.region(counter, |cx, counter| {
            let counter = counter.handle();
            column(cx).padding(20).gap(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |cx| {
                text(cx, "View state").font_size(16).weight(650);
                row(cx).gap(12).align(Align::Center) + |cx| {
                    button(cx).text("Increment view counter").on_click(counter, |counter| {
                        counter.update(|value| *value += 1)
                    });
                    text_empty(cx).font_size(18).weight(650).rx(
                        counter,
                        |node, counter| {
                            node.text(format!("View counter: {}", *counter.read()));
                        },
                    );
                };
                reload_notes(cx);
            };
        });
        text(cx, "The task is simulated; no files or network requests are made.").font_size(11).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn reload_notes(cx: &mut UiContext) {
    let details = cx.local_persisted_state(2, || false);
    cx.region(details, |cx, details| {
        let mut expanded = *details.read();
        checkbox(cx, "Show reload notes", &mut expanded).bind(details.handle());
        if expanded {
            text(cx, "Compatible edits preserve this counter. An incompatible view reset leaves the task running.")
                .font_size(12).color(Color::Muted);
        }
    });
}

#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]
plico_dev::export_service_app!(Application);
