//! Run with cargo run --release -p plico-runtime --example scroll_cost.
use plico::prelude::*;
use plico_renderer::{
    FrameSource, GesturePhase, Point, Scene, TextContext, UiEvent, WheelDeltaMode, WheelEvent,
};
use plico_runtime::{HostView, Source};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
enum List {
    Plain,
    Fixed,
    Variable,
}

struct Rows {
    count: usize,
    list: List,
    peak_row_builds: usize,
}

impl HostView for Rows {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        if self.list != List::Plain {
            let mut built = 0;
            let row = |cx: &mut UiContext<'_>, i: usize| {
                let height = if self.list == List::Variable && i.is_multiple_of(2) {
                    32.
                } else {
                    24.
                };
                text!(cx, "Row {i}").height(height).shrink(0.);
                built += 1;
            };
            match self.list {
                List::Fixed => virtual_list(cx, self.count, 24.)
                    .width(320.)
                    .height(240.)
                    .rows(|i| i, row),
                List::Variable => variable_virtual_list(cx, self.count, 28.)
                    .width(320.)
                    .height(240.)
                    .rows(|i| i, row),
                List::Plain => unreachable!(),
            };
            self.peak_row_builds = self.peak_row_builds.max(built);
            return;
        }
        column(cx)
            .width(320.)
            .height(240.)
            .scroll_y(true)
            .children(|cx| {
                for i in 0..self.count {
                    cx.keyed(i as u64, |cx| {
                        text!(cx, "Row {i}").height(24.).shrink(0.);
                    });
                }
            });
    }
}

fn wheel(delta_y: f64) -> UiEvent {
    UiEvent::Wheel(WheelEvent {
        position: Point { x: 100., y: 100. },
        delta_x: 0.,
        delta_y,
        delta_mode: WheelDeltaMode::Pixel,
        phase: GesturePhase::Changed,
        modifiers: Default::default(),
    })
}

fn measure(rows: usize, present: bool, list: List) {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Rows {
            count: rows,
            list,
            peak_row_builds: 0,
        },
        clock.clone(),
    );
    let mut scene = Scene::new();
    let mut text = TextContext::new();
    source
        .render_retained_frame(&mut scene, &mut text, (320, 240), 1.)
        .unwrap();
    let steps = if present { 64 } else { 4096 };
    let mut samples = Vec::new();
    let mut peak_placed = 0;
    for batch in 0..10 {
        let start = Instant::now();
        for step in 0..steps {
            clock.advance(Duration::from_millis(8)).unwrap();
            // Sweep 64 scroll steps per batch in the virtual case to exercise mounting
            // and eviction. Keep the original plain-list microbenchmark intact.
            let forward = if list != List::Plain {
                batch % 2 == 0
            } else {
                step % 2 == 0
            };
            assert!(
                source
                    .handle_event(wheel(if forward { 24. } else { -24. }))
                    .handled
            );
            if present {
                source
                    .render_retained_frame(&mut scene, &mut text, (320, 240), 1.)
                    .unwrap();
                peak_placed = peak_placed.max(source.placed_nodes().len());
            }
        }
        let elapsed = start.elapsed();
        if batch > 0 {
            samples.push(elapsed.as_secs_f64() * 1e6 / f64::from(steps));
        }
    }
    samples.sort_by(f64::total_cmp);
    let path = if list != List::Plain {
        "virtual + retained frame"
    } else if present {
        "event + retained frame"
    } else {
        "event only"
    };
    println!(
        "{list:?} {rows:4} rows  {path:24} {:8.2} us/event (median batch)",
        samples[samples.len() / 2]
    );
    if list != List::Plain {
        println!(
            "     peak {} row callbacks/declaration, {peak_placed} total placed nodes",
            source.view().peak_row_builds
        );
    }
}

fn main() {
    for rows in [100, 1000] {
        measure(rows, false, List::Plain);
        measure(rows, true, List::Plain);
    }
    for rows in [1_000, 1_000_000] {
        measure(rows, true, List::Fixed);
        measure(rows, true, List::Variable);
    }
}
