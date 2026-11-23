//! CPU update cost of a mounted transfer list. Excludes layout, paint and native widgets.
use nsh_core::element::reactive::{Runtime, batch, effect, signal};
use plico::{Engine, UiContext};
use std::{cell::RefCell, hint::black_box, rc::Rc, time::Instant};

fn caption(row: usize, value: u32) -> String {
    format!("Transfer {row}: {value}%")
}

fn declare(engine: &mut Engine, values: &[u32], memo: bool) {
    engine
        .frame_declarations(|cx| {
            for (row, &value) in values.iter().enumerate() {
                cx.keyed(row as u64, |cx| {
                    let label = |cx: &mut UiContext| {
                        cx.text(caption(row, value));
                    };
                    if memo {
                        cx.memo(value, label);
                    } else {
                        label(cx);
                    }
                });
            }
        })
        .unwrap();
}

fn report(rows: usize, every: bool, mode: &str, mount: Instant, mut update: impl FnMut(usize)) {
    let mount_us = mount.elapsed().as_secs_f64() * 1e6;
    let mut samples = Vec::new();
    for tick in 0..1100 {
        let start = Instant::now();
        update(tick);
        if tick >= 100 {
            samples.push(start.elapsed().as_nanos());
        }
    }
    samples.sort_unstable();
    println!(
        "{rows},{every},{mode},{mount_us:.1},{:.2},{:.2}",
        samples[499] as f64 / 1000.,
        samples[949] as f64 / 1000.
    );
}

fn main() {
    println!("rows,all_changed,mode,mount_us,p50_us,p95_us");
    for rows in [24, 200] {
        for every in [false, true] {
            let changed = |tick: usize| {
                if every {
                    0..rows
                } else {
                    let row = tick % rows;
                    row..row + 1
                }
            };
            let mount = Instant::now();
            let rt = Runtime::default();
            let _entered = rt.enter();
            let outputs = Rc::new(RefCell::new(vec![String::new(); rows]));
            let signals: Vec<_> = (0..rows)
                .map(|row| {
                    let value = signal(0u32);
                    let outputs = outputs.clone();
                    effect(move || outputs.borrow_mut()[row] = caption(row, value.get()));
                    value
                })
                .collect();
            report(rows, every, "nsh-effect", mount, |tick| {
                batch(|| {
                    for row in changed(tick) {
                        signals[row].update(|v| *v = (*v + 1) % 101);
                    }
                })
            });
            for memo in [false, true] {
                let mount = Instant::now();
                let mut engine = Engine::default();
                let mut values = vec![0; rows];
                declare(&mut engine, &values, memo);
                report(
                    rows,
                    every,
                    if memo { "plico-memo" } else { "plico-diff" },
                    mount,
                    |tick| {
                        for row in changed(tick) {
                            values[row] = (values[row] + 1) % 101;
                        }
                        declare(&mut engine, black_box(&values), memo);
                    },
                );
                let actual: Vec<_> = engine
                    .snapshot()
                    .into_iter()
                    .map(|node| node.text)
                    .collect();
                assert_eq!(actual, *outputs.borrow());
                for (row, value) in values.into_iter().enumerate() {
                    assert_eq!(actual[row], caption(row, value));
                }
            }
        }
    }
}
