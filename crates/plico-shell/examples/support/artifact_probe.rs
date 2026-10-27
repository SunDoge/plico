//! History and direct-outline controls on the persistent production GPU renderer.
use image::RgbaImage;
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico_shell::{PaintCommand, PaintScene, Scene, renderer};
use serde_json::{Value, json};
use std::path::Path;

struct Baseline {
    scene: Scene,
    width: u32,
    height: u32,
    background: Color,
    image: RgbaImage,
}

#[derive(Default)]
pub struct Probe {
    first: Option<Baseline>,
    records: Vec<Value>,
}

fn render(scene: &Scene, width: u32, height: u32, background: Color) -> RgbaImage {
    renderer::render_to_image(scene, width, height, background).expect("artifact render")
}

fn equal(actual: &RgbaImage, expected: &RgbaImage, label: &str, output: &Path) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    if actual.as_raw() != expected.as_raw() {
        actual.save(output.join("failure-actual.png")).unwrap();
        expected.save(output.join("failure-expected.png")).unwrap();
        let changed = actual
            .pixels()
            .zip(expected.pixels())
            .filter(|(a, b)| a != b)
            .count();
        panic!("{label}: {changed} changed pixels; failure images saved");
    }
}

fn tiny_components(image: &RgbaImage, background: [u8; 4]) -> Value {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let mut visited = vec![false; width * height];
    let foreground: Vec<bool> = image.pixels().map(|p| p.0 != background).collect();
    let mut stack = Vec::new();
    let mut count = 0;
    let mut sample = Vec::new();
    for start in 0..foreground.len() {
        if visited[start] || !foreground[start] {
            continue;
        }
        visited[start] = true;
        stack.push(start);
        let mut area = 0;
        let mut maximum = 0;
        while let Some(index) = stack.pop() {
            area += 1;
            let x = index % width;
            let y = index / width;
            let p = image.get_pixel(x as u32, y as u32).0;
            maximum = maximum.max(
                p.iter()
                    .zip(background)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap(),
            );
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let next = ny * width + nx;
                    if foreground[next] && !visited[next] {
                        visited[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        if area <= 2 {
            count += 1;
            if sample.len() < 12 {
                sample
                    .push(json!({"x":start%width,"y":start/width,"area":area,"contrast":maximum}));
            }
        }
    }
    // Dots on i / punctuation and small CJK marks are not automatically errors.
    json!({"eight_connected_components_up_to_two_pixels":count,"sample":sample})
}

impl Probe {
    pub fn check(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        background: Color,
        output: &Path,
        scale: f64,
    ) {
        for command in scene.commands.iter() {
            if let PaintCommand::GlyphRun {
                hint,
                embolden,
                glyph_transform,
                ..
            } = command
            {
                assert!(
                    *hint && embolden.x == 0. && embolden.y == 0. && glyph_transform.is_none(),
                    "artifact controls require hinted, unexpanded, upright outlines"
                );
            }
        }
        let baseline = render(scene, width, height, background);
        let mut outlines = scene.clone();
        crate::hint_probe::rewrite(&mut outlines, "lcd", None);
        equal(
            &baseline,
            &render(&outlines, width, height, background),
            "glyph vs direct outline",
            output,
        );
        for _ in 0..4 {
            equal(
                &render(scene, width, height, background),
                &baseline,
                "static repeat",
                output,
            );
        }
        let positions = [
            (0.125, 0.),
            (-0.125, 0.),
            (0., 0.25),
            (0., 0.5),
            (0., 1.),
            (13.25, -7.5),
        ];
        for (dx, dy) in positions {
            let mut moved = scene.clone();
            for command in moved.commands.iter_mut() {
                if let PaintCommand::GlyphRun { transform, .. } = command {
                    *transform = Affine::translate((dx, dy)) * *transform;
                }
            }
            let actual = render(&moved, width, height, background);
            let mut direct = moved.clone();
            crate::hint_probe::rewrite(&mut direct, "lcd", None);
            equal(
                &actual,
                &render(&direct, width, height, background),
                "moved glyph vs outline",
                output,
            );
            equal(
                &render(scene, width, height, background),
                &baseline,
                "return from movement",
                output,
            );
        }
        let mut poison = Scene::new();
        poison.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(255, 0, 255),
            None,
            &Rect::new(0., 0., width.into(), height.into()),
        );
        let _ = render(&poison, width, height, background);
        let blank = render(&Scene::new(), width, height, background);
        let bg = background.to_rgba8();
        let bg = [bg.r, bg.g, bg.b, bg.a];
        assert_eq!(bg[3], 255, "opaque background fixture required");
        assert!(
            blank.pixels().all(|p| p.0 == bg),
            "previous-frame pixels survived clear"
        );
        equal(
            &render(scene, width, height, background),
            &baseline,
            "return from clear",
            output,
        );
        self.records
            .push(json!({"scale":scale,"width":width,"height":height,
            "static_repeats":4,"translation_cases":positions.len(),"unexpected_outline_pixels":0,
            "history_changed_pixels":0,"tiny_components":tiny_components(&baseline,bg)}));
        println!("artifact controls passed: {scale}x");
        if self.first.is_none() {
            self.first = Some(Baseline {
                scene: scene.clone(),
                width,
                height,
                background,
                image: baseline,
            });
        }
    }

    pub fn finish(self, output: &Path) {
        if let Some(first) = self.first {
            equal(
                &render(&first.scene, first.width, first.height, first.background),
                &first.image,
                "return after DPI/target changes",
                output,
            );
        }
        std::fs::write(
            output.join("artifacts.json"),
            serde_json::to_vec_pretty(&json!({
                "cross_dpi_return_changed_pixels":0,"cases":self.records,
                "scope":"persistent production offscreen renderer; not window/compositor timing"
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
