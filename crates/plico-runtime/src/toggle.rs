//! Shared geometry for toggle layout and paint; every size follows theme density.
use kurbo::{Affine, BezPath, Cap, Join, Rect, RoundedRect, Stroke};
use peniko::{BlendMode, Fill};
use plico::{Kind, Style, Theme};
use plico_renderer::{PaintScene, Scene};

use super::recipe;

struct Geometry {
    unit: f64,
    width: f64,
    height: f64,
    gap: f64,
}
impl Geometry {
    fn new(kind: Kind, style: Style, theme: Theme) -> Self {
        let unit = theme.metrics.spacing.max(1) as f64;
        let (width, height) = if kind == Kind::Switch {
            (9. * unit, 5. * unit)
        } else {
            (4. * unit, 4. * unit)
        };
        Self {
            unit,
            width,
            height,
            gap: style.gap as f64,
        }
    }
}
pub(crate) fn left_padding(kind: Kind, style: Style, theme: Theme) -> f32 {
    let g = Geometry::new(kind, style, theme);
    style.horizontal_padding() + (g.width + g.gap) as f32
}
pub(crate) fn paint(
    kind: Kind,
    style: Style,
    theme: Theme,
    progress: f32,
    line_height: f32,
    hovered: bool,
) -> Scene {
    let g = Geometry::new(kind, style, theme);
    let progress = progress.clamp(0., 1.) as f64;
    let amount = progress as f32;
    let bg = super::rgb(match kind {
        Kind::Switch => recipe::switch_track(theme, amount),
        _ => recipe::checkbox_face(theme, amount),
    });
    // Scene fragments are relative to content origin; indicator lives in left padding.
    let x = -g.width - g.gap;
    let y = (line_height as f64 - g.height) / 2.;
    let mut scene = Scene::default();
    let radius = if kind == Kind::Switch {
        g.height / 2.
    } else {
        g.unit.min(g.height / 2.)
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        bg,
        None,
        &RoundedRect::new(x, y, x + g.width, y + g.height, radius),
    );
    if kind == Kind::Checkbox {
        // Stroke inside the allocated indicator box; density does not change
        // the one-logical-pixel outline or its alignment with the label.
        scene.stroke(
            &Stroke::new(1.),
            Affine::IDENTITY,
            super::rgb(recipe::checkbox_rim(theme, amount, hovered)),
            None,
            &RoundedRect::new(
                x + 0.5,
                y + 0.5,
                x + g.width - 0.5,
                y + g.height - 0.5,
                (radius - 0.5).max(0.),
            ),
        );
    }
    if kind == Kind::Switch {
        // A physical thumb stays light in both appearances (MyGO's white
        // face, softened in the dark); `on_accent` is text, not the thumb.
        let face = 0xffffff;
        let center = g.height / 2.;
        let cx = x + center + (g.width - g.height) * progress;
        let cy = y + center;
        let r = center - g.unit / 2.;
        scene.draw_box_shadow(
            Affine::translate((0., 1.)),
            Rect::new(cx - r, cy - r, cx + r, cy + r),
            peniko::Color::from_rgba8(0, 0, 0, 64),
            r,
            1.5,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            super::rgb(face),
            None,
            // MyGO paints the knob through the same analytical rounded-rect
            // primitive as the track (`Rect` + `d/2`), even though it is a
            // circle. Keeping both pieces on that fast path gives them the
            // same edge coverage; a kurbo `Circle` would be flattened into an
            // arbitrary path and rasterized by a different AA path.
            &RoundedRect::new(cx - r, cy - r, cx + r, cy + r, r),
        );
    } else if progress > 0. {
        // MyGO ui/widgets.go: checkPath uses these three proportional points
        // and a half-spacing round stroke. Keep it as a vector path; checkbox
        // marks do not need the SVG/image pipeline.
        let mut path = BezPath::new();
        path.move_to((x + g.width * 0.22, y + g.height * 0.52));
        path.line_to((x + g.width * 0.42, y + g.height * 0.71));
        path.line_to((x + g.width * 0.78, y + g.height * 0.31));
        scene.push_layer(
            BlendMode::default(),
            progress as f32,
            Affine::IDENTITY,
            &Rect::new(x, y, x + g.width, y + g.height),
            None,
            None,
        );
        scene.stroke(
            &Stroke::new(g.unit * 0.5)
                .with_caps(Cap::Round)
                .with_join(Join::Round),
            Affine::IDENTITY,
            super::rgb(theme.on_accent),
            None,
            &path,
        );
        scene.pop_layer();
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::{PaintCommand, PrimitiveShape};

    #[test]
    fn checkbox_mark_is_the_mygo_vector_path_not_an_svg() {
        let scene = paint(
            Kind::Checkbox,
            Style::default(),
            Theme::default(),
            1.,
            16.,
            false,
        );
        assert!(
            !scene
                .commands
                .iter()
                .any(|command| matches!(command, PaintCommand::Svg { .. }))
        );
        assert_eq!(
            scene
                .commands
                .iter()
                .filter(|command| matches!(command, PaintCommand::Stroke { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn switch_track_and_knob_share_the_analytical_round_rect_path() {
        let scene = paint(
            Kind::Switch,
            Style::default(),
            Theme::default(),
            0.5,
            20.,
            false,
        );
        let fills = scene.commands.iter().filter_map(|command| match command {
            PaintCommand::Fill { shape, .. } => Some(shape),
            _ => None,
        });
        let primitives = fills.map(|shape| shape.primitive()).collect::<Vec<_>>();
        assert_eq!(primitives.len(), 2);
        assert!(
            primitives
                .iter()
                .all(|shape| matches!(shape, Some(PrimitiveShape::RoundedRect(_))))
        );
    }
}
