//! Exercise the actual host wait loop, including a quiet interval after settling.
use fltk_native::app;
use plico::{Theme, UiContext, View, widgets::text};
use std::{cell::RefCell, rc::Rc, time::Duration};
#[derive(Default)]
struct Samples {
    times: Vec<Duration>,
    settled_count: usize,
}
struct Timed {
    samples: Rc<RefCell<Samples>>,
}
impl View for Timed {
    fn theme(&self) -> Theme {
        Theme::default()
    }
    fn show(&mut self, cx: &mut UiContext) {
        text(cx, "Finite animation scheduler probe");
        let mut samples = self.samples.borrow_mut();
        if samples.times.last() != Some(&cx.now()) {
            samples.times.push(cx.now());
        }
        let elapsed = cx.now().saturating_sub(samples.times[0]);
        if elapsed < Duration::from_millis(100) {
            cx.request_animation_frame();
        } else if samples.settled_count == 0 {
            samples.settled_count = samples.times.len();
            // No ANIMATE request now: wait quietly, then close through a native timer.
            app::add_timeout3(0.08, |_| app::quit());
        }
    }
}
fn main() {
    let samples = Rc::new(RefCell::new(Samples::default()));
    plico_fltk::run(
        Timed {
            samples: samples.clone(),
        },
        "Frame pacing probe",
        400,
        160,
    )
    .unwrap();
    let samples = samples.borrow();
    assert!(
        (2..=12).contains(&samples.times.len()),
        "{:?}",
        samples.times
    );
    assert_eq!(
        samples.settled_count,
        samples.times.len(),
        "extra declarations after animation settled"
    );
    assert!(
        samples
            .times
            .windows(2)
            .all(|t| t[1] - t[0] >= Duration::from_millis(15)),
        "{:?}",
        samples.times
    );
    println!(
        "finite_native_frames={} deadline_pacing=true settled_host_sleeps=true",
        samples.times.len()
    );
}
