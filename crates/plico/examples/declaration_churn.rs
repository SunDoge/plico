//! Adversarial inputs for declaration fast paths: changing styles/order/membership.
use plico::{Color, Engine, UiContext};
use std::time::Instant;
fn show(cx: &mut UiContext<'_>, mode: &str, phase: usize) {
    for slot in 0..1000u64 {
        let id = match mode {
            "reverse" if phase == 1 => 999 - slot,
            "insert_remove" => slot + phase as u64,
            _ => slot,
        };
        let style = mode == "all_styles" && phase == 1;
        cx.row()
            .key(id)
            .opacity(if style { 0.5 } else { 1. })
            .children(|cx| {
                for _ in 0..2 {
                    cx.text("Stable text")
                        .font_size(if style { 18 } else { 14 })
                        .color(if style {
                            Color::Literal(0xff0000)
                        } else {
                            Color::Foreground
                        });
                }
            });
    }
}
fn main() {
    println!("mode,p50_us,p95_us,visited,created,removed");
    for mode in ["all_styles", "reverse", "insert_remove"] {
        let mut engine = Engine::default();
        let mut samples = Vec::new();
        let mut totals = [0; 3];
        for frame in 0..136 {
            let start = Instant::now();
            let stats = engine
                .frame_declarations(|cx| show(cx, mode, frame % 2))
                .unwrap();
            let us = start.elapsed().as_secs_f64() * 1e6;
            if frame >= 16 {
                assert_eq!(stats.visited, 3000);
                assert_eq!(
                    (stats.created, stats.removed),
                    if mode == "insert_remove" {
                        (3, 3)
                    } else {
                        (0, 0)
                    }
                );
                totals[0] += stats.visited;
                totals[1] += stats.created;
                totals[2] += stats.removed;
                samples.push(us);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{mode},{:.3},{:.3},{},{},{}",
            samples[60],
            samples[114],
            totals[0] / 120,
            totals[1] / 120,
            totals[2] / 120
        );
    }
}
