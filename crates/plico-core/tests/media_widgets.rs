use plico_core::{Engine, Entity, ImageAsset, Kind, TextSpan, prelude::*};

fn nodes(engine: &Engine, root: Entity, out: &mut Vec<Entity>) {
    out.push(root);
    for id in engine.node(root).unwrap().children {
        nodes(engine, *id, out);
    }
}
#[component]
fn screen(
    cx: &mut UiContext,
    extra: bool,
    asset: Option<&ImageAsset>,
    reading: Option<&str>,
    spans: &[TextSpan],
) {
    if extra {
        text(cx, "optional sibling");
    }
    avatar(cx).image(asset).fallback("花").size(40.);
    ruby_text(cx, "夜空")
        .annotation(reading)
        .font_size(28)
        .spans(spans);
}
#[test]
fn media_builders_keep_identity_and_swap_content_without_stale_nodes() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| screen(cx, false, None, Some("よぞら"), &[]))
        .unwrap();
    let first = engine.node(engine.root()).unwrap().children.to_vec();
    let avatar_id = first[0];
    let ruby_id = first[1];
    let asset = ImageAsset::from_bytes(&[1, 2, 3]); // Core transports bytes; decoding belongs to the backend.
    let spans = [TextSpan {
        range: 0..3,
        weight: 600,
        italic: false,
        color: Some(Color::Accent),
    }];
    engine
        .frame_declarations(|cx| screen(cx, true, Some(&asset), None, &spans))
        .unwrap();
    let second = engine.node(engine.root()).unwrap().children;
    assert_eq!((second[1], second[2]), (avatar_id, ruby_id));
    let avatar = engine.node(avatar_id).unwrap();
    assert!(avatar.style.clip);
    assert_eq!(avatar.style.radius, 20.);
    assert_eq!(engine.node(avatar.children[0]).unwrap().kind, Kind::Image);
    let mut all = Vec::new();
    nodes(&engine, ruby_id, &mut all);
    assert!(
        !all.iter()
            .any(|id| engine.node(*id).unwrap().text == "よぞら")
    );
    let body = all
        .into_iter()
        .find(|id| engine.node(*id).unwrap().text == "夜空")
        .unwrap();
    assert_eq!(engine.node(body).unwrap().spans, &spans);
    assert_eq!(
        engine
            .node(engine.node(ruby_id).unwrap().children[0])
            .unwrap()
            .style
            .height,
        Some(18.)
    );
}
