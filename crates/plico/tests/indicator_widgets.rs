//! MyGO's indicator widgets: the leveled meter, the spinner and the star
//! rating. Interaction runs through the real Engine so clicks, node reuse
//! and the injected frame clock behave exactly as in an application.
use plico::prelude::*;
use plico::{Engine, Entity, FrameTime, Kind, NodeRef, Theme};
use std::time::Duration;

fn frame(engine: &mut Engine, view: impl FnOnce(&mut UiContext<'_>)) {
    engine.frame_declarations(view).unwrap();
}

fn collect<T>(
    engine: &Engine,
    id: Entity,
    visit: &mut impl FnMut(Entity, NodeRef<'_>) -> Option<T>,
    out: &mut Vec<T>,
) {
    let node = engine.node(id).unwrap();
    let children = node.children.to_vec();
    if let Some(value) = visit(id, node) {
        out.push(value);
    }
    for child in children {
        collect(engine, child, visit, out);
    }
}

fn buttons(engine: &Engine) -> Vec<Entity> {
    let mut out = Vec::new();
    collect(
        engine,
        engine.root(),
        &mut |entity, node| (node.kind == Kind::ButtonBase).then_some(entity),
        &mut out,
    );
    out
}

#[test]
fn meter_colors_its_fill_by_the_crossed_level() {
    for (value, expected) in [
        (20.0, Color::Success),
        (85.0, Color::Warning),
        (99.0, Color::Danger),
    ] {
        let mut engine = Engine::default();
        frame(&mut engine, |cx| {
            meter(
                cx,
                "Disk",
                value,
                0.0..=100.0,
                MeterLevels {
                    warning: Some(80.),
                    critical: Some(95.),
                },
            );
        });
        let mut colors = Vec::new();
        collect(
            &engine,
            engine.root(),
            &mut |_, node| node.style.background,
            &mut colors,
        );
        assert!(
            colors.contains(&expected),
            "value {value} should paint {expected:?}, got {colors:?}"
        );
    }
}

#[test]
fn meter_treats_a_low_critical_level_as_the_bad_end() {
    // A battery: critical below warning means low values are bad (MyGO).
    for (value, expected) in [
        (80.0, Color::Success),
        (20.0, Color::Warning),
        (5.0, Color::Danger),
    ] {
        let mut engine = Engine::default();
        frame(&mut engine, |cx| {
            meter(
                cx,
                "Battery",
                value,
                0.0..=100.0,
                MeterLevels {
                    warning: Some(30.),
                    critical: Some(10.),
                },
            );
        });
        let mut colors = Vec::new();
        collect(
            &engine,
            engine.root(),
            &mut |_, node| node.style.background,
            &mut colors,
        );
        assert!(
            colors.contains(&expected),
            "value {value} should paint {expected:?}, got {colors:?}"
        );
    }
}

#[test]
fn meter_clamps_the_fill_and_shows_none_for_non_finite_values() {
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        meter(cx, "Full", 250.0, 0.0..=100.0, MeterLevels::default());
    });
    let mut full = Vec::new();
    collect(
        &engine,
        engine.root(),
        &mut |_, node| {
            (node.style.background == Some(Color::Success)
                && node.style.width_percent == Some(100.))
            .then_some(node.style.width_percent)
        },
        &mut full,
    );
    assert_eq!(
        full,
        vec![Some(100.)],
        "values above the range fill the bar"
    );

    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        meter(cx, "Broken", f64::NAN, 0.0..=100.0, MeterLevels::default());
    });
    let mut empty = Vec::new();
    collect(
        &engine,
        engine.root(),
        &mut |_, node| {
            (node.style.background == Some(Color::Success) && node.style.width_percent == Some(0.))
                .then_some(node.style.width_percent)
        },
        &mut empty,
    );
    assert_eq!(empty, vec![Some(0.)], "non-finite values show an empty bar");
}

#[test]
fn spinner_animates_its_trail_with_the_injected_clock() {
    fn dots(engine: &Engine) -> Vec<f32> {
        let mut out = Vec::new();
        collect(
            engine,
            engine.root(),
            &mut |_, node| {
                (node.style.background == Some(Color::Muted) && node.style.width.is_some())
                    .then_some(node.style.opacity)
            },
            &mut out,
        );
        out
    }
    let mut engine = Engine::default();
    engine.set_time(FrameTime {
        now: Duration::ZERO,
        delta: Duration::ZERO,
    });
    frame(&mut engine, spinner);
    let first = dots(&engine);
    assert_eq!(first.len(), 12, "the spinner paints twelve spokes");
    engine.set_time(FrameTime {
        now: Duration::from_millis(75),
        delta: Duration::from_millis(75),
    });
    frame(&mut engine, spinner);
    let second = dots(&engine);
    assert_eq!(second.len(), 12);
    assert_ne!(first, second, "the fading trail advances with frame time");
}

#[test]
fn rating_sets_and_clears_the_stars() {
    let mut stars = 0_u8;
    let mut engine = Engine::default();
    frame(&mut engine, |cx| {
        assert!(!rating(cx, &mut stars, 5));
    });
    let stars_nodes = buttons(&engine);
    assert_eq!(stars_nodes.len(), 5, "one button per star");
    let names = stars_nodes
        .iter()
        .map(|entity| engine.node(*entity).unwrap().text.to_string())
        .collect::<Vec<_>>();
    assert_eq!(names[2], "3 of 5 stars");

    engine.queue_click(stars_nodes[2]);
    let mut changed = false;
    frame(&mut engine, |cx| {
        changed = rating(cx, &mut stars, 5);
    });
    assert_eq!(stars, 3, "a click sets that star");
    assert!(changed);

    engine.queue_click(stars_nodes[2]);
    frame(&mut engine, |cx| {
        changed = rating(cx, &mut stars, 5);
    });
    assert_eq!(stars, 0, "clicking the set star clears the rating");
    assert!(changed);
}

#[test]
fn rating_clamps_an_out_of_range_value_and_reports_the_clamp() {
    let mut stars = 9_u8;
    let mut engine = Engine::default();
    let mut changed = false;
    frame(&mut engine, |cx| {
        changed = rating(cx, &mut stars, 5);
    });
    assert_eq!(stars, 5);
    assert!(changed, "clamping to the maximum is a reported change");
}

#[test]
fn semantic_colors_resolve_from_the_theme_with_legacy_defaults() {
    let dark = Theme::dark();
    assert_eq!(dark.resolve(Color::Success), dark.success);
    assert_eq!(dark.resolve(Color::Warning), dark.warning);
    assert_eq!(dark.resolve(Color::Danger), dark.danger);
    let legacy: Theme = serde_json::from_str(r#"{"surface":16777215}"#).unwrap();
    let defaults = Theme::default();
    assert_eq!(legacy.success, defaults.success);
    assert_eq!(legacy.warning, defaults.warning);
    assert_eq!(legacy.danger, defaults.danger);
}
