use plico::{Engine, FrameError, UiContext};

fn list(ui: &mut UiContext<'_>, ids: &[u64]) {
    for id in ids {
        ui.row().key(*id).children(|ui| {
            ui.text("value").key("value");
        });
    }
}

#[test]
fn ordered_hint_preserves_key_identity_through_edits_and_removals() {
    for simulated_layout in [false, true] {
        let mut engine = Engine::default();
        let mut declare = |ids: &[u64]| {
            if simulated_layout {
                engine.frame(|ui| list(ui, ids)).unwrap()
            } else {
                engine.frame_declarations(|ui| list(ui, ids)).unwrap()
            }
        };
        declare(&[1, 2, 3]);
        let original: Vec<_> = [1u64, 2, 3]
            .map(|k| engine.find(engine.root(), k).unwrap())
            .into();
        for ids in [&[1u64, 2, 3][..], &[3, 1, 2], &[4, 3, 1, 2], &[2, 5, 3]] {
            let stats = if simulated_layout {
                engine.frame(|ui| list(ui, ids)).unwrap()
            } else {
                engine.frame_declarations(|ui| list(ui, ids)).unwrap()
            };
            for (key, old) in [1u64, 2, 3].into_iter().zip(&original) {
                if ids.contains(&key) {
                    assert_eq!(engine.find(engine.root(), key), Some(*old));
                } else {
                    assert!(engine.node(*old).is_none());
                }
            }
            let order: Vec<_> = ids
                .iter()
                .map(|key| engine.find(engine.root(), *key).unwrap())
                .collect();
            assert_eq!(engine.node(engine.root()).unwrap().children, order);
            assert_eq!(stats.visited, ids.len() * 2);
        }
        let removed = engine.frame_declarations(|_| {}).unwrap();
        assert_eq!(removed.removed, 6);
        assert!(engine.node(engine.root()).unwrap().children.is_empty());
    }
}

#[test]
fn ordered_hint_checks_scope_and_duplicate_keys_after_reuse() {
    let mut engine = Engine::default();
    let scoped = |ui: &mut UiContext<'_>, scopes: &[u64]| {
        for scope in scopes {
            ui.keyed(*scope, |ui| {
                ui.text("scoped").key("same");
            });
        }
    };
    engine.frame_declarations(|ui| scoped(ui, &[1, 2])).unwrap();
    let original = engine.node(engine.root()).unwrap().children.to_vec();
    engine.frame_declarations(|ui| scoped(ui, &[2, 1])).unwrap();
    assert_eq!(
        engine.node(engine.root()).unwrap().children,
        [original[1], original[0]]
    );
    let error = engine
        .frame_declarations(|ui| scoped(ui, &[2, 2]))
        .unwrap_err();
    assert!(matches!(error, FrameError::DuplicateKey { .. }));
    // A rejected pass must not prevent a subsequent valid declaration.
    engine.frame_declarations(|ui| scoped(ui, &[1, 2])).unwrap();
    assert_eq!(engine.node(engine.root()).unwrap().children, original);
}

#[test]
fn unchanged_rejected_style_is_never_accepted_by_the_validation_cache() {
    use plico::{Key, Kind, Style};
    let mut engine = Engine::default();
    for invalid in [
        Style {
            opacity: -1.,
            ..Default::default()
        },
        Style {
            width: Some(f32::NAN),
            ..Default::default()
        },
        Style {
            font_size: 0,
            ..Default::default()
        },
    ] {
        for _ in 0..2 {
            assert_eq!(
                engine.frame_declarations(|ui| {
                    ui.element(Key::Static("value"), Kind::Text, invalid, "text", |_| {});
                }),
                Err(FrameError::InvalidStyle)
            );
        }
        engine
            .frame_declarations(|ui| {
                ui.text("text").key("value");
            })
            .unwrap();
    }
}
