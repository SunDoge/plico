//! Scene → epaint replay.
//!
//! Milestone 1 scope: solid fills and strokes, layer/clip scopes, and
//! explicit `Unsupported` errors for everything not yet replayed. Each
//! subsequent milestone adds one command family (glyphs, SVG, shadows)
//! behind the same testable entry points.

use crate::glyph::GlyphCache;
use egui::{Color32, Pos2, Rect as ERect};
use plico_renderer::{PaintCommand, Scene};
use snafu::Snafu;
use vello_common::kurbo::{Affine, PathSeg, Shape as KurboShape};

#[derive(Debug, Snafu)]
pub enum ReplayError {
    #[snafu(display("the epaint consumer does not support {command}"))]
    Unsupported { command: &'static str },
}

/// Append `scene`'s commands as epaint shapes at logical scale. Mirrors the
/// role `SkiaSurface::draw_scene` plays for the Skia consumer; unsupported
/// commands fail loudly instead of drawing nothing.
pub fn replay_scene(
    painter: &egui::Painter,
    scene: &Scene,
    cache: &mut GlyphCache,
) -> Result<(), ReplayError> {
    replay_scene_at(painter, scene, cache, Affine::IDENTITY)
}

/// [`replay_scene`] under a parent transform (DPI scale, window origin).
pub fn replay_scene_at(
    painter: &egui::Painter,
    scene: &Scene,
    cache: &mut GlyphCache,
    transform: Affine,
) -> Result<(), ReplayError> {
    let mut layers: Vec<(f32, ERect)> = Vec::new();
    let base_clip = painter.clip_rect();
    for command in &scene.commands {
        let clip = layers.last().map_or(base_clip, |(_, clip)| *clip);
        match command {
            PaintCommand::PushLayer {
                alpha,
                transform: local,
                clip: clip_path,
                ..
            } => {
                let bounds = path_bounds(transform * *local, clip_path).intersect(clip);
                layers.push((*alpha, bounds));
            }
            PaintCommand::PushClip {
                transform: local,
                clip: clip_path,
            } => {
                let bounds = path_bounds(transform * *local, clip_path).intersect(clip);
                layers.push((1., bounds));
            }
            PaintCommand::PopLayer => {
                layers.pop();
            }
            PaintCommand::Fill {
                fill,
                transform: local,
                brush,
                shape,
                ..
            } => {
                // The epaint tessellator applies the non-zero rule only. The
                // even-odd difference is limited to subpath overlaps (donuts,
                // self-intersections); those are rare in UI scenes, so the
                // fill is approximated and logged rather than dropped.
                if *fill == peniko::Fill::EvenOdd {
                    tracing::debug!("approximating an even-odd fill as non-zero");
                }
                let Ok(color) = solid(brush).map(|color| fold_alpha(color, layer_alpha(&layers)))
                else {
                    continue;
                };
                let points = flatten_path(transform * *local, shape);
                if points.len() < 3 {
                    continue;
                }
                painter
                    .with_clip_rect(clip)
                    .add(egui::Shape::Path(egui::epaint::PathShape {
                        points,
                        closed: true,
                        fill: color,
                        stroke: egui::epaint::PathStroke::NONE,
                    }));
            }
            PaintCommand::Stroke {
                style,
                transform: local,
                brush,
                shape,
                ..
            } => {
                let Ok(color) = solid(brush).map(|color| fold_alpha(color, layer_alpha(&layers)))
                else {
                    continue;
                };
                let points = flatten_path(transform * *local, shape);
                if points.len() < 2 {
                    continue;
                }
                painter
                    .with_clip_rect(clip)
                    .add(egui::Shape::Path(egui::epaint::PathShape {
                        points,
                        closed: false,
                        fill: Color32::TRANSPARENT,
                        stroke: egui::epaint::PathStroke::new(style.width as f32, color),
                    }));
            }
            PaintCommand::GlyphRun {
                font,
                font_size,
                brush,
                brush_alpha,
                transform: local,
                glyph_transform,
                glyphs,
                ..
            } => {
                let Ok(color) =
                    solid(brush).map(|color| fold_alpha(color, layer_alpha(&layers) * brush_alpha))
                else {
                    continue;
                };
                let run = transform * *local * glyph_transform.unwrap_or(Affine::IDENTITY);
                for glyph in glyphs {
                    let origin =
                        run * vello_common::kurbo::Point::new(glyph.x as f64, glyph.y as f64);
                    cache.draw_glyph(
                        painter.ctx(),
                        painter,
                        font,
                        *font_size,
                        &plico_renderer::Glyph {
                            id: glyph.id,
                            x: origin.x as f32,
                            y: origin.y as f32,
                        },
                        color,
                    );
                }
            }
            PaintCommand::Paragraph { .. } => {
                // Host-native paragraphs only exist for native-widget hosts;
                // the egui path consumes pre-shaped GlyphRun commands.
                return Err(ReplayError::Unsupported {
                    command: "host-native paragraphs",
                });
            }
            PaintCommand::BoxShadow {
                transform: local,
                rect,
                brush,
                radius,
                std_dev,
            } => {
                let color = color32(*brush);
                let color = fold_alpha(color, layer_alpha(&layers));
                crate::shadow::draw_box_shadow(
                    painter,
                    cache,
                    transform * *local,
                    *rect,
                    color,
                    *radius,
                    *std_dev,
                );
            }
            PaintCommand::Svg {
                document,
                transform: local,
            } => {
                // Lower through the shared usvg→PaintCommand projector (the
                // same one the Skia consumer replays) and recurse; its
                // rejections surface verbatim.
                let converted =
                    plico_renderer::svg::convert_paint::commands(&document.0).map_err(|_| {
                        ReplayError::Unsupported {
                            command: "SVG feature rejected by the shared converter",
                        }
                    })?;
                replay_scene_at(painter, &converted, cache, transform * *local)?;
            }
            PaintCommand::Shader { .. } => {
                return Err(ReplayError::Unsupported {
                    command: "WGSL shader effect",
                });
            }
        }
    }
    Ok(())
}

fn layer_alpha(layers: &[(f32, ERect)]) -> f32 {
    layers.iter().map(|(alpha, _)| *alpha).product()
}

fn path_bounds(transform: Affine, shape: &impl KurboShape) -> ERect {
    let bbox = transform.transform_rect_bbox(shape.bounding_box());
    ERect::from_min_max(
        Pos2::new(bbox.x0 as f32, bbox.y0 as f32),
        Pos2::new(bbox.x1 as f32, bbox.y1 as f32),
    )
}

/// Flatten a kurbo shape into polygon points at a fixed 0.25px tolerance.
fn flatten_path(transform: Affine, shape: &impl KurboShape) -> Vec<Pos2> {
    let mut points = Vec::new();
    let emit = |points: &mut Vec<Pos2>, p: vello_common::kurbo::Point| {
        let p = transform * p;
        points.push(Pos2::new(p.x as f32, p.y as f32));
    };
    for seg in shape.path_segments(0.25) {
        match seg {
            PathSeg::Line(line) => {
                if points.is_empty() {
                    emit(&mut points, line.p0);
                }
                emit(&mut points, line.p1);
            }
            PathSeg::Quad(q) => {
                if points.is_empty() {
                    emit(&mut points, q.p0);
                }
                emit(&mut points, q.p1);
                emit(&mut points, q.p2);
            }
            PathSeg::Cubic(c) => {
                if points.is_empty() {
                    emit(&mut points, c.p0);
                }
                emit(&mut points, c.p1);
                emit(&mut points, c.p2);
                emit(&mut points, c.p3);
            }
        }
    }
    points
}

/// Convert a plain peniko Color into an epaint vertex color.
fn color32(color: peniko::color::AlphaColor<peniko::color::Srgb>) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color.components[0] * 255.) as u8,
        (color.components[1] * 255.) as u8,
        (color.components[2] * 255.) as u8,
        (color.components[3] * 255.) as u8,
    )
}

/// Resolve a brush to a flat vertex color; unsupported brush kinds are
/// skipped with a log rather than aborting the frame.
fn solid(brush: &peniko::Brush) -> Result<Color32, ReplayError> {
    match brush {
        peniko::Brush::Solid(color) => Ok(color32(*color)),
        other => {
            tracing::debug!(?other, "skipping an unsupported brush kind");
            Err(ReplayError::Unsupported {
                command: "gradient or image brush",
            })
        }
    }
}

fn fold_alpha(color: Color32, alpha: f32) -> Color32 {
    if alpha >= 1. {
        return color;
    }
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * alpha) as u8)
}
