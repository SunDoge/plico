//! The incremental re-encoder must reproduce a full rebuild exactly.
use plico_renderer::layout::PlacedNode;
use plico_renderer::scene::{SceneRanges, build_scene_incremental, build_scene_tracked};
use plico_renderer::{Scene, text::TextContext};
use vello_common::peniko::Color;

fn node(id: taffy::NodeId, depth: usize, parent: Option<taffy::NodeId>, color: u32) -> PlacedNode {
    let background = Some(Color::from_rgba8(
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
        255,
    ));
    PlacedNode {
        node_id: id,
        parent_node_id: parent,
        depth,
        rect: [0., depth as f32 * 40., 200., depth as f32 * 40. + 40.],
        content_origin: [0., depth as f32 * 40.],
        content_size: [200., 40.],
        clip: None,
        clip_radius: 0.,
        clip_radii: Default::default(),
        clip_depth: None,
        own_clip: None,
        own_clip_radius: 0.,
        own_clip_radii: Default::default(),
        border_widths: [0.; 4],
        scroll: Default::default(),
        paint: plico_renderer::style::Paint {
            background,
            ..Default::default()
        },
    }
}
fn nodes(color: u32) -> Vec<PlacedNode> {
    vec![
        node(taffy::NodeId::new(1), 0, None, color),
        node(taffy::NodeId::new(2), 1, Some(taffy::NodeId::new(1)), color),
        node(taffy::NodeId::new(3), 1, Some(taffy::NodeId::new(1)), color),
    ]
}
fn commands_of(scene: &Scene) -> Vec<String> {
    scene.commands.iter().map(|c| format!("{c:?}")).collect()
}
#[test]
fn full_dirty_incremental_matches_full_rebuild() {
    let mut tcx = TextContext::new();
    let mut ranges = SceneRanges::default();
    let mut full = Scene::new();
    let a = nodes(0x3366aa);
    build_scene_tracked(
        &mut full,
        &[a.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges),
    );
    let b = nodes(0xaa6633);
    let mut incremental = full.clone();
    let dirty = vec![
        taffy::NodeId::new(1),
        taffy::NodeId::new(2),
        taffy::NodeId::new(3),
    ];
    assert!(build_scene_incremental(
        &mut incremental,
        &[b.as_slice()],
        &full,
        &ranges,
        &dirty,
        &mut tcx,
        1.
    ));
    let mut rebuilt = Scene::new();
    let mut ranges_b = SceneRanges::default();
    build_scene_tracked(
        &mut rebuilt,
        &[b.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges_b),
    );
    assert_eq!(commands_of(&incremental), commands_of(&rebuilt));
}
#[test]
fn partial_dirty_incremental_matches_full_rebuild() {
    let mut tcx = TextContext::new();
    let mut ranges = SceneRanges::default();
    let mut full = Scene::new();
    let a = nodes(0x3366aa);
    build_scene_tracked(
        &mut full,
        &[a.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges),
    );
    let mut b = nodes(0x3366aa);
    b[2] = node(
        taffy::NodeId::new(3),
        1,
        Some(taffy::NodeId::new(1)),
        0xaa6633,
    );
    let mut incremental = full.clone();
    let dirty = vec![taffy::NodeId::new(3)];
    assert!(build_scene_incremental(
        &mut incremental,
        &[b.as_slice()],
        &full,
        &ranges,
        &dirty,
        &mut tcx,
        1.
    ));
    let mut rebuilt = Scene::new();
    let mut ranges_b = SceneRanges::default();
    build_scene_tracked(
        &mut rebuilt,
        &[b.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges_b),
    );
    assert_eq!(commands_of(&incremental), commands_of(&rebuilt));
}

#[test]
fn scrollbar_tail_survives_incremental_reencoding() {
    let mut tcx = TextContext::new();
    let mut ranges = SceneRanges::default();
    let mut full = Scene::new();
    let mut a = nodes(0x3366aa);
    // A scrollable container paints scrollbar commands at its Exit.
    a[0].scroll = plico_renderer::layout::ScrollMetrics {
        port: [0., 0., 200., 40.],
        scrollable: [false, true],
        range: [0., 400.],
        offset: [0., 0.],
        opacity: 1.,
        interaction: 0,
    };
    build_scene_tracked(
        &mut full,
        &[a.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges),
    );
    // Change a child's color only; the scroll container is not dirty.
    let mut b = nodes(0x3366aa);
    b[0].scroll = a[0].scroll;
    b[2] = node(
        taffy::NodeId::new(3),
        1,
        Some(taffy::NodeId::new(1)),
        0xaa6633,
    );
    let mut incremental = full.clone();
    let dirty = vec![taffy::NodeId::new(3)];
    assert!(build_scene_incremental(
        &mut incremental,
        &[b.as_slice()],
        &full,
        &ranges,
        &dirty,
        &mut tcx,
        1.
    ));
    let mut rebuilt = Scene::new();
    let mut ranges_b = SceneRanges::default();
    build_scene_tracked(
        &mut rebuilt,
        &[b.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges_b),
    );
    assert_eq!(commands_of(&incremental), commands_of(&rebuilt));
    // Dirtying the scroll container re-emits its tail; when the whole tree
    // changes color every node is dirty, matching a full rebuild.
    let mut c = nodes(0xaa3366);
    c[0].scroll = a[0].scroll;
    let mut incremental2 = full.clone();
    let dirty2 = vec![
        taffy::NodeId::new(1),
        taffy::NodeId::new(2),
        taffy::NodeId::new(3),
    ];
    assert!(build_scene_incremental(
        &mut incremental2,
        &[c.as_slice()],
        &full,
        &ranges,
        &dirty2,
        &mut tcx,
        1.
    ));
    let mut rebuilt2 = Scene::new();
    let mut ranges_c = SceneRanges::default();
    build_scene_tracked(
        &mut rebuilt2,
        &[c.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges_c),
    );
    assert_eq!(commands_of(&incremental2), commands_of(&rebuilt2));
}

#[test]
fn repeated_encoding_is_deterministic() {
    let mut tcx = TextContext::new();
    let mut ranges = SceneRanges::default();
    let mut full = Scene::new();
    let a = nodes(0x3366aa);
    build_scene_tracked(
        &mut full,
        &[a.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges),
    );
    // A second full encode of identical nodes must be byte-identical.
    let mut again = Scene::new();
    let mut ranges2 = SceneRanges::default();
    build_scene_tracked(
        &mut again,
        &[a.as_slice()],
        &mut tcx,
        400,
        300,
        Color::WHITE,
        1.,
        Some(&mut ranges2),
    );
    assert_eq!(commands_of(&full), commands_of(&again));
}
