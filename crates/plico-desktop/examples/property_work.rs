//! Native backend CPU probe; no window, GPU submission or present.
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
enum Change {
    Color,
    Text,
    Size,
    Theme,
}
struct App {
    rows: usize,
    flip: bool,
    change: Change,
}
impl View for App {
    fn theme(&self) -> plico::Theme {
        let mut theme = plico::Theme::default();
        if self.flip && matches!(self.change, Change::Theme) {
            theme.panel = 0x334455;
        }
        theme
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(480).height(360).clip(true) + |cx| {
            for index in 0..self.rows {
                let active = index == 0 && self.flip;
                let fill = if active && matches!(self.change, Change::Color) {
                    Color::Accent
                } else {
                    Color::Panel
                };
                let width = if active && matches!(self.change, Change::Size) { 321 } else { 320 };
                let label = if active && matches!(self.change, Change::Text) { "Value 0001" } else { "Value 0000" };
                row(cx).key(index).width(width).height(24).shrink(0.).background(fill) + |cx| {
                    text(cx, label);
                };
            }
        };
    }
}
fn main() {
    println!(
        "rows,change,total_us,sync_us,layout_us,sync_visits,style_writes,paint_writes,layout_runs"
    );
    for rows in [32, 256, 1024] {
        for change in [Change::Color, Change::Text, Change::Size, Change::Theme] {
            let mut source = Source::with_clock(
                App {
                    rows,
                    flip: false,
                    change,
                },
                plico::VirtualClock::default(),
            );
            let mut text = TextContext::new();
            let mut scene = Scene::new();
            source
                .render_retained_frame(&mut scene, &mut text, (480, 360), 1.)
                .unwrap();
            let mut total = [0u128; 7];
            for index in 0..50 {
                source.view.flip = !source.view.flip;
                source.request_rebuild();
                let start = Instant::now();
                source
                    .render_retained_frame(&mut scene, &mut text, (480, 360), 1.)
                    .unwrap();
                let elapsed = start.elapsed().as_nanos();
                let stats = source.adapter.stats;
                assert_eq!(stats.created, 0);
                assert_eq!(stats.removed, 0);
                if matches!(change, Change::Color | Change::Theme) {
                    assert_eq!(stats.style_writes, 0);
                    assert_eq!(stats.layout_runs, 0);
                }
                if index >= 10 {
                    for (sum, value) in total.iter_mut().zip([
                        elapsed,
                        stats.sync_micros * 1000,
                        stats.layout_micros * 1000,
                        stats.sync_visits as u128,
                        stats.style_writes as u128,
                        stats.paint_writes as u128,
                        stats.layout_runs as u128,
                    ]) {
                        *sum += value;
                    }
                }
            }
            println!(
                "{rows},{change:?},{:.2},{:.2},{:.2},{},{},{},{}",
                total[0] as f64 / 40_000.,
                total[1] as f64 / 40_000.,
                total[2] as f64 / 40_000.,
                total[3] / 40,
                total[4] / 40,
                total[5] / 40,
                total[6] / 40
            );
        }
    }
}
