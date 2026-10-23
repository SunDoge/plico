use plico_core::*;
fn text(engine: &mut Engine, label: &str, color: Color) -> FrameStats {
    engine
        .frame(|ui| {
            ui.text(label).key("label").color(color);
        })
        .unwrap()
}
#[test]
fn slots_reuse_storage_without_reviving_stale_handles() {
    let mut arena = Arena::default();
    let old = arena.insert("old");
    assert_eq!(arena.remove(old), Some("old"));
    let new = arena.insert("new");
    assert_ne!(new, old);
    assert_eq!(arena.len(), 1);
    assert!(arena.get(old).is_none());
    assert!(arena.get_mut(old).is_none());
    assert!(arena.remove(old).is_none());
    assert_eq!(arena.get(new), Some(&"new"));
    assert_eq!(arena.len(), 1);
}
#[test]
fn static_frame_reuses_nodes_and_skips_measurement() {
    let mut engine = Engine::default();
    assert_eq!(text(&mut engine, "hello", Color::Foreground).created, 1);
    let id = engine.find(engine.root(), "label").unwrap();
    let stats = text(&mut engine, "hello", Color::Foreground);
    assert_eq!(
        (
            stats.created,
            stats.reused,
            stats.measure_calls,
            stats.layout_runs,
            stats.paint_updates
        ),
        (0, 1, 0, 0, 0)
    );
    assert_eq!(engine.find(engine.root(), "label"), Some(id));
}
#[test]
fn color_changes_skip_measurement_and_layout() {
    let mut engine = Engine::default();
    text(&mut engine, "hello", Color::Foreground);
    let stats = text(&mut engine, "hello", Color::Literal(0xff0000));
    assert_eq!(
        (stats.measure_calls, stats.layout_runs, stats.paint_updates),
        (0, 0, 1)
    );
}
#[test]
fn content_changes_measure_only_affected_leaf() {
    let mut engine = Engine::default();
    let view = |ui: &mut Ui<'_>, edited: bool| {
        for i in 0..100usize {
            ui.text(if edited && i == 42 {
                "edited"
            } else {
                "stable"
            })
            .key(i);
        }
    };
    engine.frame(|ui| view(ui, false)).unwrap();
    let stats = engine.frame(|ui| view(ui, true)).unwrap();
    assert_eq!(
        (
            stats.created,
            stats.text_writes,
            stats.measure_calls,
            stats.paint_updates
        ),
        (0, 1, 1, 1)
    );
}
#[test]
fn font_changes_propagate_size_through_ancestors() {
    let mut engine = Engine::default();
    let view = |ui: &mut Ui<'_>, size| {
        let _ = ui.column().key("panel").children(|ui| {
            ui.text("title").key("title").font_size(size);
        });
    };
    engine.frame(|ui| view(ui, 16)).unwrap();
    let stats = engine.frame(|ui| view(ui, 24)).unwrap();
    assert_eq!((stats.measure_calls, stats.layout_runs), (1, 2));
}
#[test]
fn viewport_constraint_invalidates_measurement_cache() {
    let mut engine = Engine::default();
    text(&mut engine, "long text", Color::Foreground);
    engine.set_width(20.).unwrap();
    let stats = text(&mut engine, "long text", Color::Foreground);
    assert_eq!(stats.measure_calls, 1);
    assert!(stats.layout_runs > 0);
    assert!(engine.set_width(f32::NAN).is_err());
    assert!(engine.set_width(-1.).is_err());
}
#[test]
fn explicit_width_and_intrinsic_width_are_distinct_cache_inputs() {
    let mut engine = Engine::default();
    engine.set_width(200.).unwrap();
    text(&mut engine, "hello", Color::Foreground);
    let id = engine.find(engine.root(), "label").unwrap();
    assert!(engine.node(id).unwrap().size.width < 200.);
    let stats = engine
        .frame(|ui| {
            ui.text("hello").key("label").width(200.);
        })
        .unwrap();
    assert_eq!(stats.measure_calls, 1);
    assert_eq!(engine.node(id).unwrap().size.width, 200.);
}
#[test]
fn keyed_reordering_preserves_handles_and_text_measurement() {
    let mut engine = Engine::default();
    let view = |ui: &mut Ui<'_>, order: &[u64]| {
        for &key in order {
            ui.text("item").key(key);
        }
    };
    engine.frame(|ui| view(ui, &[1, 2, 3])).unwrap();
    let id = engine.find(engine.root(), 2u64).unwrap();
    let stats = engine.frame(|ui| view(ui, &[3, 2, 1])).unwrap();
    assert_eq!(
        (
            stats.created,
            stats.removed,
            stats.measure_calls,
            stats.order_changes
        ),
        (0, 0, 0, 1)
    );
    assert_eq!(engine.find(engine.root(), 2u64), Some(id));
}
#[test]
fn removed_subtrees_reject_old_handles_and_can_remount() {
    let mut engine = Engine::default();
    let view = |ui: &mut Ui<'_>| {
        let _ = ui.column().key("panel").children(|ui| {
            ui.text("child").key("child");
        });
    };
    engine.frame(view).unwrap();
    let panel = engine.find(engine.root(), "panel").unwrap();
    let child = engine.find(panel, "child").unwrap();
    let stats = engine.frame(|_| {}).unwrap();
    assert_eq!(stats.removed, 2);
    assert!(engine.node(panel).is_none());
    assert!(engine.node(child).is_none());
    engine.frame(view).unwrap();
    let new = engine.find(engine.root(), "panel").unwrap();
    assert_ne!(new, panel);
    assert!(engine.find(panel, "child").is_none());
}
#[test]
fn theme_update_affects_tokens_without_measuring_or_copying_theme_per_node() {
    let mut engine = Engine::default();
    engine
        .frame(|ui| {
            ui.text("token").key("token");
            ui.text("fixed").key("fixed").color(Color::Literal(0));
        })
        .unwrap();
    engine.set_theme(Theme {
        foreground: 0xffffff,
        ..Default::default()
    });
    let stats = engine
        .frame(|ui| {
            ui.text("token").key("token");
            ui.text("fixed").key("fixed").color(Color::Literal(0));
        })
        .unwrap();
    assert_eq!(
        (stats.measure_calls, stats.layout_runs, stats.paint_updates),
        (0, 0, 1)
    );
}
#[test]
fn queued_click_is_consumed_synchronously_and_disabled_button_ignores_it() {
    let mut engine = Engine::default();
    engine
        .frame(|ui| {
            ui.button("Save").key("save");
        })
        .unwrap();
    let id = engine.find(engine.root(), "save").unwrap();
    engine.queue_click(id);
    let mut clicked = false;
    engine
        .frame(|ui| clicked = ui.button("Save").key("save").clicked())
        .unwrap();
    assert!(clicked);
    engine
        .frame(|ui| clicked = ui.button("Save").key("save").clicked())
        .unwrap();
    assert!(!clicked);
    engine.queue_click(id);
    engine
        .frame(|ui| clicked = ui.button("Save").key("save").disabled(true).clicked())
        .unwrap();
    assert!(!clicked);
}
#[test]
fn duplicate_identity_and_invalid_style_are_reported() {
    let mut engine = Engine::default();
    assert!(matches!(
        engine.frame(|ui| {
            ui.text("a").key("same");
            ui.text("b").key("same");
        }),
        Err(FrameError::DuplicateKey { .. })
    ));
    assert_eq!(
        engine.frame(|ui| {
            ui.text("a").width(f32::NAN);
        }),
        Err(FrameError::InvalidStyle)
    );
}
#[test]
fn repairing_an_error_frame_does_not_lose_paint_invalidation() {
    let mut engine = Engine::default();
    text(&mut engine, "hello", Color::Literal(1));
    assert!(
        engine
            .frame(|ui| {
                ui.text("hello").key("label").color(Color::Literal(2));
                ui.text("duplicate").key("label");
            })
            .is_err()
    );
    let stats = text(&mut engine, "hello", Color::Literal(2));
    assert_eq!(stats.paint_updates, 1);
}
#[test]
fn changing_widget_kind_reinitializes_layout_cache_without_unsafe_casts() {
    let mut engine = Engine::default();
    text(&mut engine, "hello", Color::Foreground);
    let id = engine.find(engine.root(), "label").unwrap();
    let stats = engine
        .frame(|ui| {
            ui.column().key("label").children(|_| {});
        })
        .unwrap();
    assert_eq!(engine.node(id).unwrap().kind, Kind::Column);
    assert!(stats.layout_runs > 0);
}

#[test]
fn explicit_height_updates_cached_simulated_measurement_and_invalid_paint_is_rejected() {
    let mut engine = Engine::default();
    engine
        .frame(|ui| {
            ui.column().key("panel").height(100.).children(|ui| {
                ui.text("Sized").key("text").height(40.);
            });
        })
        .unwrap();
    let panel = engine.find(engine.root(), "panel").unwrap();
    let text = engine.find(panel, "text").unwrap();
    assert_eq!(engine.node(panel).unwrap().size.height, 100.);
    assert_eq!(engine.node(text).unwrap().size.height, 40.);
    engine
        .frame(|ui| {
            ui.column().key("panel").height(120.).children(|ui| {
                ui.text("Sized").key("text").height(60.);
            });
        })
        .unwrap();
    assert_eq!(engine.node(panel).unwrap().size.height, 120.);
    assert_eq!(engine.node(text).unwrap().size.height, 60.);
    assert!(
        engine
            .frame_declarations(|ui| {
                ui.text("Invalid").opacity(f32::NAN);
            })
            .is_err()
    );
    assert!(
        engine
            .frame_declarations(|ui| {
                ui.column().height(-1.).children(|_| {});
            })
            .is_err()
    );
}

#[test]
fn frame_errors_report_the_first_failure() {
    let mut engine = Engine::default();
    // Invalid style is detected before the duplicate key later in the same frame.
    assert_eq!(
        engine.frame(|ui| {
            ui.text("a").key("same").width(f32::NAN);
            ui.text("b").key("same");
        }),
        Err(FrameError::InvalidStyle)
    );
    // A nested control fires first; a later invalid style must not overwrite it.
    assert_eq!(
        engine.frame(|ui| {
            ui.button_base("outer").children(|ui| {
                ui.switch("inner", &mut false);
            });
            ui.text("late").width(f32::NAN);
        }),
        Err(FrameError::NestedControl)
    );
}

#[test]
fn unconsumed_path_events_are_reported_not_silently_dropped() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|ui| {
            ui.button("Save").key("save");
        })
        .unwrap();
    engine.queue_click_path(vec![PathKey::Named("save".into())]);
    let mut clicked = false;
    engine
        .frame_declarations(|ui| clicked = ui.button("Save").key("save").clicked())
        .unwrap();
    assert!(clicked);
    let unmatched = engine.take_unmatched_paths();
    assert_eq!((unmatched.clicks.len(), unmatched.values.len()), (0, 0));
    // A stale path matches no declaration and stays observable after the pass.
    engine.queue_click_path(vec![PathKey::Named("gone".into())]);
    engine.queue_value_path(vec![PathKey::Named("missing".into())], 1.);
    engine
        .frame_declarations(|ui| {
            ui.button("Save").key("save");
        })
        .unwrap();
    let unmatched = engine.take_unmatched_paths();
    assert_eq!((unmatched.clicks.len(), unmatched.values.len()), (1, 1));
    // Draining resets the report.
    let unmatched = engine.take_unmatched_paths();
    assert!(unmatched.clicks.is_empty() && unmatched.values.is_empty());
}

#[test]
fn removed_entities_accumulate_across_declaration_passes() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|ui| {
            ui.column().key("panel").children(|ui| {
                ui.text("a").key("item");
            });
        })
        .unwrap();
    // A following pass without removals must not erase the earlier removal:
    // a host syncs once per settled batch, after every pass.
    engine
        .frame_declarations(|ui| {
            ui.column().key("panel").children(|_| {});
        })
        .unwrap();
    engine
        .frame_declarations(|ui| {
            ui.column().key("panel").children(|_| {});
        })
        .unwrap();
    assert_eq!(engine.take_removed().len(), 1);
    assert!(engine.take_removed().is_empty());
    // The simulated-measurement path clears the accumulator per frame instead.
    let mut sim = Engine::default();
    sim.frame(|ui| {
        ui.text("a").key("x");
        ui.text("b").key("y");
    })
    .unwrap();
    sim.frame(|ui| {
        ui.text("a").key("x");
    })
    .unwrap();
    assert_eq!(sim.take_removed().len(), 1);
    sim.frame(|ui| {
        ui.text("a").key("x");
    })
    .unwrap();
    assert!(sim.take_removed().is_empty());
}

#[test]
fn changing_input_kind_releases_the_old_native_binding() {
    let mut engine = Engine::default();
    let buffer = plico_core::TextBuffer::new();
    engine
        .frame_declarations(|ui| {
            ui.text_input("Field", &buffer).key("same");
        })
        .unwrap();
    assert_eq!(engine.input_bindings().count(), 1);
    engine
        .frame_declarations(|ui| {
            ui.text("Label").key("same");
        })
        .unwrap();
    assert_eq!(engine.input_bindings().count(), 0);
}
