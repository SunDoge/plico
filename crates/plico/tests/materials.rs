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
