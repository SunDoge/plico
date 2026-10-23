use plico_core::{Engine, Kind, prelude::*};
#[component]
fn chart(cx: &mut UiContext, source: &str, visible: bool) {
    if visible {
        svg_source(cx, source.to_owned())
            .size(200., 44.)
            .width_percent(100.);
    }
}
#[test]
fn generated_svg_keeps_identity_and_updates_without_leaking_static_assets() {
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| chart(cx, "<svg/>", true))
        .unwrap();
    let id = engine.node(engine.root()).unwrap().children[0];
    engine
        .frame_declarations(|cx| chart(cx, "<svg><path/></svg>", true))
        .unwrap();
    let node = engine.node(id).unwrap();
    assert_eq!(node.kind, Kind::Svg);
    assert_eq!(node.text, "<svg><path/></svg>");
    assert_eq!(node.style.layout.min_size, [None, None]);
    engine
        .frame_declarations(|cx| chart(cx, "<svg><path/></svg>", true))
        .unwrap();
    assert!(engine.changes().is_empty());
}
