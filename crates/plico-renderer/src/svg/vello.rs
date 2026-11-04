//! Optional SVG projection into Vello scenes; parsing stays backend-neutral.
use super::{Error, RasterImageSnafu, RenderReport, UnsupportedFeature, parse};
use vello_gpu::Scene;
mod convert;
mod render;

/// Parse and append an SVG document to an existing Vello GPU scene.
pub fn append(
    scene: &mut Scene,
    svg: &str,
    options: &usvg::Options,
) -> Result<RenderReport, Error> {
    let tree = parse(svg, options)?;
    append_tree(scene, &tree)
}

/// Append a normalized SVG tree to an existing Vello GPU scene.
pub fn append_tree(scene: &mut Scene, tree: &usvg::Tree) -> Result<RenderReport, Error> {
    append_tree_with_transform(scene, tree, kurbo::Affine::IDENTITY)
}

/// Append a normalized SVG tree under a caller-provided root transform.
pub fn append_tree_with_transform(
    scene: &mut Scene,
    tree: &usvg::Tree,
    transform: kurbo::Affine,
) -> Result<RenderReport, Error> {
    let mut report = RenderReport::default();
    render::render_group(scene, tree.root(), transform, &mut report)?;
    Ok(report)
}

/// Parse an SVG and create a Vello GPU scene sized by the caller.
pub fn render(
    svg: &str,
    options: &usvg::Options,
    width: u16,
    height: u16,
) -> Result<(Scene, RenderReport), Error> {
    let mut scene = Scene::new(width, height);
    let report = append(&mut scene, svg, options)?;
    Ok((scene, report))
}
