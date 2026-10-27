//! Scene → epaint replay.
//!
//! Milestone 1 scope: solid fills and strokes, layer/clip scopes, and
//! explicit `Unsupported` errors for everything not yet replayed. Each
//! subsequent milestone adds one command family (glyphs, SVG, shadows)
//! behind the same testable entry points.

use egui::{Color32, Pos2, Rect as ERect};
use plico_renderer::{PaintCommand, Scene};
use snafu::Snafu;
use vello_common::kurbo::{Affine, PathSeg, Shape as KurboShape};

#[derive(Debug, Snafu)]
pub enum ReplayError {
    #[snafu(display("the epaint consumer does not support {command}"))]
    Unsupported { command: &'static str },
}

/// Replay state that survives across frames. Glyph coverage and textures
/// arrive with the glyph-run milestone; the type exists now so the public
/// replay entry points keep one signature across milestones.
#[derive(Default)]
pub struct GlyphCache {}

/// Append `scene`'s commands as epaint shapes at logical scale. Mirrors the
/// role `SkiaSurface::draw_scene` plays for the Skia consumer; unsupported
/// commands fail loudly instead of drawing nothing.
pub fn replay_scene(
    painter: &egui::Painter,
    scene: &Scene,
    _cache: &mut GlyphCache,
) -> Result<(), ReplayError> {
    replay_scene_at(painter, scene, Affine::IDENTITY)
}

/// [`replay_scene`] under a parent transform (DPI scale, window origin).
pub fn replay_scene_at(
    painter: &egui::Painter,
    scene: &Scene,
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
                // The epaint tessellator applies the non-zero rule only;
                // even-odd fills would need explicit shape decomposition.
                if *fill == vello_common::peniko::Fill::EvenOdd {
                    return Err(ReplayError::Unsupported {
                        command: "even-odd fill rule",
                    });
                }
                let color = fold_alpha(solid(brush)?, layer_alpha(&layers));
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
                let color = fold_alpha(solid(brush)?, layer_alpha(&layers));
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
            PaintCommand::GlyphRun { .. } => {
                return Err(ReplayError::Unsupported {
                    command: "glyph runs (milestone 2)",
                });
            }
            PaintCommand::Paragraph { .. } => {
                // Host-native paragraphs only exist for native-widget hosts;
                // the egui path consumes pre-shaped GlyphRun commands.
                return Err(ReplayError::Unsupported {
                    command: "host-native paragraphs",
                });
            }
            PaintCommand::BoxShadow { .. } => {
                return Err(ReplayError::Unsupported {
                    command: "box shadows (milestone 3)",
                });
            }
            PaintCommand::Svg { .. } => {
                return Err(ReplayError::Unsupported {
                    command: "SVG documents (milestone 3)",
                });
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

fn solid(brush: &vello_common::peniko::Brush) -> Result<Color32, ReplayError> {
    match brush {
        vello_common::peniko::Brush::Solid(color) => Ok(Color32::from_rgba_unmultiplied(
            (color.components[0] * 255.) as u8,
            (color.components[1] * 255.) as u8,
            (color.components[2] * 255.) as u8,
            (color.components[3] * 255.) as u8,
        )),
        _ => Err(ReplayError::Unsupported {
            command: "gradient or image brush",
        }),
    }
}

fn fold_alpha(color: Color32, alpha: f32) -> Color32 {
    if alpha >= 1. {
        return color;
    }
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * alpha) as u8)
}
