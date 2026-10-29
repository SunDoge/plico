use plico::prelude::*;
use plico_runtime::regions::{
    Access, BindState, OnChange, OnClick, OnClickWith, OnSubmit, Reactive, Regions, WatchUi,
};

#[derive(Clone, Copy)]
struct Preferences {
    enabled: bool,
    volume: f64,
}
struct Task {
    id: u64,
    label: String,
}
fn add_task((draft, tasks): (Access<'_, TextBuffer>, Access<'_, Vec<Task>>)) {
    let value = draft.read().take();
    draft.notify();
    let label = value.trim();
    if label.is_empty() {
        return;
    }
    tasks.update(|tasks| {
        let id = tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1;
        tasks.push(Task {
            id,
            label: label.to_owned(),
        });
    });
}
type Captures<'a> = (
    Access<'a, i32>,
    Access<'a, bool>,
    Access<'a, Preferences>,
    Access<'a, TextBuffer>,
    Access<'a, Vec<Task>>,
);

#[component]
#[rustfmt::skip]
fn counters(cx: &mut UiContext, (count, expanded, preferences, draft, tasks): Captures<'_>) {
    column(cx).width_percent(100.).height_percent(100.).scroll_y(true).padding(24).gap(16) + |cx| {
        text(cx, "ID-driven regions").font_size(26).weight(600);
        text(cx, "Native controls · explicit dependencies · component-owned state");
        row(cx).gap(8) + |cx| {
            button(cx).text("Increment").primary().on_click(
                (count.handle(),),
                |(count,)| {
                    count.update(|v| *v += 1);
                },
            );
            button(cx).text("Reset").ghost().on_click((count.handle(),), |(count,)| count.set(0));
            button(cx).text("Toggle details").on_click(
                (expanded.handle(),),
                |(expanded,)| {
                    expanded.update(|v| *v = !*v);
                },
            );
        };
        row(cx).gap(16) + |cx| {
            for key in [1, 2] {
                cx.component_keyed(key, (), |cx, ()| {
                    let count = cx.local_state(|| 0_i32);
                    let status = cx.local_state(|| String::from("Idle"));
                    text_empty(cx).reactive((status,), |node, (status,)| {
                        node.text(status.read().clone());
                    });
                    button(cx).text("Load on host").on_click((status,), |(status,)| {
                        status.set(String::from("Loading…"));
                        let result = status.sender();
                        std::thread::spawn(move || {
                            // Simulated host I/O. The UI thread never waits.
                            std::thread::sleep(std::time::Duration::from_millis(600));
                            let _ = result.send(String::from("Loaded"));
                        });
                    });
                    cx.watch((count,), |cx, (count,)| {
                        text!(cx, "Component-local: {}", *count.read());
                        button(cx).text("Local +1").on_click(
                            (count.handle(),),
                            |(count,)| {
                                count.update(|value| *value += 1);
                            },
                        );
                    });
                });
            }
        };
        text_empty(cx).font_size(32).reactive((count.handle(),), |node, (count,)| {
            node.text(format!("Count: {}", *count.read()));
        });
        cx.watch(
            (expanded.handle(), count.handle()),
            |cx, (expanded, count)| {
                if *expanded.read() {
                    column(cx).padding(16).gap(8).radius(12.).background(Color::Panel) + |cx| {
                        text(cx, "Nested region").weight(600);
                        text_empty(cx).reactive((count.handle(),), |node, (count,)| {
                            node.text(format!("Double: {}", *count.read() * 2));
                        });
                    };
                }
            },
        );
        grid(cx).width_percent(100.).gap(16).grid_columns([fr(1.), fr(1.)]).align(Align::Start) + |cx| {
            column(cx).min_width(0.).padding(16).gap(12).radius(12.).background(Color::Panel)
                .border(1., Color::Border) + |cx|
            {
                text(cx, "Live preferences").font_size(20).weight(600);
                cx.watch((preferences.handle(),), |cx, (preferences,)| {
                    column(cx).gap(12) + |cx| {
                        let mut enabled = preferences.read().enabled;
                        checkbox(cx, "Enable accent", &mut enabled).on_change(
                            (preferences.handle(),),
                            |(preferences,), enabled| {
                                preferences.update(|value| value.enabled = enabled);
                            },
                        );
                        let mut enabled = preferences.read().enabled;
                        switch(cx, "Mirror switch", &mut enabled).on_change(
                            (preferences.handle(),),
                            |(preferences,), enabled| {
                                preferences.update(|value| value.enabled = enabled);
                            },
                        );
                        let mut volume = preferences.read().volume;
                        slider(cx, "Intensity", &mut volume, 0. ..=100.).width_percent(100.).on_change(
                            (preferences.handle(),),
                            |(preferences,), volume| {
                                preferences.update(|value| value.volume = volume);
                            },
                        );
                        text!(cx, "Intensity: {:.0}%", preferences.read().volume);
                    };
                });
                text(cx, "Live style preview").reactive(
                    (preferences.handle(),),
                    |node, (preferences,)| {
                        let settings = *preferences.read();
                        node.font_size(14 + (settings.volume / 10.) as u16)
                            .weight(if settings.enabled { 700 } else { 400 })
                            .color(if settings.enabled { Color::Accent } else { Color::Muted });
                    },
                );
            };
            column(cx).min_width(0.).padding(16).gap(12).radius(12.).background(Color::Panel)
                .border(1., Color::Border) + |cx|
            {
                text(cx, "Keyed task list").font_size(20).weight(600);
                cx.watch((draft.handle(), tasks.handle()), |cx, (draft, tasks)| {
                    row(cx).gap(8) + |cx| {
                        text_input(cx, "New task", &draft.read()).placeholder("Type a task, then Enter")
                            .grow(1.).width(0.).bind(draft.handle())
                            .on_submit((draft.handle(), tasks.handle()), add_task);
                        button(cx).text("Add").primary()
                            .on_click((draft.handle(), tasks.handle()), add_task);
                    };
                });
                cx.watch((tasks.handle(),), |cx, (tasks,)| {
                    column(cx).gap(8) + |cx| {
                        row(cx).gap(8).align(Align::Center) + |cx| {
                            text!(cx, "{} tasks", tasks.read().len()).grow(1.);
                            button(cx).text("Reverse").ghost().on_click(
                                (tasks.handle(),),
                                |(tasks,)| {
                                    tasks.update(|tasks| tasks.reverse());
                                },
                            );
                        };
                        column(cx).max_height(230.).scroll_y(true).gap(6) + |cx| {
                            for task in tasks.read().iter() {
                                row(cx).key(task.id).width_percent(100.).gap(8).align(Align::Center) + |cx| {
                                    text(cx, &task.label).grow(1.).min_width(0.);
                                    button(cx).text("Remove").aria_label(format!(
                                        "Remove {}",
                                        task.label
                                    )).ghost().on_click_with(
                                        (tasks.handle(),),
                                        task.id,
                                        |(tasks,), id| {
                                            tasks.update(|tasks| {
                                                tasks.retain(|task| {
                                                    task.id != id
                                                })
                                            });
                                        },
                                    );
                                };
                            }
                        };
                    };
                });
            };
        };
    };
}
fn main() -> Result<(), plico_shell::Error> {
    let app = Regions::new(|app| {
        let count = app.state(|| 0_i32);
        let expanded = app.state(|| true);
        let preferences = app.state(|| Preferences {
            enabled: true,
            volume: 40.,
        });
        let draft = app.state(TextBuffer::new);
        let tasks = app.state(|| {
            vec![
                Task {
                    id: 1,
                    label: "Try the form controls".into(),
                },
                Task {
                    id: 2,
                    label: "Add a task with Enter".into(),
                },
                Task {
                    id: 3,
                    label: "Reverse the keyed rows".into(),
                },
            ]
        });
        app.root((count, expanded, preferences, draft, tasks), counters);
    });
    plico_shell::run_window_with_options(
        Box::new(plico_runtime::Source::new(app)),
        plico_runtime::WindowOptions::new()
            .title("Plico · native regions")
            .initial_inner_size(1040, 800)
            .min_inner_size(760, 600)
            .renderer(plico_renderer::RenderBackend::Skia),
    )
}
