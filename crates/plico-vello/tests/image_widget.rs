use plico_core::{Engine, ImageAsset, prelude::*};
use plico_renderer::FrameSource as _;

fn test_png() -> &'static ImageAsset {
    use std::sync::OnceLock;
    static ASSET: OnceLock<ImageAsset> = OnceLock::new();
    ASSET.get_or_init(|| {
        // 1x1 red PNG: IHDR(1,1,8,RGB) + one filtered scanline + IEND.
        const PNG: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 218, 99, 248, 207,
            192, 0, 0, 3, 1, 1, 0, 247, 3, 65, 67, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        ImageAsset::new(PNG)
    })
}

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

#[test]
fn image_node_carries_lossless_asset_bytes() {
    let asset = *test_png();
    let encoded = asset.encoded();
    assert_eq!(ImageAsset::decode(&encoded).unwrap(), asset.data());
    let mut engine = Engine::default();
    engine
        .frame_declarations(|cx| {
            image(cx, &asset).size(32., 32.);
        })
        .unwrap();
    let node = engine.node(find_image(&engine)).unwrap();
    assert_eq!(node.text, encoded);
    assert_eq!(node.style.width, Some(32.));
    assert_eq!(node.style.height, Some(32.));
    let first = find_image(&engine);
    engine
        .frame_declarations(|cx| {
            image(cx, &asset).size(32., 32.);
        })
        .unwrap();
    assert_eq!(first, find_image(&engine));
}

#[test]
fn embedded_image_reaches_offscreen_pixels() {
    use plico_renderer::TextContext;
    use plico_vello::{Source, View};
    struct App;
    impl View for App {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).padding(30.).children(|cx| {
                image(cx, test_png()).size(40., 40.);
            });
        }
    }
    let mut source = Source::new(App);
    let mut tcx = TextContext::new();
    let nodes = source.build_frame(&mut tcx, 100, 100);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    let image_nodes: Vec<_> = nodes.iter().filter(|n| n.paint.image.is_some()).collect();
    assert_eq!(image_nodes.len(), 1);
    assert_eq!(
        image_nodes[0].paint.image.as_ref().unwrap().size(),
        [1., 1.]
    );
    let mut scene = plico_renderer::Scene::new();
    source.paint_scene(&mut scene, &nodes, &mut tcx, (100, 100), 1.);
    let img =
        plico_shell::renderer::render_to_image(&scene, 100, 100, source.base_color()).unwrap();
    let center = img.get_pixel(50, 50).0;
    assert!(center[0] > 200, "center pixel should be red: {center:?}");
    assert!(
        center[1] < 60 && center[2] < 60,
        "center pixel should be red: {center:?}"
    );
}
