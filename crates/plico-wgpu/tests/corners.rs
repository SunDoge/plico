//! Per-corner radii coverage: mygo-style individual corner rounding on quads
//! and shared scenes, which previously fell back to CPU raster with a
//! "nonuniform corner radii" rejection.
use kurbo::{Affine, RoundedRect};
use peniko::{Color, Fill};
use plico_renderer::{PaintScene, Scene};
use plico_wgpu::{Quad, Renderer};

fn pixel(image: &[u8], size: [u32; 2], x: u32, y: u32) -> u8 {
    image[((y * size[0] + x) * 4 + 3) as usize]
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn quad_rounds_only_the_corners_with_a_radius() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [32, 32];
    // Top-left rounded, the other three corners stay square.
    let mut sheet = Quad::new([0., 0., 32., 32.], [1., 0., 0., 1.]);
    sheet.radii = [10., 0., 0., 0.];
    let image = renderer.render(&[sheet], size, [0.; 4], 0.).unwrap();
    assert_eq!(pixel(&image, size, 0, 0), 0, "top-left corner is cut");
    assert_eq!(pixel(&image, size, 31, 0), 255, "top-right stays square");
    assert_eq!(
        pixel(&image, size, 31, 31),
        255,
        "bottom-right stays square"
    );
    assert_eq!(pixel(&image, size, 0, 31), 255, "bottom-left stays square");
    assert_eq!(pixel(&image, size, 3, 1), 0, "inside the top-left arc");
    assert_eq!(pixel(&image, size, 16, 16), 255, "center covered");

    // Borders follow each corner independently: the ring keeps the arc of the
    // rounded corners (top-left, bottom-right) and sharp corners elsewhere.
    let mut bordered = Quad::new([2., 2., 28., 28.], [0., 0., 0., 0.]);
    bordered.radii = [8., 0., 8., 0.];
    bordered.border_width = 2.;
    bordered.border_color = [0., 0., 1., 1.];
    let image = renderer.render(&[bordered], size, [0.; 4], 0.).unwrap();
    assert_eq!(
        pixel(&image, size, 2, 2),
        0,
        "outside the rounded top-left arc"
    );
    assert_eq!(
        pixel(&image, size, 29, 2),
        255,
        "square top-right corner covered"
    );
    assert_eq!(
        pixel(&image, size, 29, 29),
        0,
        "outside the rounded bottom-right arc"
    );
    assert_eq!(
        pixel(&image, size, 2, 29),
        255,
        "square bottom-left corner covered"
    );
    assert_eq!(pixel(&image, size, 16, 2), 255, "straight top band");
    assert_eq!(pixel(&image, size, 16, 16), 0, "transparent interior");
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn shared_scenes_accept_nonuniform_corner_radii_without_a_cpu_fallback() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [64, 64];
    // A sheet rounded only on top, the pattern real layouts use for popovers.
    let sheet = RoundedRect::new(
        8.,
        8.,
        56.,
        48.,
        kurbo::RoundedRectRadii::new(12., 12., 0., 0.),
    );
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(40, 90, 220, 255),
        None,
        &sheet,
    );
    let pixels = renderer
        .render_scene(&scene, size, [0.; 4], false)
        .expect("nonuniform corner radii must not be rejected");
    assert_eq!(pixel(&pixels, size, 8, 8), 0, "top-left corner is rounded");
    assert_eq!(pixel(&pixels, size, 8, 47), 255, "bottom-left stays square");
    assert_eq!(pixel(&pixels, size, 32, 24), 255, "interior covered");

    // A mitered stroke keeps the arcs of the rounded corners (top-left,
    // bottom-left) and leaves the other corners sharp. The outer rect grows
    // by half the stroke width on every side.
    let mut scene = Scene::new();
    scene.stroke(
        &kurbo::Stroke::new(2.).with_join(kurbo::Join::Miter),
        Affine::IDENTITY,
        Color::from_rgba8(0, 128, 0, 255),
        None,
        &RoundedRect::new(
            12.,
            12.,
            52.,
            52.,
            kurbo::RoundedRectRadii::new(10., 0., 0., 10.),
        ),
    );
    let pixels = renderer
        .render_scene(&scene, size, [0.; 4], false)
        .expect("nonuniform stroked corners must not be rejected");
    assert_eq!(
        pixel(&pixels, size, 10, 10),
        0,
        "outside the rounded top-left arc"
    );
    assert_eq!(
        pixel(&pixels, size, 12, 52),
        0,
        "outside the rounded bottom-left arc"
    );
    assert_eq!(
        pixel(&pixels, size, 52, 12),
        255,
        "square top-right of the ring"
    );
    assert_eq!(
        pixel(&pixels, size, 52, 52),
        255,
        "square bottom-right of the ring"
    );
    assert_eq!(pixel(&pixels, size, 32, 11), 255, "straight top band");
    assert_eq!(
        pixel(&pixels, size, 32, 16),
        0,
        "transparent inside the ring"
    );
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn overlapping_adjacent_corners_shrink_proportionally_instead_of_inverting() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [32, 32];
    // Two corners of 12 on a 16-pixel edge must shrink to 8 each (16/2).
    // The whole top edge is then arc: the junction stays nearly covered
    // (no hole or inversion) and the bottom edge stays straight and square.
    let mut pinch = Quad::new([0., 0., 16., 32.], [1., 0., 0., 1.]);
    pinch.radii = [12., 12., 0., 0.];
    let image = renderer.render(&[pinch], size, [0.; 4], 0.).unwrap();
    assert_eq!(pixel(&image, size, 0, 0), 0, "corner still rounded");
    assert!(
        pixel(&image, size, 8, 0) > 200,
        "the arc junction stays covered: {}",
        pixel(&image, size, 8, 0)
    );
    assert!(
        pixel(&image, size, 4, 0) < pixel(&image, size, 8, 0),
        "away from the junction the arc sags: {}/{}",
        pixel(&image, size, 4, 0),
        pixel(&image, size, 8, 0)
    );
    assert_eq!(pixel(&image, size, 8, 31), 255, "bottom edge straight");
    assert_eq!(pixel(&image, size, 0, 31), 255, "bottom-left stays square");
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn continuous_fill_and_content_clip_use_identical_coverage() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let size = [160, 120];
    let rect = kurbo::Rect::new(8.25, 8.25, 144.25, 108.25);
    let shape = plico_renderer::corner_shape::corner_shape(
        rect,
        plico_renderer::style::CornerRadii::uniform(18.),
        1.,
    );
    let mut filled = Scene::new();
    filled.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &shape);
    let expected = renderer
        .render_scene(&filled, size, [0.; 4], false)
        .unwrap();
    let mut clipped = Scene::new();
    clipped.push_clip_layer(Affine::IDENTITY, &shape);
    clipped.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &kurbo::Rect::new(0., 0., 160., 120.),
    );
    clipped.pop_layer();
    let actual = renderer
        .render_scene(&clipped, size, [0.; 4], false)
        .unwrap();
    let maximum = expected
        .iter()
        .zip(&actual)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        maximum <= 2,
        "continuous fill/clip coverage differs by {maximum}"
    );
    assert_eq!(pixel(&actual, size, 8, 8), 0);
    assert_eq!(pixel(&actual, size, 80, 60), 255);
}
