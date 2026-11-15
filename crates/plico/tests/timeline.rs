use plico::{
    DisplayPreferences, Easing, Engine, FrameTime, TimelineOffset, TimelineOptions, Tween,
    UiContext, after_previous, at_label, at_time, with_previous,
};
use std::time::Duration;

fn at(engine: &mut Engine, milliseconds: u64, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.set_time(FrameTime {
        now: Duration::from_millis(milliseconds),
        delta: Duration::ZERO,
    });
    engine.frame_declarations(view).unwrap();
}

fn linear(from: f32, to: f32, milliseconds: u64) -> Tween {
    Tween::new(from, to, Duration::from_millis(milliseconds)).easing(Easing::Linear)
}

#[test]
fn timeline_places_sequential_parallel_absolute_and_overlapping_tweens() {
    let mut engine = Engine::default();
    let mut values = [0.; 4];
    let mut build = |cx: &mut UiContext<'_>| {
        let mut timeline = cx.timeline("intro", TimelineOptions::default());
        values[0] = timeline.tween(linear(0., 100., 400));
        values[1] = timeline.tween_at(
            linear(0., 100., 300),
            after_previous(TimelineOffset::before(Duration::from_millis(150))),
        );
        values[2] = timeline.tween_at(linear(0., 100., 200), with_previous(Duration::ZERO));
        values[3] = timeline.tween_at(linear(0., 100., 100), at_time(Duration::from_millis(100)));
    };
    at(&mut engine, 0, &mut build);
    at(&mut engine, 300, &mut build);
    assert_eq!(values[0], 75.);
    assert!((values[1] - 100. / 6.).abs() < 0.001);
    assert!((values[2] - 25.).abs() < 0.001);
    assert_eq!(values[3], 100.);
}

#[test]
fn labels_stagger_tweens_from_one_named_point() {
    let mut engine = Engine::default();
    let mut rows = [0.; 3];
    let mut build = |cx: &mut UiContext<'_>| {
        let mut timeline = cx.timeline("rows", TimelineOptions::default());
        let _ = timeline.tween(linear(0., 1., 300));
        timeline.label("rows");
        for (index, row) in rows.iter_mut().enumerate() {
            *row = timeline.tween_at(
                linear(0., 1., 200),
                at_label("rows", Duration::from_millis(index as u64 * 40)),
            );
        }
    };
    at(&mut engine, 0, &mut build);
    at(&mut engine, 400, &mut build);
    assert_eq!(rows, [0.5, 0.3, 0.1]);
}

#[test]
fn pause_holds_elapsed_time_and_resume_continues_without_a_jump() {
    let mut engine = Engine::default();
    let mut value = 0.;
    at(&mut engine, 0, |cx| {
        let mut timeline = cx.timeline("pause", TimelineOptions::default());
        value = timeline.tween(linear(0., 1., 1000));
    });
    let mut paused = true;
    at(&mut engine, 200, |cx| {
        let mut timeline = cx.timeline(
            "pause",
            TimelineOptions {
                paused,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 1., 1000));
    });
    assert!((value - 0.2).abs() < 0.001);
    at(&mut engine, 800, |cx| {
        let mut timeline = cx.timeline(
            "pause",
            TimelineOptions {
                paused,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 1., 1000));
    });
    assert!((value - 0.2).abs() < 0.001);
    paused = false;
    at(&mut engine, 1000, |cx| {
        let mut timeline = cx.timeline(
            "pause",
            TimelineOptions {
                paused,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 1., 1000));
    });
    assert!((value - 0.2).abs() < 0.001);
    at(&mut engine, 1100, |cx| {
        let mut timeline = cx.timeline("pause", TimelineOptions::default());
        value = timeline.tween(linear(0., 1., 1000));
    });
    assert!((value - 0.3).abs() < 0.001);
}

#[test]
fn progress_seek_and_reverse_control_the_same_playhead() {
    let mut engine = Engine::default();
    let mut options = TimelineOptions {
        progress: Some(0.75),
        ..Default::default()
    };
    let mut value = 0.;
    at(&mut engine, 0, |cx| {
        let mut timeline = cx.timeline("controls", options);
        value = timeline.tween(linear(0., 100., 1000));
    });
    // The second declaration knows the duration constructed by the first.
    at(&mut engine, 0, |cx| {
        let mut timeline = cx.timeline("controls", options);
        value = timeline.tween(linear(0., 100., 1000));
    });
    assert_eq!(value, 75.);
    options.progress = None;
    options.reverse = true;
    at(&mut engine, 100, |cx| {
        let mut timeline = cx.timeline("controls", options);
        value = timeline.tween(linear(0., 100., 1000));
    });
    assert_eq!(
        value, 75.,
        "releasing a held playhead does not include wait time"
    );
    at(&mut engine, 350, |cx| {
        let mut timeline = cx.timeline("controls", options);
        value = timeline.tween(linear(0., 100., 1000));
    });
    assert_eq!(value, 50.);
}

#[test]
fn run_restarts_and_repeat_alternate_returns_to_the_start() {
    let mut engine = Engine::default();
    let mut run = 0;
    let mut value = 0.;
    at(&mut engine, 0, |cx| {
        let mut timeline = cx.timeline(
            "repeat",
            TimelineOptions {
                run,
                repeat: 1,
                alternate: true,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 100., 400));
    });
    at(&mut engine, 600, |cx| {
        let mut timeline = cx.timeline(
            "repeat",
            TimelineOptions {
                run,
                repeat: 1,
                alternate: true,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 100., 400));
    });
    assert_eq!(value, 50.);
    at(&mut engine, 800, |cx| {
        let mut timeline = cx.timeline(
            "repeat",
            TimelineOptions {
                run,
                repeat: 1,
                alternate: true,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 100., 400));
        assert!(timeline.settled());
    });
    assert_eq!(value, 0.);
    run += 1;
    at(&mut engine, 900, |cx| {
        let mut timeline = cx.timeline(
            "repeat",
            TimelineOptions {
                run,
                repeat: 1,
                alternate: true,
                ..Default::default()
            },
        );
        value = timeline.tween(linear(0., 100., 400));
    });
    assert_eq!(value, 0., "changing run restarts in the same declaration");
}

#[test]
fn reduced_motion_settles_spatial_motion_but_non_motion_still_plays() {
    let mut engine = Engine::default();
    let mut moved = 0.;
    let mut faded = 0.;
    let render = |engine: &mut Engine, milliseconds, moved: &mut f32, faded: &mut f32| {
        at(engine, milliseconds, |cx| {
            let mut movement = cx.timeline("move", TimelineOptions::default());
            *moved = movement.tween(linear(0., 100., 400));
            let mut fade = cx.timeline(
                "fade",
                TimelineOptions {
                    non_motion: true,
                    ..Default::default()
                },
            );
            *faded = fade.tween(linear(0., 1., 400));
        });
    };
    render(&mut engine, 0, &mut moved, &mut faded);
    engine.set_display_preferences(DisplayPreferences {
        reduce_motion: true,
        ..Default::default()
    });
    render(&mut engine, 200, &mut moved, &mut faded);
    assert_eq!(moved, 100.);
    assert_eq!(faded, 0.5);
}

#[test]
#[should_panic(expected = "only once per declaration")]
fn one_timeline_key_cannot_be_requested_twice_in_one_declaration() {
    let mut engine = Engine::default();
    at(&mut engine, 0, |cx| {
        let _first = cx.timeline("duplicate", TimelineOptions::default());
        drop(_first);
        let _second = cx.timeline("duplicate", TimelineOptions::default());
    });
}
