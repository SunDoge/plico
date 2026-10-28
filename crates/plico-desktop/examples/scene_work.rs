//! Full scene encoding and cache-reuse costs. CPU only; no GPU or window.
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};

#[derive(usage_rs::Cli)]
#[usage(about = "Measure full scene encoding, declaration and cache reuse")]
struct Options {
    #[usage(long, default = "120")]
    samples: usize,
    #[usage(long)]
    rows: Option<usize>,
    /// Synthetic tall viewport: all rows visible instead of normal culling.
    #[usage(long)]
    unculled: bool,
}
#[derive(Clone, Copy, Debug)]
enum Case {
    Idle,
    Redeclare,
    ColorOne,
    ColorAll,
    TextOne,
    SizeOne,
    Opacity,
    Structure,
}
struct App {
    rows: usize,
    case: Case,
    flip: bool,
    calls: usize,
}
impl View for App {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        self.calls += 1;
        let alpha = if self.flip && matches!(self.case, Case::Opacity) { 0.8 } else { 1. };
        column(cx).width_percent(100.).height_percent(100.).clip(true).opacity(alpha) + |cx| {
            for index in 0..self.rows {
                let first = index == 0;
                let colored = self.flip
                    && (matches!(self.case, Case::ColorAll)
                        || first && matches!(self.case, Case::ColorOne));
                let fill = if colored { Color::Accent } else { Color::Panel };
                let width = if first && self.flip && matches!(self.case, Case::SizeOne) { 781 } else { 780 };
                let label = if first && self.flip && matches!(self.case, Case::TextOne) {
                    "Retained row B"
                } else {
                    "Retained row A"
                };
                row(cx).key(index).width(width).height(24).shrink(0.).background(fill) + |cx| {
                    text(cx, label);
                };
            }
            if self.flip && matches!(self.case, Case::Structure) {
                text(cx, "Conditional child");
            }
        };
    }
}
fn percentile(values: &mut [f64], percentile: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * percentile).round() as usize]
}
fn run(rows: usize, case: Case, options: &Options) {
    let height = if options.unculled {
        (rows as u32 + 2) * 24
    } else {
        600
    };
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        App {
            rows,
            case,
            flip: false,
            calls: 0,
        },
        clock.clone(),
    );
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (800, height), 1.)
        .unwrap();
    let mut total = Vec::with_capacity(options.samples);
    let mut encode = Vec::with_capacity(options.samples);
    let mut phases = [0u128; 3];
    let mut counts = [0usize; 4];
    let mut verified = 0;
    for frame in 0..options.samples + 20 {
        clock.advance(Duration::from_millis(16)).unwrap();
        source.view.flip = frame % 2 == 0;
        if !matches!(case, Case::Idle) {
            source.request_rebuild();
        }
        let calls = source.view.calls;
        let start = Instant::now();
        let (_, _, scene_ms) = source
            .render_retained_frame(&mut scene, &mut text, (800, height), 1.)
            .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1e6;
        assert!(source.last_error.is_none());
        if matches!(case, Case::Idle | Case::Redeclare) {
            assert_eq!(source.adapter.stats.scene_reuses, 1);
            assert_eq!(source.adapter.stats.scene_builds, 0);
            assert_eq!(
                source.view.calls - calls,
                usize::from(matches!(case, Case::Redeclare))
            );
        } else {
            assert_eq!(source.adapter.stats.scene_builds, 1);
        }
        if frame >= 20 {
            total.push(elapsed);
            encode.push(scene_ms * 1000.);
            phases[0] += source.declaration_micros;
            phases[1] += source.adapter.stats.sync_micros;
            phases[2] += source.adapter.stats.layout_micros;
            counts[0] += source.adapter.stats.scene_builds;
            counts[1] += source.adapter.stats.scene_reuses;
            counts[2] += source.view.calls - calls;
            counts[3] += scene.commands.len();
        } else if frame < 8 {
            // Check every workload against a full encoder outside timing.
            // Counting glyphs alone cannot detect leaked clips/opacity.
            let nodes = source.build_frame(&mut text, 800, height);
            let mut full = Scene::new();
            plico_renderer::scene::build_scene_scaled_layers(
                &mut full,
                &[&nodes],
                &mut text,
                800,
                height,
                source.base_color(),
                1.,
            );
            assert_eq!(
                scene.commands.as_slice(),
                full.commands.as_slice(),
                "{case:?} frame {frame}"
            );
            verified += 1;
        }
    }
    let n = options.samples as f64;
    println!(
        "{rows},{},{case:?},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{verified}",
        if options.unculled { "tall" } else { "viewport" },
        options.samples,
        percentile(&mut total, 0.5),
        percentile(&mut total, 0.95),
        phases[0] as f64 / n,
        phases[1] as f64 / n,
        phases[2] as f64 / n,
        encode.iter().sum::<f64>() / n,
        percentile(&mut encode, 0.5),
        percentile(&mut encode, 0.95),
        counts[0],
        counts[1],
        counts[2],
        counts[3] / options.samples
    );
}
fn main() {
    let options = Options::parse();
    assert!(options.samples > 0 && options.rows != Some(0));
    println!(
        "rows,visibility,case,samples,total_p50_us,total_p95_us,declare_mean_us,sync_mean_us,geometry_mean_us,scene_mean_us,scene_p50_us,scene_p95_us,full_frames,reused_frames,show_calls,commands,verified_frames"
    );
    for rows in options
        .rows
        .map_or_else(|| vec![100, 1000, 4000], |rows| vec![rows])
    {
        for case in [
            Case::Idle,
            Case::Redeclare,
            Case::ColorOne,
            Case::ColorAll,
            Case::TextOne,
            Case::SizeOne,
            Case::Opacity,
            Case::Structure,
        ] {
            run(rows, case, &options);
        }
    }
}
