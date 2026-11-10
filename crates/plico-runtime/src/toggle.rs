//! Shared geometry for toggle layout and paint; every size follows theme density.
use kurbo::{Affine, Circle, Rect, RoundedRect, Stroke};
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
            (4.5 * unit, 4.5 * unit)
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
    check: Option<&plico_renderer::svg::SvgImage>,
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
        let face = recipe::thumb_face(theme);
        let center = g.height / 2.;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            super::rgb(face),
            None,
            &Circle::new(
                (x + center + (g.width - g.height) * progress, y + center),
                center - g.unit / 2.,
            ),
        );
    } else if let Some(check) = check {
        let inset = g.unit * 0.25;
        let side = (g.width - 2. * inset) * (0.75 + 0.25 * progress);
        let origin = (x + (g.width - side) / 2., y + (g.height - side) / 2.);
        scene.push_layer(
            BlendMode::default(),
            progress as f32,
            Affine::IDENTITY,
            &Rect::new(x, y, x + g.width, y + g.height),
            None,
            None,
        );
        scene.draw_svg(
            check.document().clone(),
            Affine::translate(origin) * Affine::scale(side / check.size()[0] as f64),
        );
        scene.pop_layer();
    }
    scene
}
