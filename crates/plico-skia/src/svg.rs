// Geometry conversion adapted from plico-renderer/src/svg/convert.rs.
// Copyright 2024 the Vello Authors and 2026 the Wabou contributors
// SPDX-License-Identifier: Apache-2.0
use crate::Error;
use plico_renderer::{PaintScene, Scene, svg::usvg};
use vello_common::{
    kurbo::{Affine, BezPath, Cap, Join, Point, Stroke},
    peniko::{BlendMode, Color, Fill},
};

pub(crate) fn commands(tree: &usvg::Tree) -> Result<Scene, Error> {
    let mut scene = Scene::new();
    group(&mut scene, tree.root())?;
    Ok(scene)
}
fn unsupported<T>(command: &'static str) -> Result<T, Error> {
    Err(Error::Unsupported { command })
}
fn brush(paint: &usvg::Paint, opacity: usvg::Opacity) -> Result<Color, Error> {
    match paint {
        usvg::Paint::Color(c) => Ok(Color::from_rgba8(
            c.red,
            c.green,
            c.blue,
            (opacity.get() * 255.).round() as u8,
        )),
        _ => unsupported("SVG gradient/pattern"),
    }
}
fn group(scene: &mut Scene, node_group: &usvg::Group) -> Result<(), Error> {
    if node_group.mask().is_some()
        || !node_group.filters().is_empty()
        || node_group.clip_path().is_some()
        || node_group.blend_mode() != usvg::BlendMode::Normal
    {
        return unsupported("SVG mask/filter/clip/blend");
    }
    let layered = node_group.opacity().get() != 1.;
    if layered {
        let rect = node_group.abs_layer_bounding_box();
        scene.push_layer(
            BlendMode::default(),
            node_group.opacity().get(),
            Affine::IDENTITY,
            &vello_common::kurbo::Rect::new(
                rect.left() as f64,
                rect.top() as f64,
                rect.right() as f64,
                rect.bottom() as f64,
            ),
            None,
            None,
        );
    }
    for node in node_group.children() {
        match node {
            usvg::Node::Group(child) => group(scene, child)?,
            usvg::Node::Text(text) => group(scene, text.flattened())?,
            usvg::Node::Image(_) => return unsupported("SVG image"),
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let geometry = path(p);
                let transform = affine(&p.abs_transform());
                let fill = |scene: &mut Scene| -> Result<(), Error> {
                    if let Some(fill) = p.fill() {
                        scene.fill(
                            match fill.rule() {
                                usvg::FillRule::NonZero => Fill::NonZero,
                                usvg::FillRule::EvenOdd => Fill::EvenOdd,
                            },
                            transform,
                            brush(fill.paint(), fill.opacity())?,
                            None,
                            &geometry,
                        );
                    }
                    Ok(())
                };
                let outline = |scene: &mut Scene| -> Result<(), Error> {
                    if let Some(s) = p.stroke() {
                        if s.linejoin() == usvg::LineJoin::MiterClip {
                            return unsupported("SVG miter-clip");
                        }
                        scene.stroke(
                            &stroke(s),
                            transform,
                            brush(s.paint(), s.opacity())?,
                            None,
                            &geometry,
                        );
                    }
                    Ok(())
                };
                match p.paint_order() {
                    usvg::PaintOrder::FillAndStroke => {
                        fill(scene)?;
                        outline(scene)?;
                    }
                    usvg::PaintOrder::StrokeAndFill => {
                        outline(scene)?;
                        fill(scene)?;
                    }
                }
            }
        }
    }
    if layered {
        scene.pop_layer();
    }
    Ok(())
}
fn affine(transform: &usvg::Transform) -> Affine {
    Affine::new(
        [
            transform.sx,
            transform.ky,
            transform.kx,
            transform.sy,
            transform.tx,
            transform.ty,
        ]
        .map(f64::from),
    )
}

fn path(path: &usvg::Path) -> BezPath {
    let mut output = BezPath::new();
    let mut just_closed = false;
    let mut subpath_start = Point::ZERO;

    for segment in path.data().segments() {
        let reopen = |output: &mut BezPath, just_closed: &mut bool| {
            if std::mem::take(just_closed) {
                output.move_to(subpath_start);
            }
        };
        match segment {
            usvg::tiny_skia_path::PathSegment::MoveTo(point) => {
                just_closed = false;
                subpath_start = Point::new(point.x.into(), point.y.into());
                output.move_to(subpath_start);
            }
            usvg::tiny_skia_path::PathSegment::LineTo(point) => {
                reopen(&mut output, &mut just_closed);
                output.line_to((f64::from(point.x), f64::from(point.y)));
            }
            usvg::tiny_skia_path::PathSegment::QuadTo(control, point) => {
                reopen(&mut output, &mut just_closed);
                output.quad_to(
                    (f64::from(control.x), f64::from(control.y)),
                    (f64::from(point.x), f64::from(point.y)),
                );
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(first, second, point) => {
                reopen(&mut output, &mut just_closed);
                output.curve_to(
                    (f64::from(first.x), f64::from(first.y)),
                    (f64::from(second.x), f64::from(second.y)),
                    (f64::from(point.x), f64::from(point.y)),
                );
            }
            usvg::tiny_skia_path::PathSegment::Close => {
                just_closed = true;
                output.close_path();
            }
        }
    }
    output
}

fn stroke(stroke: &usvg::Stroke) -> Stroke {
    let mut output = Stroke::new(f64::from(stroke.width().get()))
        .with_caps(match stroke.linecap() {
            usvg::LineCap::Butt => Cap::Butt,
            usvg::LineCap::Round => Cap::Round,
            usvg::LineCap::Square => Cap::Square,
        })
        .with_join(match stroke.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => Join::Miter,
            usvg::LineJoin::Round => Join::Round,
            usvg::LineJoin::Bevel => Join::Bevel,
        })
        .with_miter_limit(f64::from(stroke.miterlimit().get()));
    if let Some(dashes) = stroke.dasharray() {
        output = output.with_dashes(
            f64::from(stroke.dashoffset()),
            dashes.iter().map(|value| f64::from(*value)),
        );
    }
    output
}
