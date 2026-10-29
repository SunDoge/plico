use plico::{Changes, Entity, prelude::*};
use plico_renderer::TextContext;
use plico_runtime::{
    Source,
    regions::{Access, Reactive, Regions, State, TextProperties, RegionUi},
};

fn update(node: &mut TextProperties, (value, color): (Access<'_, String>, Access<'_, Color>)) {
    node.text(value.read().clone()).color(*color.read());
}
fn fixture() -> (Source<Regions>, State<String>, State<Color>, State<bool>) {
    let mut handles = None;
    let app = Regions::new(|app| {
        let value = app.state(|| "first".to_owned());
        let color = app.state(|| Color::Accent);
        let visible = app.state(|| true);
        handles = Some((value, color, visible));
        app.root((value, color, visible), root);
    });
    let (value, color, visible) = handles.unwrap();
    (Source::new(app), value, color, visible)
}
#[component]
fn root(
    cx: &mut UiContext<'_>,
    (value, color, visible): (Access<'_, String>, Access<'_, Color>, Access<'_, bool>),
) {
    column(cx).key("shell").gap(10).children(|cx| {
        text(cx, "Static").key("static");
        cx.region((visible.handle(),), |cx, (visible,)| {
            text(cx, if *visible.read() { "visible" } else { "hidden" });
        });
        // The parent captures these IDs, but only this leaf subscribes to them.
        text(cx, "builder fallback")
            .key("dynamic")
            .font_size(24)
            .rx((value.handle(), color.handle()), update);
        text(cx, "Sibling").key("sibling");
    });
}
fn frame(source: &mut Source<Regions>, text: &mut TextContext, width: u32) {
    source.experiment_prepare_without_scene(text, width, 700);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn target(source: &Source<Regions>) -> Entity {
    source
        .view
        .region_runs()
        .into_iter()
        .find(|(id, _)| {
            source
                .engine
                .node(*id)
                .is_some_and(|n| n.kind == plico::Kind::Text)
        })
        .unwrap()
        .0
}
#[test]
fn property_updates_skip_declarations_keep_identity_and_invalidate_correct_channels() {
    let (mut source, value, color, _) = fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    let id = target(&source);
    let live = source.engine.experiment_live_nodes();
    assert_eq!(live, 8, "property binding must not add a container node");
    let before = source.view.region_runs();
    assert_eq!(source.engine.node(id).unwrap().text, "first");
    source.view.access(color).set(Color::Literal(0xff0000));
    source.try_declare().unwrap();
    assert!(source.engine.changes().contains(Changes::PAINT));
    assert!(
        !source
            .engine
            .changes()
            .intersects(Changes::LAYOUT | Changes::MEASURE | Changes::STRUCTURE)
    );
    assert_eq!(source.engine.dirty_nodes(), &[id]);
    frame(&mut source, &mut text, 800);
    assert_eq!(
        source.engine.node(id).unwrap().style.foreground,
        Color::Literal(0xff0000)
    );
    source
        .view
        .access(value)
        .set("a considerably longer dynamic label".into());
    source.try_declare().unwrap();
    assert!(
        source
            .engine
            .changes()
            .contains(Changes::MEASURE | Changes::LAYOUT | Changes::SEMANTICS)
    );
    assert_eq!(source.engine.last_stats().visited, 0);
    frame(&mut source, &mut text, 800);
    assert_eq!(target(&source), id);
    assert_eq!(source.engine.experiment_live_nodes(), live);
    let after = source.view.region_runs();
    for (entity, runs) in before {
        if entity != id {
            assert_eq!(after.iter().find(|(e, _)| *e == entity).unwrap().1, runs);
        }
    }
    source.view.access(value).notify();
    source.try_declare().unwrap();
    assert!(
        source.engine.changes().is_empty(),
        "equal values must not dirty the node"
    );
}
#[test]
fn full_redeclaration_and_resize_reapply_bound_values() {
    let (mut source, value, color, _) = fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    source
        .view
        .access(value)
        .set("dynamic survives rebuild".into());
    source.view.access(color).set(Color::Muted);
    frame(&mut source, &mut text, 800);
    let id = target(&source);
    source.request_rebuild();
    frame(&mut source, &mut text, 800);
    frame(&mut source, &mut text, 480);
    assert_eq!(target(&source), id);
    assert_eq!(
        source.engine.node(id).unwrap().text,
        "dynamic survives rebuild"
    );
    assert_eq!(
        source.engine.node(id).unwrap().style.foreground,
        Color::Muted
    );
}
#[test]
fn unmount_cleans_property_subscriptions_and_remount_uses_new_generation() {
    let mut handles = None;
    let app = Regions::new(|app| {
        let visible = app.state(|| true);
        let value = app.state(|| 1);
        handles = Some((visible, value));
        app.root((visible, value), |cx, (visible, value)| {
            cx.region(
                (visible.handle(), value.handle()),
                |cx, (visible, value)| {
                    if *visible.read() {
                        cx.component((value.handle(),), |cx, (value,)| {
                            let local = cx.local_state(|| 10);
                            text_empty(cx).rx((value.handle(), local), |node, (value, local)| {
                                node.text(format!("{}:{}", *value.read(), *local.read()));
                            });
                        });
                    }
                },
            );
        });
    });
    let (visible, value) = handles.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    let old = target(&source);
    source.view.access(visible).set(false);
    source.view.access(value).set(2);
    frame(&mut source, &mut text, 800);
    assert!(source.engine.node(old).is_none());
    let empty = source.view.region_runs().len();
    for value in 3..8 {
        source.view.access(handles.unwrap().1).set(value);
        frame(&mut source, &mut text, 800);
        assert_eq!(source.view.region_runs().len(), empty);
    }
    source.view.access(visible).set(true);
    frame(&mut source, &mut text, 800);
    let new = target(&source);
    assert_eq!(source.engine.node(new).unwrap().text, "7:10");
    assert_ne!(old, new);
}

#[test]
fn dependency_rebinding_unsubscribes_old_state_without_accumulating_bindings() {
    use plico_runtime::View;
    let mut handles = None;
    let app = Regions::new(|app| {
        let choose = app.state(|| false);
        let a = app.state(|| 1);
        let b = app.state(|| 2);
        let bundle = app.state(|| (a, b));
        handles = Some((choose, a, b));
        app.root((choose, bundle), |cx, (choose, bundle)| {
            cx.region(
                (choose.handle(), bundle.handle()),
                |cx, (choose, bundle)| {
                    let (a, b) = *bundle.read();
                    let selected = if *choose.read() { b } else { a };
                    text_empty(cx)
                        .key("bound")
                        .rx((selected,), |node, (value,)| {
                            node.text(value.read().to_string());
                        });
                },
            );
        });
    });
    let (choose, a, b) = handles.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    let count = source.view.region_runs().len();
    let id = target(&source);
    for round in 0..20 {
        let use_b = round % 2 == 0;
        source.view.access(choose).set(use_b);
        frame(&mut source, &mut text, 800);
        assert_eq!(source.view.region_runs().len(), count);
        assert_eq!(target(&source), id);
        let (selected, unused) = if use_b { (b, a) } else { (a, b) };
        source.view.access(unused).set(100 + round);
        assert!(
            !source.view.snapshot_changed(),
            "old dependency still subscribed"
        );
        source.view.access(selected).set(200 + round);
        frame(&mut source, &mut text, 800);
        assert_eq!(
            source.engine.node(id).unwrap().text,
            (200 + round).to_string()
        );
    }
}

#[test]
fn property_callbacks_cannot_write_state() {
    let mut state = None;
    let app = Regions::new(|app| {
        let value = app.state(|| false);
        state = Some(value);
        app.root((value,), |cx, (value,)| {
            text_empty(cx).rx((value.handle(),), |node, (value,)| {
                node.text("safe read");
                if *value.read() {
                    value.set(false);
                }
            });
        });
    });
    let mut source = Source::new(app);
    source.try_declare().unwrap();
    source.view.access(state.unwrap()).set(true);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.try_declare()));
    assert!(panic.is_err());
}

#[test]
fn host_completion_updates_property_without_replaying_parent() {
    let (mut source, value, _, _) = fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    let before = source.view.region_runs();
    let id = target(&source);
    let sender = source.view.sender(value);
    sender.send("host result".to_owned()).unwrap();
    assert!(plico_renderer::FrameSource::poll_async(&mut source));
    frame(&mut source, &mut text, 800);
    assert_eq!(source.engine.node(id).unwrap().text, "host result");
    assert_eq!(source.declaration_stats.visited, 0);
    for (entity, runs) in before {
        if entity != id {
            assert_eq!(
                source
                    .view
                    .region_runs()
                    .iter()
                    .find(|(e, _)| *e == entity)
                    .unwrap()
                    .1,
                runs
            );
        }
    }
}

#[test]
fn rich_text_target_is_explicitly_rejected() {
    let app = Regions::new(|app| {
        app.root((), |cx, ()| {
            text(cx, "abc")
                .spans(&[plico::TextSpan {
                    range: 0..3,
                    weight: 700,
                    italic: false,
                    color: None,
                }])
                .rx((), |node, ()| {
                    node.text("changed");
                });
        });
    });
    let mut source = Source::new(app);
    assert!(matches!(
        source.try_declare(),
        Err(plico_runtime::SourceError::Declaration {
            source: plico::FrameError::InvalidRegion
        })
    ));
}

#[test]
fn omitted_properties_restore_builder_baseline_on_local_and_full_updates() {
    let mut state = None;
    let app = Regions::new(|app| {
        let active = app.state(|| true);
        state = Some(active);
        app.root((active,), |cx, (active,)| {
            text(cx, "baseline")
                .color(Color::Muted)
                .rx(active.handle(), |node, active| {
                    if *active.read() {
                        node.text("override").color(Color::Accent);
                    }
                });
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 800);
    let id = target(&source);
    assert_eq!(source.engine.node(id).unwrap().text, "override");
    source.view.access(state.unwrap()).set(false);
    frame(&mut source, &mut text, 800);
    assert_eq!(source.engine.node(id).unwrap().text, "baseline");
    assert_eq!(
        source.engine.node(id).unwrap().style.foreground,
        Color::Muted
    );
    source.request_rebuild();
    frame(&mut source, &mut text, 800);
    assert_eq!(source.engine.node(id).unwrap().text, "baseline");
    assert_eq!(
        source.engine.node(id).unwrap().style.foreground,
        Color::Muted
    );
}

#[test]
fn multiline_property_update_reflows_siblings_like_full_declaration() {
    let (mut source, value, _, _) = fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 480);
    let label = "first line\nsecond line\nthird line";
    source.view.access(value).set(label.to_owned());
    frame(&mut source, &mut text, 480);
    let geometry = |source: &Source<Regions>| {
        source
            .adapter
            .placed
            .iter()
            .map(|p| (p.paint.text.clone(), p.rect))
            .collect::<Vec<_>>()
    };
    let local = geometry(&source);
    let rect = source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some(label))
        .unwrap()
        .rect;
    assert!(
        rect[3] - rect[1] > 60.,
        "text measurement did not expand for three lines"
    );
    source.request_rebuild();
    frame(&mut source, &mut text, 480);
    assert_eq!(geometry(&source), local);
}

#[derive(Clone, Copy)]
struct Typography {
    size: u16,
    weight: u16,
    family: FontFamily,
    opacity: f32,
    override_baseline: bool,
}
impl Default for Typography {
    fn default() -> Self {
        Self {
            size: 18,
            weight: 400,
            family: FontFamily::Monospace,
            opacity: 0.8,
            override_baseline: true,
        }
    }
}
fn typography_fixture() -> (Source<Regions>, State<Typography>) {
    let mut state = None;
    let app = Regions::new(|app| {
        let attributes = app.state(Typography::default);
        state = Some(attributes);
        app.root((attributes,), |cx, (attributes,)| {
            column(cx).children(|cx| {
                text(cx, "Typography")
                    .font_size(18)
                    .weight(400)
                    .font_family(FontFamily::Monospace)
                    .opacity(0.8)
                    .rx((attributes.handle(),), |node, (attributes,)| {
                        let attributes = attributes.read();
                        if attributes.override_baseline {
                            node.font_size(attributes.size)
                                .weight(attributes.weight)
                                .font_family(attributes.family)
                                .opacity(attributes.opacity);
                            if attributes.size == 0 {
                                node.text("must not apply invalid text");
                            }
                        }
                    });
                text(cx, "Following sibling");
            });
        });
    });
    (Source::new(app), state.unwrap())
}
#[test]
fn typography_reflows_and_opacity_only_repaints_with_baseline_restoration() {
    let (mut source, attributes) = typography_fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 600);
    let id = target(&source);
    let parents = source.view.region_runs();
    let mut values = Typography {
        opacity: 0.3,
        ..Typography::default()
    };
    source.view.access(attributes).set(values);
    source.try_declare().unwrap();
    assert!(source.engine.changes().contains(Changes::PAINT));
    assert!(
        !source
            .engine
            .changes()
            .intersects(Changes::MEASURE | Changes::LAYOUT | Changes::STRUCTURE)
    );
    frame(&mut source, &mut text, 600);
    assert_eq!(source.engine.node(id).unwrap().style.opacity, 0.3);
    values.size = 34;
    values.weight = 700;
    values.family = FontFamily::Serif;
    source.view.access(attributes).set(values);
    source.try_declare().unwrap();
    assert!(
        source
            .engine
            .changes()
            .contains(Changes::MEASURE | Changes::LAYOUT)
    );
    assert_eq!(source.engine.dirty_nodes(), &[id]);
    frame(&mut source, &mut text, 600);
    let node = source.engine.node(id).unwrap();
    assert_eq!(
        (
            node.style.font_size,
            node.style.font_weight,
            node.style.font_family
        ),
        (34, 700, FontFamily::Serif)
    );
    assert_eq!(source.declaration_stats.visited, 0);
    for (entity, runs) in parents {
        if entity != id {
            assert_eq!(
                source
                    .view
                    .region_runs()
                    .iter()
                    .find(|(e, _)| *e == entity)
                    .unwrap()
                    .1,
                runs
            );
        }
    }
    let local = source
        .adapter
        .placed
        .iter()
        .map(|p| p.rect)
        .collect::<Vec<_>>();
    source.request_rebuild();
    frame(&mut source, &mut text, 600);
    assert_eq!(
        source
            .adapter
            .placed
            .iter()
            .map(|p| p.rect)
            .collect::<Vec<_>>(),
        local
    );
    values.override_baseline = false;
    source.view.access(attributes).set(values);
    frame(&mut source, &mut text, 600);
    let style = source.engine.node(id).unwrap().style;
    assert_eq!(
        (
            style.font_size,
            style.font_weight,
            style.font_family,
            style.opacity
        ),
        (18, 400, FontFamily::Monospace, 0.8)
    );
    source.view.access(attributes).notify();
    source.try_declare().unwrap();
    assert!(source.engine.changes().is_empty());
}
#[test]
fn invalid_dynamic_attributes_are_rejected_before_mutation_and_recover() {
    let (mut source, state) = typography_fixture();
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 600);
    let id = target(&source);
    let valid = Typography::default();
    for invalid in [
        Typography { size: 0, ..valid },
        Typography {
            weight: 99,
            ..valid
        },
        Typography {
            weight: 901,
            ..valid
        },
        Typography {
            opacity: f32::NAN,
            ..valid
        },
        Typography {
            opacity: f32::INFINITY,
            ..valid
        },
        Typography {
            opacity: -0.1,
            ..valid
        },
        Typography {
            opacity: 1.1,
            ..valid
        },
    ] {
        source.view.access(state).set(invalid);
        assert!(matches!(
            source.try_declare(),
            Err(plico_runtime::SourceError::Declaration {
                source: plico::FrameError::InvalidStyle
            })
        ));
        let node = source.engine.node(id).unwrap();
        assert_eq!(node.text, "Typography");
        assert_eq!(
            (
                node.style.font_size,
                node.style.font_weight,
                node.style.opacity
            ),
            (18, 400, 0.8)
        );
        source.view.access(state).set(valid);
        source.try_declare().unwrap();
        frame(&mut source, &mut text, 600);
    }
}

#[test]
fn global_theme_refresh_preserves_dynamic_typography_and_resolves_color_tokens() {
    let mut states = None;
    let app = Regions::new(|app| {
        let theme = app.state(Theme::light);
        let size = app.state(|| 28_u16);
        app.theme_state(theme);
        states = Some((theme, size));
        app.root((size,), |cx, (size,)| {
            text(cx, "Themed").rx((size.handle(),), |node, (size,)| {
                node.font_size(*size.read())
                    .weight(600)
                    .color(Color::Accent)
                    .opacity(0.8);
            });
        });
    });
    let (theme, size) = states.unwrap();
    let mut source = Source::new(app);
    source.set_reduced_motion(true);
    let mut text = TextContext::new();
    frame(&mut source, &mut text, 600);
    let id = target(&source);
    for theme_value in [Theme::dark(), Theme::light()] {
        source.view.access(theme).set(theme_value);
        source.view.access(size).set(32);
        frame(&mut source, &mut text, 600);
        assert_eq!(source.engine.theme(), theme_value);
        assert_eq!(target(&source), id);
        let style = source.engine.node(id).unwrap().style;
        assert_eq!(
            (
                style.font_size,
                style.font_weight,
                style.foreground,
                style.opacity
            ),
            (32, 600, Color::Accent, 0.8)
        );
        let paint = source
            .adapter
            .placed
            .iter()
            .find(|p| p.paint.text.as_deref() == Some("Themed"))
            .unwrap();
        let rgb = theme_value.accent;
        assert_eq!(
            paint.paint.text_color,
            peniko::Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        );
    }
}

#[test]
fn ordinary_builder_and_reactive_chains_produce_identical_text_attributes() {
    #[derive(Clone)]
    struct Values {
        text: String,
        color: Color,
        size: u16,
        weight: u16,
        family: FontFamily,
        opacity: f32,
    }
    fn make(reactive: bool, initial: Values) -> (Source<Regions>, State<Values>) {
        let mut state = None;
        let app = Regions::new(|app| {
            let values = app.state(|| initial);
            let reactive = app.state(|| reactive);
            state = Some(values);
            app.root((values, reactive), |cx, (values, reactive)| {
                if *reactive.read() {
                    text_empty(cx)
                        .key("label")
                        .rx((values.handle(),), |node, (values,)| {
                            let values = values.read();
                            node.text(values.text.clone())
                                .color(values.color)
                                .font_size(values.size)
                                .weight(values.weight)
                                .font_family(values.family)
                                .opacity(values.opacity);
                        });
                } else {
                    let values = values.read();
                    text(cx, &values.text)
                        .key("label")
                        .color(values.color)
                        .font_size(values.size)
                        .weight(values.weight)
                        .font_family(values.family)
                        .opacity(values.opacity);
                }
            });
        });
        (Source::new(app), state.unwrap())
    }
    let initial = Values {
        text: "First".into(),
        color: Color::Accent,
        size: 16,
        weight: 400,
        family: FontFamily::SansSerif,
        opacity: 1.,
    };
    let (mut ordinary, ordinary_state) = make(false, initial.clone());
    let (mut reactive, reactive_state) = make(true, initial.clone());
    let (mut ordinary_text, mut reactive_text) = (TextContext::new(), TextContext::new());
    for values in [
        initial,
        Values {
            text: "Second\nline".into(),
            color: Color::Muted,
            size: 32,
            weight: 700,
            family: FontFamily::Monospace,
            opacity: 0.4,
        },
        Values {
            text: "Third".into(),
            color: Color::Literal(0x345678),
            size: 12,
            weight: 450,
            family: FontFamily::Serif,
            opacity: 0.,
        },
    ] {
        ordinary.view.access(ordinary_state).set(values.clone());
        ordinary.request_rebuild();
        reactive.view.access(reactive_state).set(values);
        frame(&mut ordinary, &mut ordinary_text, 480);
        frame(&mut reactive, &mut reactive_text, 480);
        assert_eq!(
            serde_json::to_value(ordinary.engine.snapshot()).unwrap(),
            serde_json::to_value(reactive.engine.snapshot()).unwrap()
        );
        assert_eq!(
            ordinary
                .adapter
                .placed
                .iter()
                .map(|p| p.rect)
                .collect::<Vec<_>>(),
            reactive
                .adapter
                .placed
                .iter()
                .map(|p| p.rect)
                .collect::<Vec<_>>()
        );
    }
    let valid = Values {
        text: "Valid".into(),
        color: Color::Accent,
        size: 16,
        weight: 400,
        family: FontFamily::Default,
        opacity: 1.,
    };
    for invalid in [
        Values {
            size: 0,
            ..valid.clone()
        },
        Values {
            weight: 901,
            ..valid.clone()
        },
        Values {
            opacity: f32::NAN,
            ..valid.clone()
        },
    ] {
        ordinary.view.access(ordinary_state).set(invalid.clone());
        reactive.view.access(reactive_state).set(invalid);
        assert!(matches!(
            ordinary.try_declare(),
            Err(plico_runtime::SourceError::Declaration {
                source: plico::FrameError::InvalidStyle
            })
        ));
        assert!(matches!(
            reactive.try_declare(),
            Err(plico_runtime::SourceError::Declaration {
                source: plico::FrameError::InvalidStyle
            })
        ));
        ordinary.view.access(ordinary_state).set(valid.clone());
        reactive.view.access(reactive_state).set(valid.clone());
        ordinary.try_declare().unwrap();
        reactive.try_declare().unwrap();
    }
}
