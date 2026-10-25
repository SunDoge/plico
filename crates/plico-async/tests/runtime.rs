use plico_async::{AsyncUi, Error, Input, Result, Runtime, ScopeId};
use plico_core::{Changes, Engine, Entity, prelude::*};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

fn find(engine: &Engine, parent: Entity, label: &str) -> Option<Entity> {
    let node = engine.node(parent)?;
    if matches!(
        node.kind,
        plico_core::Kind::Button | plico_core::Kind::ButtonBase
    ) && engine.accessible_name(parent).as_deref() == Some(label)
    {
        return Some(parent);
    }
    node.children.iter().find_map(|&id| find(engine, id, label))
}
fn find_button(runtime: &Runtime, parent: Entity, label: &str) -> Entity {
    find(&runtime.engine(), parent, label).unwrap()
}
fn text_present(runtime: &Runtime, text: &str) -> bool {
    fn visit(engine: &Engine, parent: Entity, text: &str) -> bool {
        let node = engine.node(parent).unwrap();
        node.text == text || node.children.iter().any(|&id| visit(engine, id, text))
    }
    visit(&runtime.engine(), runtime.engine().root(), text)
}
async fn counter(ui: AsyncUi, title: Input<String>, calls: Rc<Cell<usize>>) -> Result<()> {
    title.subscribe(&ui);
    let mut count: i32 = ui.state("count", || 0);
    loop {
        ui.frame(|cx| {
            calls.set(calls.get() + 1);
            title.with(|title| {
                text(cx, title.as_str());
            });
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        })
        .await?;
        ui.checkpoint("count", &count)?;
    }
}

fn mount_counter(
    runtime: &mut Runtime,
    key: &'static str,
    title: Input<String>,
    calls: Rc<Cell<usize>>,
) -> ScopeId {
    let mut id = None;
    runtime
        .render(|cx, mounts| {
            id = Some(mounts.mount(column(cx).key(key), |ui| counter(ui, title, calls)));
        })
        .unwrap();
    id.unwrap()
}

#[test]
fn sync_parent_and_two_async_scopes_share_identity_and_route_batched_clicks() {
    let mut runtime = Runtime::new();
    let a_calls = Rc::new(Cell::new(0));
    let b_calls = Rc::new(Cell::new(0));
    let a_title = Input::new("A".into());
    let b_title = Input::new("B".into());
    let (mut a, mut b) = (None, None);
    runtime
        .render(|cx, mounts| {
            text(cx, "Parent").key("parent");
            a = Some(mounts.mount(column(cx).key("a"), {
                let title = a_title.clone();
                let calls = a_calls.clone();
                move |ui| counter(ui, title, calls)
            }));
            b = Some(mounts.mount(column(cx).key("b"), {
                let title = b_title.clone();
                let calls = b_calls.clone();
                move |ui| counter(ui, title, calls)
            }));
        })
        .unwrap();
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(runtime.pump().unwrap(), 2);
    let a_button = find_button(&runtime, a.entity(), "Increment");
    let b_button = find_button(&runtime, b.entity(), "Increment");
    runtime.take_delta();
    runtime.queue_click(a_button);
    runtime.queue_click(b_button);
    // Re-rendering the synchronous parent must not erase queued async input.
    runtime
        .render(|cx, mounts| {
            text(cx, "Parent").key("parent");
            assert_eq!(
                mounts.mount(column(cx).key("a"), |_| async { panic!("recreated A") }),
                a
            );
            assert_eq!(
                mounts.mount(column(cx).key("b"), |_| async { panic!("recreated B") }),
                b
            );
        })
        .unwrap();
    runtime.pump().unwrap();
    assert_eq!(a_calls.get(), 3);
    assert_eq!(b_calls.get(), 3);
    assert_eq!(find_button(&runtime, a.entity(), "Increment"), a_button);
    let delta = runtime.take_delta();
    assert!(!delta.changes.is_empty());
    assert!(delta.frame.contains(Changes::PAINT));
    assert!(!delta.frame.contains(Changes::VIEW));
    assert!(runtime.take_delta().changes.is_empty());
    let a_before = a_calls.get();
    let b_before = b_calls.get();
    a_title.set("Changed A".into());
    a_title.set("Latest A".into());
    assert_eq!(runtime.pump().unwrap(), 1);
    assert!(text_present(&runtime, "Latest A"));
    assert_eq!(a_calls.get(), a_before + 1);
    assert_eq!(b_calls.get(), b_before);
    assert_eq!(runtime.pump().unwrap(), 0);
    runtime
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("b"), |_| async { panic!("recreated B") });
        })
        .unwrap();
    assert!(!runtime.queue_click(a_button));
    assert!(runtime.take_delta().removed.contains(&a_button));
    assert!(runtime.queue_click(b_button));
    runtime.pump().unwrap();
}

#[derive(Clone, Default)]
struct Completion(Arc<Mutex<(Option<i32>, Option<Waker>)>>);
impl Completion {
    fn complete(&self, value: i32) {
        let mut state = self.0.lock().unwrap();
        state.0 = Some(value);
        let wake = state.1.take();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
impl Future for Completion {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let mut state = self.0.lock().unwrap();
        if let Some(value) = state.0.take() {
            Poll::Ready(value)
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[test]
fn background_wake_does_not_redeclare_sibling_and_scope_can_be_cleared() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let observed = wakes.clone();
    let mut runtime = Runtime::with_wake(
        move || {
            observed.fetch_add(1, Ordering::Relaxed);
        },
        false,
    );
    let completion = Completion::default();
    let sibling_calls = Rc::new(Cell::new(0));
    let (mut waiting, mut sibling) = (None, None);
    runtime
        .render(|cx, mounts| {
            waiting = Some(mounts.mount(column(cx).key("waiting"), {
                let task = completion.clone();
                move |ui| async move {
                    let value = ui
                        .while_pending(task, |cx| {
                            text(cx, "Loading");
                        })
                        .await?;
                    loop {
                        ui.frame(|cx| {
                            text!(cx, "Result: {value}");
                        })
                        .await?;
                    }
                }
            }));
            sibling = Some(mounts.mount(column(cx).key("sibling"), {
                let calls = sibling_calls.clone();
                move |ui| counter(ui, Input::new("Sibling".into()), calls)
            }));
        })
        .unwrap();
    runtime.pump().unwrap();
    assert!(text_present(&runtime, "Loading"));
    let before = sibling_calls.get();
    let task = completion.clone();
    std::thread::spawn(move || task.complete(42))
        .join()
        .unwrap();
    assert!(runtime.has_ready());
    runtime.pump().unwrap();
    assert!(text_present(&runtime, "Result: 42"));
    assert_eq!(sibling_calls.get(), before);
    assert!(wakes.load(Ordering::Relaxed) > 0);
    runtime.cancel(waiting.unwrap()).unwrap();
    assert!(!text_present(&runtime, "Result: 42"));
    let sibling = sibling.unwrap();
    runtime.queue_click(find_button(&runtime, sibling.entity(), "Increment"));
    runtime.pump().unwrap();
    assert_eq!(sibling_calls.get(), before + 2);
}

#[test]
fn cancellation_while_pending_drops_children_before_parent_and_old_wakes_are_inert() {
    struct Guard(&'static str, Rc<RefCell<Vec<&'static str>>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }
    let mut runtime = Runtime::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let task = Completion::default();
    let mut parent = None;
    runtime
        .render(|cx, mounts| {
            parent = Some(mounts.mount(column(cx).key("parent"), {
                let log = log.clone();
                let task = task.clone();
                move |ui| async move {
                    let _parent = Guard("parent", log.clone());
                    loop {
                        ui.frame(|cx| {
                            ui.mount(column(cx).key("child"), {
                                let log = log.clone();
                                let task = task.clone();
                                move |ui| async move {
                                    let _child = Guard("child", log);
                                    ui.while_pending(task, |cx| {
                                        text(cx, "Waiting child");
                                    })
                                    .await?;
                                    Ok(())
                                }
                            })
                        })
                        .await??;
                    }
                }
            }));
        })
        .unwrap();
    runtime.pump().unwrap();
    assert!(text_present(&runtime, "Waiting child"));
    runtime.cancel(parent.unwrap()).unwrap();
    assert_eq!(*log.borrow(), ["child", "parent"]);
    task.complete(1);
    assert_eq!(runtime.pump().unwrap(), 0);
    assert!(!text_present(&runtime, "Waiting child"));
}

#[test]
fn checkpoints_roundtrip_between_runtimes_and_incompatible_values_reset() {
    let mut old = Runtime::new();
    let id = mount_counter(
        &mut old,
        "counter",
        Input::new("Counter".into()),
        Rc::default(),
    );
    old.pump().unwrap();
    old.queue_click(find_button(&old, id.entity(), "Increment"));
    old.pump().unwrap();
    let snapshot = old.dump_state().unwrap();
    drop(old);
    let mut new = Runtime::new();
    mount_counter(
        &mut new,
        "counter",
        Input::new("Counter".into()),
        Rc::default(),
    );
    new.restore_state(&snapshot).unwrap();
    new.pump().unwrap();
    assert!(text_present(&new, "Count: 1"));
    assert!(matches!(
        new.restore_state(&snapshot),
        Err(Error::AlreadyStarted)
    ));
    let mut incompatible = Runtime::new();
    incompatible
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("counter"), |ui| async move {
                let count: String = ui.state("count", || "reset".into());
                loop {
                    ui.frame(|cx| {
                        text(cx, &count);
                    })
                    .await?;
                }
            });
        })
        .unwrap();
    incompatible.restore_state(&snapshot).unwrap();
    incompatible.pump().unwrap();
    assert!(text_present(&incompatible, "reset"));
}

#[test]
fn finite_components_errors_and_poll_budgets_are_explicit() {
    let mut runtime = Runtime::new();
    runtime
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("finite"), |ui| async move {
                ui.frame(|cx| {
                    text(cx, "Temporary");
                })
                .await?;
                Ok(())
            });
        })
        .unwrap();
    runtime.pump().unwrap();
    assert!(!text_present(&runtime, "Temporary"));
    runtime
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("bad"), |ui| async move {
                ui.frame(|cx| {
                    text(cx, "bad").width(f32::NAN);
                })
                .await?;
                Ok(())
            });
        })
        .unwrap();
    assert!(matches!(runtime.pump(), Err(Error::Frame { .. })));
    runtime
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("spin"), |ui| async move {
                loop {
                    ui.frame(|cx| {
                        text(cx, "spin");
                        cx.request_rebuild();
                    })
                    .await?;
                }
            });
        })
        .unwrap();
    assert!(matches!(
        runtime.pump_with_budget(3),
        Err(Error::PollBudget { budget: 3 })
    ));
}

#[test]
fn finite_animation_uses_injected_clock_and_only_wakes_animating_scope() {
    let mut runtime = Runtime::new();
    let calls = Rc::new(Cell::new(0));
    let static_calls = Rc::new(Cell::new(0));
    runtime
        .render(|cx, mounts| {
            mounts.mount(column(cx).key("animated"), {
                let calls = calls.clone();
                move |ui| async move {
                    loop {
                        ui.frame(|cx| {
                            calls.set(calls.get() + 1);
                            text!(cx, "Time: {}", cx.now().as_millis());
                            if cx.now() < std::time::Duration::from_millis(50) {
                                cx.request_animation_frame();
                            }
                        })
                        .await?;
                    }
                }
            });
            mounts.mount(column(cx).key("static"), {
                let calls = static_calls.clone();
                move |ui| counter(ui, Input::new("Static".into()), calls)
            });
        })
        .unwrap();
    runtime.pump().unwrap();
    assert!(runtime.needs_animation_frame());
    assert!(runtime.take_delta().frame.contains(Changes::ANIMATE));
    for ms in [10, 30, 50] {
        runtime.set_time(plico_core::FrameTime {
            now: std::time::Duration::from_millis(ms),
            delta: std::time::Duration::from_millis(10),
        });
        assert_eq!(runtime.pump().unwrap(), 1);
    }
    assert_eq!(calls.get(), 4);
    assert_eq!(static_calls.get(), 1);
    assert!(!runtime.needs_animation_frame());
    runtime.set_time(plico_core::FrameTime {
        now: std::time::Duration::from_millis(100),
        delta: std::time::Duration::from_millis(50),
    });
    assert_eq!(runtime.pump().unwrap(), 0);
}

#[test]
fn inherited_disabled_state_rebuilds_scope_without_recreating_it() {
    let mut runtime = Runtime::new();
    let mut id = None;
    runtime
        .render(|cx, mounts| {
            id = Some(mounts.mount(column(cx).key("counter"), |ui| {
                counter(ui, Input::new("Counter".into()), Rc::default())
            }));
        })
        .unwrap();
    runtime.pump().unwrap();
    let id = id.unwrap();
    let increment = find_button(&runtime, id.entity(), "Increment");
    runtime.queue_click(increment);
    runtime
        .render(|cx, mounts| {
            assert_eq!(
                mounts.mount(column(cx).key("counter").disabled(true), |_| async {
                    panic!("recreated")
                }),
                id
            );
        })
        .unwrap();
    runtime.pump().unwrap();
    assert!(text_present(&runtime, "Count: 0"));
    assert!(runtime.engine().node(increment).unwrap().style.disabled);
    assert!(matches!(
        Runtime::new().cancel(id),
        Err(Error::WrongRuntime)
    ));
}

#[test]
fn runtime_drop_breaks_ownership_and_late_network_wake_does_not_notify_host() {
    struct Guard(Rc<Cell<bool>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let notices = Arc::new(AtomicUsize::new(0));
    let notify = notices.clone();
    let completion = Completion::default();
    {
        let mut runtime = Runtime::with_wake(
            move || {
                notify.fetch_add(1, Ordering::Relaxed);
            },
            false,
        );
        runtime
            .render(|cx, mounts| {
                mounts.mount(column(cx), {
                    let dropped = dropped.clone();
                    let task = completion.clone();
                    move |ui| async move {
                        let _guard = Guard(dropped);
                        ui.while_pending(task, |cx| {
                            text(cx, "Waiting");
                        })
                        .await?;
                        Ok(())
                    }
                });
            })
            .unwrap();
        runtime.pump().unwrap();
    }
    assert!(dropped.get());
    let before = notices.load(Ordering::Relaxed);
    completion.complete(1);
    assert_eq!(notices.load(Ordering::Relaxed), before);
}

#[test]
fn cancel_button_finishes_waiting_and_drops_request_receiver() {
    struct Tracked {
        task: Completion,
        dropped: Rc<Cell<bool>>,
    }
    impl Future for Tracked {
        type Output = i32;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
            Pin::new(&mut self.task).poll(cx)
        }
    }
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    let mut runtime = Runtime::new();
    let task = Completion::default();
    let dropped = Rc::new(Cell::new(false));
    let mut id = None;
    runtime
        .render(|cx, mounts| {
            id = Some(mounts.mount(column(cx).key("task"), {
                let task = task.clone();
                let dropped = dropped.clone();
                move |ui| async move {
                    let result = ui
                        .while_pending_or(Tracked { task, dropped }, |cx| {
                            text(cx, "Loading");
                            button(cx).text("Cancel").clicked().then_some(())
                        })
                        .await?;
                    let message = match result {
                        plico_async::WaitOutcome::Completed(_) => "Completed",
                        plico_async::WaitOutcome::Action(()) => "Cancelled",
                    };
                    loop {
                        ui.frame(|cx| {
                            text(cx, message);
                        })
                        .await?;
                    }
                }
            }));
        })
        .unwrap();
    runtime.pump().unwrap();
    runtime.queue_click(find_button(&runtime, id.unwrap().entity(), "Cancel"));
    runtime.pump().unwrap();
    assert!(dropped.get());
    assert!(text_present(&runtime, "Cancelled"));
    task.complete(42);
    runtime.pump().unwrap();
    assert!(text_present(&runtime, "Cancelled"));
    assert!(!text_present(&runtime, "Completed"));
}

#[test]
fn existing_component_call_site_keys_preserve_mounts_when_a_sibling_disappears() {
    #[component]
    fn parent(
        cx: &mut UiContext,
        mounts: &mut plico_async::Mounts<'_>,
        hide_first: bool,
        ids: &mut Vec<ScopeId>,
    ) {
        if !hide_first {
            ids.push(mounts.mount(column(cx), |ui| {
                counter(ui, Input::new("A".into()), Rc::default())
            }));
        }
        ids.push(mounts.mount(column(cx), |ui| {
            counter(ui, Input::new("B".into()), Rc::default())
        }));
    }
    let mut runtime = Runtime::new();
    let mut ids = Vec::new();
    runtime
        .render(|cx, mounts| parent(cx, mounts, false, &mut ids))
        .unwrap();
    runtime.pump().unwrap();
    let b = ids[1];
    let increment = find_button(&runtime, b.entity(), "Increment");
    runtime.queue_click(increment);
    runtime.pump().unwrap();
    ids.clear();
    runtime
        .render(|cx, mounts| parent(cx, mounts, true, &mut ids))
        .unwrap();
    assert_eq!(ids, [b]);
    assert_eq!(find_button(&runtime, b.entity(), "Increment"), increment);
    assert!(text_present(&runtime, "Count: 1"));
    assert_eq!(runtime.pump().unwrap(), 0);
}
