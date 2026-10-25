//! Controlled comparisons: same body/Engine/storage, plus separate replay phases.
use super::*;

fn draw_tree(cx: &mut UiContext<'_>, values: &[u64], rounds: usize) {
    let _ = column(cx).key("forest").gap(10.)
        + |cx| {
            for (index, &value) in values.iter().enumerate() {
                cx.keyed(index, |cx| {
                    let _ = body(cx, value, rounds);
                });
            }
        };
}
async fn whole_view(ui: AsyncUi, values: Rc<RefCell<Vec<u64>>>, rounds: usize) {
    loop {
        ui.frame(|cx| draw_tree(cx, &values.borrow(), rounds)).await;
    }
}
const DRIVERS: [&str; 4] = [
    "driver/direct",
    "driver/sync-bridge",
    "driver/future-static",
    "driver/future-dynamic",
];

fn drivers() {
    for count in [100, 1000] {
        for rounds in [0, 8192] {
            for (scenario, dirty) in [
                ("forced-stable", 0),
                ("sparse-one", 1),
                ("dense-all", count),
            ] {
                for offset in 0..4 {
                    let model = (offset + dirty + rounds) % 4;
                    let values = Rc::new(RefCell::new(vec![0; count]));
                    // Identical captured input storage and one Engine per active path.
                    // All construction and future boxing happen outside measurement.
                    let mut direct = Engine::default();
                    let ui = AsyncUi {
                        session: Rc::default(),
                        wake: Arc::default(),
                    };
                    let mut concrete = Box::pin(whole_view(ui.clone(), values.clone(), rounds));
                    let mut erased: Pin<Box<dyn Future<Output = ()>>> =
                        Box::pin(whole_view(ui.clone(), values.clone(), rounds));
                    let waker = Waker::from(ui.wake.clone());
                    let mut cx = Context::from_waker(&waker);
                    let mut step = || -> FrameStats {
                        if model == 0 {
                            direct
                                .frame(|cx| draw_tree(cx, &values.borrow(), rounds))
                                .unwrap()
                        } else {
                            ui.session.borrow_mut().pass += 1;
                            match model {
                                1 => {
                                    assert!(
                                        ui.draw(|cx| draw_tree(cx, &values.borrow(), rounds))
                                            .is_ready()
                                    );
                                }
                                2 => {
                                    assert!(concrete.as_mut().poll(&mut cx).is_pending());
                                }
                                3 => {
                                    assert!(erased.as_mut().poll(&mut cx).is_pending());
                                }
                                _ => unreachable!(),
                            }
                            ui.session.borrow().engine.last_stats()
                        }
                    };
                    for _ in 0..5 {
                        step();
                    }
                    let samples = (0..21)
                        .map(|event| {
                            measure(|| {
                                {
                                    let mut values = values.borrow_mut();
                                    for offset in 0..dirty {
                                        values[(event + offset) % count] = event as u64 + 1;
                                    }
                                }
                                let stats = step();
                                assert_eq!(stats.visited, count * 5 + 1);
                                (stats.visited, count)
                            })
                        })
                        .collect();
                    row(DRIVERS[model], count, rounds, scenario, samples);
                }
            }
        }
    }
}

fn phases() {
    for count in [100, 1000] {
        for rounds in [0, 8192] {
            for (scenario, dirty) in [
                ("forced-stable", 0),
                ("sparse-one", 1),
                ("dense-all", count),
            ] {
                let mut app = Coroutine::new(count, rounds);
                for _ in 0..5 {
                    app.frame();
                }
                let mut local = Vec::new();
                let mut replay = Vec::new();
                let mut routes = Vec::new();
                for event in 0..21 {
                    // All phase prerequisites are deliberately outside its timer.
                    app.update(event, dirty);
                    let before = app.declarations();
                    local.push(measure(|| {
                        let polls = app.scopes.pump();
                        assert_eq!(polls, dirty);
                        (dirty * 5, dirty)
                    }));
                    assert_eq!(app.declarations() - before, dirty);
                    replay.push(measure(|| {
                        let stats = app.shared.replay(&app.scopes);
                        (stats.visited, 0)
                    }));
                    routes.push(measure(|| {
                        app.shared.rebuild_routes(&app.scopes);
                        (app.shared.routes.len(), 0)
                    }));
                }
                row("phase/local-pump", count, rounds, scenario, local);
                row("phase/shared-replay", count, rounds, scenario, replay);
                row("phase/route-rebuild", count, rounds, scenario, routes);
            }
        }
    }
}

async fn ticks(value: Rc<std::cell::Cell<u64>>) {
    loop {
        let mut yielded = false;
        poll_fn(|_| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                Poll::Pending
            }
        })
        .await;
        value.set(value.get() + 1);
    }
}
fn poll_only() {
    // No Engine, routing, rendering, IO or allocation per poll. Same Cell access
    // and black_box operation in all paths. One warm poll primes the Future.
    const OPERATIONS: usize = 100_000;
    for model in 0..3 {
        let value = Rc::new(std::cell::Cell::new(0));
        let mut concrete = Box::pin(ticks(value.clone()));
        let mut erased: Pin<Box<dyn Future<Output = ()>>> = Box::pin(ticks(value.clone()));
        let mut cx = Context::from_waker(Waker::noop());
        if model == 1 {
            assert!(concrete.as_mut().poll(&mut cx).is_pending());
        }
        if model == 2 {
            assert!(erased.as_mut().poll(&mut cx).is_pending());
        }
        let samples = (0..21)
            .map(|_| {
                let before = value.get();
                let measured = measure(|| {
                    for _ in 0..OPERATIONS {
                        match model {
                            0 => {
                                let value = black_box(&value);
                                value.set(value.get() + 1);
                            }
                            1 => {
                                assert!(black_box(concrete.as_mut()).poll(&mut cx).is_pending());
                            }
                            2 => {
                                assert!(black_box(erased.as_mut()).poll(&mut cx).is_pending());
                            }
                            _ => unreachable!(),
                        }
                    }
                    (0, OPERATIONS)
                });
                assert_eq!(value.get() - before, OPERATIONS as u64);
                assert_eq!(measured.allocations, 0);
                measured
            })
            .collect();
        row(
            [
                "poll/sync-cell",
                "poll/future-static",
                "poll/future-dynamic",
            ][model],
            OPERATIONS,
            0,
            "poll-batch",
            samples,
        );
    }
}
pub(super) fn run() {
    eprintln!("isolation: 21 samples, 5 UI warmups; poll-batch times are per 100000 operations");
    println!(
        "model,components,work_rounds,scenario,median_ns,p95_ns,alloc_calls,allocated_bytes,shared_nodes_visited,business_components_executed"
    );
    drivers();
    phases();
    poll_only();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_engine_future_driver_matches_synchronous_body() {
        let values = Rc::new(RefCell::new(vec![0; 10]));
        let mut engine = Engine::default();
        let ui = AsyncUi {
            session: Rc::default(),
            wake: Arc::default(),
        };
        let mut task = Box::pin(whole_view(ui.clone(), values.clone(), 64));
        let mut cx = Context::from_waker(Waker::noop());
        for pass in 1..4 {
            values.borrow_mut()[3] = pass;
            engine
                .frame(|cx| draw_tree(cx, &values.borrow(), 64))
                .unwrap();
            ui.session.borrow_mut().pass = pass;
            assert!(task.as_mut().poll(&mut cx).is_pending());
            let local = ui.session.borrow();
            assert_eq!(
                serde_json::to_value(engine.snapshot()).unwrap(),
                serde_json::to_value(local.engine.snapshot()).unwrap()
            );
            assert_eq!(engine.last_stats(), local.engine.last_stats());
        }
        let declarations = ui.session.borrow().declarations;
        assert!(task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(ui.session.borrow().declarations, declarations);
    }
    #[test]
    fn decomposed_commit_preserves_results_and_routes() {
        let mut app = Coroutine::new(10, 0);
        app.scopes.pump();
        app.shared.replay(&app.scopes);
        app.shared.rebuild_routes(&app.scopes);
        assert_eq!(app.shared.routes.len(), 50);
        let button = app
            .shared
            .button(app.scopes.nodes.iter().next().unwrap().0, "Increment");
        assert!(app.shared.click(&mut app.scopes, button));
    }
}
