//! Replay vault updates through the same NSH event, layout and mount-batch pipeline.
use nsh_core::{
    ApproxTextEngine, Core, EventKind, EventPayload, MountItem, Size,
    element::{
        Accessible, AnyElement, Component, Interactive, IntoElement, NoEvent, ParentElement, Root,
        Scope, Styled, batch, dynamic, signal, text, view,
    },
    geometry::px,
};
use plico::{Engine, UiContext};
use plico_nsh::Adapter;
use std::{cell::RefCell, time::Instant};

const WARMUP: usize = 100;
const SAMPLES: usize = 1000;

fn item(i: usize, active: bool) -> String {
    format!("{} Login {i:02}", if active { "●" } else { "○" })
}
fn detail(i: usize) -> String {
    format!("Details: Login {i:02}")
}
fn countdown(seconds: usize) -> String {
    format!("Code expires in {seconds:02}s")
}

struct Vault<const SIGNALS: bool, const MEMO: bool, const SELECT: bool>;
impl<const SIGNALS: bool, const MEMO: bool, const SELECT: bool> Component
    for Vault<SIGNALS, MEMO, SELECT>
{
    type Props = ();
    type Event = NoEvent;
    fn view(_: (), _: Scope<NoEvent>) -> impl IntoElement {
        let rows = std::env::var("NSH_VAULT_ROWS").map_or(24, |s| s.parse::<usize>().unwrap());
        assert!(rows >= 2);
        let step = signal(0usize);
        let active = SIGNALS.then(|| signal(0usize));
        let seconds = SIGNALS.then(|| signal(30usize));
        let markers: Vec<_> = (0..if SIGNALS { rows } else { 0 })
            .map(|i| signal(i == 0))
            .collect();
        let body: AnyElement = if SIGNALS {
            view()
                .flex_col()
                .children(markers.iter().copied().enumerate().map(|(i, marker)| {
                    text(move || nsh_core::element::SharedString::from(item(i, marker.get())))
                }))
                .child(text(move || {
                    nsh_core::element::SharedString::from(detail(active.unwrap().get()))
                }))
                .child(text(move || {
                    nsh_core::element::SharedString::from(countdown(seconds.unwrap().get()))
                }))
                .into_element()
        } else {
            let engine = RefCell::new(Engine::default());
            let adapter = Adapter::default();
            dynamic(move || {
                let n = step.get();
                let selected = if SELECT { n % rows } else { 0 };
                let remaining = if SELECT { 30 } else { 30 - n % 30 };
                let mut engine = engine.borrow_mut();
                engine
                    .frame_declarations(|cx| {
                        let list = |cx: &mut UiContext| {
                            for i in 0..rows {
                                cx.text(item(i, i == selected)).key(i as u64);
                            }
                        };
                        if MEMO {
                            cx.memo(selected, list);
                        } else {
                            list(cx);
                        }
                        cx.text(detail(selected));
                        cx.text(countdown(remaining));
                    })
                    .unwrap();
                adapter.project(&engine).unwrap()
            })
            .into_element()
        };
        view()
            .flex_col()
            .child(
                view()
                    .accessible(true)
                    .label("Replay event")
                    .child(text("Replay event"))
                    .on_press(move || {
                        let n = step.get_untracked() + 1;
                        batch(|| {
                            step.set(n);
                            if SIGNALS && SELECT {
                                markers[(n - 1) % rows].set(false);
                                markers[n % rows].set(true);
                                active.unwrap().set(n % rows);
                            } else if SIGNALS {
                                seconds.unwrap().set(30 - n % 30);
                            }
                        });
                    }),
            )
            .child(body)
    }
}

fn run<const SIGNALS: bool, const MEMO: bool, const SELECT: bool>() {
    let rows = std::env::var("NSH_VAULT_ROWS").map_or(24, |s| s.parse::<usize>().unwrap());
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let memory = std::env::var_os("NSH_VAULT_MEMORY").is_some();
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if memory {
        memory_sample("baseline");
    }
    let mut core = Core::new(ApproxTextEngine, Size::new(px(720.), px(900.)));
    core.mount_root(&Root::new::<Vault<SIGNALS, MEMO, SELECT>>(|| ()));
    let first = core.commit().unwrap();
    let trigger = first
        .items
        .iter()
        .find_map(|it| match it {
            MountItem::ViewProps { id, props }
                if props.accessibility.label.as_deref() == Some("Replay event") =>
            {
                Some(*id)
            }
            _ => None,
        })
        .unwrap();
    drop(first);
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if memory {
        memory_sample("mounted");
        for n in 1..=10000 {
            assert!(core.component_event(trigger, EventKind::Press, &EventPayload::None));
            drop(core.commit().unwrap());
            if n == 100 {
                memory_sample("after100");
            }
        }
        memory_sample("after10000");
        drop(core);
        memory_sample("closed");
        return;
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("NSH_VAULT_CPU").is_some() {
        // Linux schedstat's first field is time actually scheduled on this thread.
        let scheduled_ns = || -> u64 {
            std::fs::read_to_string("/proc/thread-self/schedstat")
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        let mut update = || {
            assert!(core.component_event(trigger, EventKind::Press, &EventPayload::None));
            std::hint::black_box(core.commit().unwrap());
        };
        for _ in 0..WARMUP {
            update();
        }
        const EVENTS: u64 = 50_000;
        let before = scheduled_ns();
        for _ in 0..EVENTS {
            update();
        }
        let cpu_us = (scheduled_ns() - before) as f64 / EVENTS as f64 / 1000.;
        println!(
            "{},{},{cpu_us:.3}",
            if SELECT { "select" } else { "tick" },
            if SIGNALS {
                "signal"
            } else if MEMO {
                "list-memo"
            } else {
                "diff"
            }
        );
        return;
    }
    let mut times = Vec::new();
    let mut ops = 0;
    for n in 1..=WARMUP + SAMPLES {
        let start = Instant::now();
        assert!(core.component_event(trigger, EventKind::Press, &EventPayload::None));
        let batch = core.commit().unwrap();
        let elapsed = start.elapsed();
        let mut actual: Vec<_> = batch
            .items
            .iter()
            .filter_map(|it| match it {
                MountItem::Text { text, .. } => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        let mut expected = if SELECT {
            vec![
                item((n - 1) % rows, false),
                item(n % rows, true),
                detail(n % rows),
            ]
        } else {
            vec![countdown(30 - n % 30)]
        };
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected);
        if n > WARMUP {
            times.push(elapsed.as_nanos());
            ops += batch.items.len();
        }
    }
    times.sort_unstable();
    println!(
        "{},{},{:.2},{:.2},{:.1}",
        if SELECT { "select" } else { "tick" },
        if SIGNALS {
            "signal"
        } else if MEMO {
            "list-memo"
        } else {
            "diff"
        },
        times[499] as f64 / 1000.,
        times[949] as f64 / 1000.,
        ops as f64 / SAMPLES as f64
    );
}

#[cfg(all(feature = "native", target_os = "linux"))]
fn window(mode: &str) {
    use plico_nsh::native::platform;
    let root = match mode {
        "tick-signal" => Root::new::<Vault<true, false, false>>(|| ()),
        "tick-diff" => Root::new::<Vault<false, false, false>>(|| ()),
        "tick-memo" => Root::new::<Vault<false, true, false>>(|| ()),
        "select-signal" => Root::new::<Vault<true, false, true>>(|| ()),
        "select-diff" => Root::new::<Vault<false, false, true>>(|| ()),
        "select-memo" => Root::new::<Vault<false, true, true>>(|| ()),
        _ => panic!("unknown NSH_VAULT_WINDOW mode: {mode}"),
    };
    let _ = platform::run_app(
        platform::AppSource::Rust(root),
        platform::AppOptions {
            app_id: "dev.plico.vault-benchmark".into(),
            title: "Vault replay".into(),
            size: (720, 900),
            ..Default::default()
        },
    );
}

// glibc mallinfo2 fields in ABI order. Include live mmap allocations.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn memory_sample(stage: &str) {
    #[repr(C)]
    struct Info {
        arena: usize,
        ordblks: usize,
        smblks: usize,
        hblks: usize,
        hblkhd: usize,
        usmblks: usize,
        fsmblks: usize,
        uordblks: usize,
        fordblks: usize,
        keepcost: usize,
    }
    unsafe extern "C" {
        fn mallinfo2() -> Info;
    }
    // SAFETY: mallinfo2 takes no arguments and returns this glibc ABI struct by value.
    let info = unsafe { mallinfo2() };
    let smaps = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let kb = |field: &str| {
        smaps
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    println!(
        "{stage},{},{},{}",
        info.uordblks + info.hblkhd,
        kb("Rss:"),
        kb("Private_Clean:") + kb("Private_Dirty:")
    );
}

fn main() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if let Ok(mode) = std::env::var("NSH_VAULT_MEMORY") {
        match mode.as_str() {
            "signal" => run::<true, false, true>(),
            "diff" => run::<false, false, true>(),
            "memo" => run::<false, true, true>(),
            _ => panic!("expected signal, diff or memo"),
        }
        return;
    }

    #[cfg(all(feature = "native", target_os = "linux"))]
    if let Ok(mode) = std::env::var("NSH_VAULT_WINDOW") {
        return window(&mode);
    }
    if cfg!(target_os = "linux") && std::env::var_os("NSH_VAULT_CPU").is_some() {
        println!("action,mode,cpu_us_per_event");
    } else {
        println!("action,mode,p50_us,p95_us,mount_ops_per_event");
    }
    run::<true, false, false>();
    run::<false, false, false>();
    run::<false, true, false>();
    run::<true, false, true>();
    run::<false, false, true>();
    run::<false, true, true>();
}
