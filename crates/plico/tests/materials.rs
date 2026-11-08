use plico::{Engine, Entity, Kind, prelude::*};

fn descendants(engine: &Engine, entity: Entity) -> Vec<Entity> {
    let node = engine.node(entity).unwrap();
    std::iter::once(entity)
        .chain(
            node.children
                .iter()
                .flat_map(|&child| descendants(engine, child)),
        )
        .collect()
}
#[component]
fn pane(cx: &mut UiContext, material: LiquidGlass, elevation: Elevation) {
    column(cx)
        .width(240.)
        .height(160.)
        .radius(18.)
        .corner_smoothing(1.)
        .material(material)
        .elevation(elevation)
        .children(|cx| {
            text(cx, "Sharp foreground");
            button(cx).text("Continue");
        });
}
#[test]
fn material_declares_positioned_backdrop_before_content_and_updates_without_layout_work() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| pane(cx, LiquidGlass::default(), Elevation::PANEL))
        .unwrap();
    let nodes = descendants(&engine, engine.root());
    let shader = *nodes
        .iter()
        .find(|&&id| engine.node(id).unwrap().kind == Kind::Shader)
        .unwrap();
    let params = engine.node(shader).unwrap().shader.unwrap();
    assert!(params.reads_backdrop);
    assert!(params.prefilter_backdrop.is_some());
    assert!(!params.animated);
    assert_eq!(params.values[0], 18.);
    assert_eq!(params.values[4], 1.6);
    assert_eq!(params.values[12], 1.);
    let parent = nodes
        .iter()
        .find_map(|&id| {
            let node = engine.node(id).unwrap();
            node.children.contains(&shader).then_some(node)
        })
        .unwrap();
    assert_eq!(parent.style.layout.position, Position::Relative);
    assert_eq!(parent.children[0], shader);
    assert_eq!(parent.style.shadow, Elevation::PANEL.contact);
    assert_eq!(parent.style.ambient_shadow, Elevation::PANEL.ambient);
    let stats = engine
        .frame_declarations(|cx| {
            pane(
                cx,
                LiquidGlass {
                    frost: 0.2,
                    ..Default::default()
                },
                Elevation::CONTROL,
            )
        })
        .unwrap();
    assert_eq!((stats.measure_calls, stats.layout_runs), (0, 0));
    assert_eq!(engine.node(shader).unwrap().shader.unwrap().values[1], 0.2);
}
#[test]
fn disabled_material_is_opaque_and_reduced_motion_does_not_request_animation() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            pane(
                cx,
                LiquidGlass {
                    enabled: false,
                    ..Default::default()
                },
                Elevation::NONE,
            )
        })
        .unwrap();
    assert!(
        descendants(&engine, engine.root())
            .iter()
            .all(|&id| engine.node(id).unwrap().kind != Kind::Shader)
    );
    assert!(
        descendants(&engine, engine.root()).iter().any(|&id| engine
            .node(id)
            .unwrap()
            .style
            .background
            == Some(Color::Literal(0xf9fcff)))
    );
    engine
        .frame_declarations(|cx| {
            pane(
                cx,
                LiquidGlass {
                    animated: true,
                    reduce_motion: true,
                    ..Default::default()
                },
                Elevation::PANEL,
            )
        })
        .unwrap();
    let shaders: Vec<_> = descendants(&engine, engine.root())
        .iter()
        .filter_map(|&id| engine.node(id).unwrap().shader.cloned())
        .collect();
    assert_eq!(shaders.len(), 1);
    assert!(!shaders[0].animated);
    assert_eq!(shaders[0].values[3], 0.);
}
#[test]
fn invalid_material_parameters_are_bounded_and_absolute_panes_keep_their_position() {
    let material = LiquidGlass {
        frost: f32::NAN,
        tint_opacity: 2.,
        refraction: -1.,
        bevel: f32::INFINITY,
        saturation: 4.,
        rim: -1.,
        ..Default::default()
    }
    .validated();
    assert_eq!(material.frost, LiquidGlass::default().frost);
    assert_eq!(material.tint_opacity, 1.);
    assert_eq!(material.refraction, 0.);
    assert_eq!(material.bevel, LiquidGlass::default().bevel);
    assert_eq!(material.saturation, 3.);
    assert_eq!(material.rim, 0.);
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            column(cx)
                .absolute(20., 30.)
                .width(240.)
                .height(160.)
                .material(material)
                .end();
        })
        .unwrap();
    assert!(descendants(&engine, engine.root()).iter().any(|&id| {
        let node = engine.node(id).unwrap();
        node.kind == Kind::Column && node.style.layout.position == Position::Absolute
    }));
}

#[test]
fn progressive_blur_entry_points_pack_typed_parameters_without_renderer_dependencies() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            for side in [
                ScrollEdgeSide::Top,
                ScrollEdgeSide::Bottom,
                ScrollEdgeSide::Left,
                ScrollEdgeSide::Right,
            ] {
                scroll_edge(
                    cx,
                    side,
                    BackdropBlur {
                        radius: 12.,
                        tint: Color::Accent,
                        tint_opacity: 0.3,
                    },
                )
                .width(200.)
                .height(80.);
            }
            backdrop_blur(cx, BackdropBlur::default())
                .width(200.)
                .height(80.);
            column(cx)
                .elevation(Elevation::PANEL)
                .no_shadow()
                .blur(3.)
                .end();
        })
        .unwrap();
    let nodes = descendants(&engine, engine.root());
    let effects: Vec<_> = nodes
        .iter()
        .filter_map(|&id| engine.node(id).unwrap().shader)
        .collect();
    assert_eq!(effects.len(), 5);
    assert_eq!(
        effects
            .iter()
            .map(|effect| effect.values[1])
            .collect::<Vec<_>>(),
        [1., 2., 3., 4., 0.]
    );
    assert!(effects.iter().all(|effect| effect.reads_backdrop));
    let clear = nodes
        .iter()
        .find_map(|&id| {
            let node = engine.node(id).unwrap();
            (node.style.foreground_blur == 3.).then_some(node)
        })
        .unwrap();
    assert!(clear.style.shadow.is_none() && clear.style.ambient_shadow.is_none());
    let options = BackdropBlur {
        radius: f32::NAN,
        tint_opacity: f32::INFINITY,
        ..Default::default()
    }
    .validated();
    assert_eq!((options.radius, options.tint_opacity), (8., 0.));
}

#[test]
fn glass_buttons_keep_semantics_and_paint_material_before_sharp_content() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            button(cx)
                .text("Save")
                .primary()
                .radius(20.)
                .material(LiquidGlass::default());
            button_base(cx, "Custom glass")
                .material(LiquidGlass::default())
                .children(|cx| {
                    text(cx, "Custom");
                });
        })
        .unwrap();
    let nodes = descendants(&engine, engine.root());
    for label in ["Save", "Custom glass"] {
        let node = nodes
            .iter()
            .map(|&id| engine.node(id).unwrap())
            .find(|node| node.kind == Kind::ButtonBase && node.text == label)
            .unwrap();
        assert_eq!(node.style.background, None);
        assert_eq!(node.style.layout.position, Position::Relative);
        let material = engine.node(node.children[0]).unwrap();
        assert_eq!(material.kind, Kind::Shader);
        assert!(node.children.len() >= 2);
        if label == "Save" {
            assert_eq!(node.style.foreground, Color::OnAccent);
            assert_eq!(material.shader.as_ref().unwrap().values[2], 1.0);
        }
    }
    engine
        .frame_declarations(|cx| {
            button(cx).text("Save").primary().material(LiquidGlass {
                enabled: false,
                ..Default::default()
            });
        })
        .unwrap();
    assert!(
        descendants(&engine, engine.root())
            .iter()
            .all(|&id| engine.node(id).unwrap().kind != Kind::Shader)
    );
}

#[test]
fn glass_dialog_background_stays_outside_its_content_scroller_and_has_opaque_fallback() {
    let mut engine = Engine::default();
    let mut state = ModalState::default();
    state.open();
    for enabled in [true, false] {
        engine
            .frame_declarations(|cx| {
                modal(cx, &mut state)
                    .title("Glass dialog")
                    .material(LiquidGlass {
                        enabled,
                        ..Default::default()
                    })
                    .children(|cx| {
                        text(cx, "Sharp dialog content");
                    });
            })
            .unwrap();
        let nodes = descendants(&engine, engine.root());
        let dialog = nodes
            .iter()
            .map(|&id| engine.node(id).unwrap())
            .find(|node| node.kind == Kind::Dialog)
            .unwrap();
        assert!(!dialog.style.scroll_y);
        assert_eq!(dialog.style.radius, 24.);
        assert_eq!(dialog.style.shadow, Elevation::OVERLAY.contact);
        assert_eq!(dialog.style.ambient_shadow, Elevation::OVERLAY.ambient);
        let background = engine.node(dialog.children[0]).unwrap();
        let content = engine.node(dialog.children[1]).unwrap();
        assert!(content.style.scroll_y);
        if enabled {
            assert_eq!(background.kind, Kind::Shader);
            assert!(background.shader.as_ref().unwrap().reads_backdrop);
        } else {
            assert_ne!(background.kind, Kind::Shader);
            assert!(
                nodes
                    .iter()
                    .all(|&id| engine.node(id).unwrap().kind != Kind::Shader)
            );
        }
    }
}

#[test]
fn mygo_glass_presets_keep_theme_and_clarity_in_paint_parameters() {
    assert_eq!(LiquidGlass::default().style, GlassStyle::Custom);
    assert_eq!(LiquidGlass::regular().style, GlassStyle::Regular);
    assert_eq!(LiquidGlass::clear().style, GlassStyle::Clear);
    assert_eq!(LiquidGlass::regular().fallback, Color::Panel);
    assert_eq!(
        LiquidGlass {
            clarity: f32::NAN,
            ..LiquidGlass::regular()
        }
        .validated()
        .clarity,
        0.
    );
    assert_eq!(
        LiquidGlass {
            clarity: 2.,
            ..LiquidGlass::regular()
        }
        .validated()
        .clarity,
        1.
    );
    let mut engine = Engine::default();
    let mut material = LiquidGlass::regular();
    engine
        .frame_declarations(|cx| pane(cx, material, Elevation::NONE))
        .unwrap();
    let shader = descendants(&engine, engine.root())
        .into_iter()
        .find(|id| engine.node(*id).unwrap().kind == Kind::Shader)
        .unwrap();
    let values = &engine.node(shader).unwrap().shader.as_ref().unwrap().values;
    assert_eq!(&values[13..16], &[1., 0., 0.]);
    material.clarity = 0.75;
    let stats = engine
        .frame_declarations(|cx| pane(cx, material, Elevation::NONE))
        .unwrap();
    assert_eq!((stats.measure_calls, stats.layout_runs), (0, 0));
    assert_eq!(
        engine.node(shader).unwrap().shader.as_ref().unwrap().values[14],
        0.75
    );
}

#[test]
fn disabled_glass_controls_use_a_readable_fallback_without_backdrop_work() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            button(cx)
                .text("Unavailable")
                .primary()
                .disabled(true)
                .material(LiquidGlass::regular());
        })
        .unwrap();
    let nodes = descendants(&engine, engine.root());
    assert!(
        nodes
            .iter()
            .all(|id| engine.node(*id).unwrap().kind != Kind::Shader)
    );
    assert!(
        nodes
            .iter()
            .any(|id| engine.node(*id).unwrap().style.background == Some(Color::Accent))
    );
    let button = nodes
        .iter()
        .find_map(|id| {
            let node = engine.node(*id).unwrap();
            (node.text == "Unavailable").then_some(node)
        })
        .unwrap();
    assert!(button.style.disabled);
}
