//! The image widget's declaration channel: hex encoding is lossless and the
//! node carries the asset bytes for backend decode.
use plico_core::{Engine, ImageAsset, prelude::*};

fn test_png() -> ImageAsset {
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 218, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 247, 3, 65, 67, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    ImageAsset::new(PNG)
}

#[test]
fn image_node_carries_lossless_asset_bytes() {
    let asset = test_png();
    let encoded = asset.encoded();
    // Round trip.
    assert_eq!(ImageAsset::decode(&encoded).unwrap(), asset.data());
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            image(cx, &asset).size(32., 32.);
        })
        .unwrap();
    fn find_image(engine: &Engine) -> plico_core::Entity {
        fn walk(engine: &Engine, id: plico_core::Entity) -> Option<plico_core::Entity> {
            if engine.node(id).unwrap().kind == plico_core::Kind::Image {
                return Some(id);
            }
            engine
                .node(id)
                .unwrap()
                .children
                .iter()
                .find_map(|&c| walk(engine, c))
        }
        walk(engine, engine.root()).unwrap()
    }
    let node = engine.node(find_image(&engine)).unwrap();
    assert_eq!(node.text, encoded);
    assert_eq!(node.style.width, Some(32.));
    assert_eq!(node.style.height, Some(32.));
    // A second identical declaration reuses identity (hex text unchanged).
    let first = find_image(&engine);
    engine
        .frame_declarations(|cx| {
            image(cx, &asset).size(32., 32.);
        })
        .unwrap();
    let second = find_image(&engine);
    assert_eq!(first, second);
}
