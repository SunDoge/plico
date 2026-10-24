//! Isolate post-hint geometric expansion on an integer-aligned one-pixel stem.
use plico_shell::{PaintScene, Scene};
use vello_common::{
    kurbo::{Affine, Diagonal2, Join, Rect, Shape, expand_path},
    peniko::{Color, Fill},
};

fn main() {
    let output = std::path::Path::new("reports/hinted-embolden");
    std::fs::create_dir_all(output).unwrap();
    let black = Color::from_rgb8(0, 0, 0);
    let white = Color::from_rgb8(255, 255, 255);
    for (boost, tolerance) in [(0., 0.1), (2., 0.1), (2., 0.01), (2., 0.001)] {
        // Production boost formula for logical 14px text at 1.75x.
        let amount = 14. * 1.75 * boost / 1000.;
        let stem = Rect::new(20., 20., 21., 44.).to_path(0.1);
        let path = if boost == 0. {
            stem
        } else {
            // Same operation/default controls used by Glifo 0.4.0.
            expand_path(
                stem,
                Diagonal2::new(amount, amount),
                Join::Miter,
                4.,
                tolerance,
            )
        };
        let mut scene = Scene::new();
        let diagonal_edges = path
            .segments()
            .filter(|segment| {
                let vello_common::kurbo::PathSeg::Line(line) = segment else {
                    return false;
                };
                line.p0.x != line.p1.x && line.p0.y != line.p1.y
            })
            .count();
        if boost != 0. {
            assert_eq!(diagonal_edges > 0, tolerance == 0.1);
        }
        println!(
            "outline boost={boost} tolerance={tolerance}: {:?}",
            path.elements()
        );
        scene.fill(Fill::NonZero, Affine::IDENTITY, white, None, &path);
        let filename = output.join(format!("boost-{boost}-tolerance-{tolerance}.png"));
        plico_shell::renderer::render_to_png(&scene, 48, 64, black, filename.to_str().unwrap())
            .unwrap();
        let image = image::open(filename).unwrap().to_rgba8();
        let columns: Vec<_> = (18..24).map(|x| image.get_pixel(x, 30).0[0]).collect();
        println!(
            "boost={boost} tolerance={tolerance} expansion={amount} bbox={:?} row={columns:?}",
            path.bounding_box()
        );
        if boost == 0. {
            assert_eq!(columns, [0, 0, 255, 0, 0, 0]);
        } else {
            assert_eq!(columns[2], 255);
            assert!(columns[1] > 0 && columns[1] <= 15);
            assert!(columns[3] > 0 && columns[3] <= 15);
        }
        let mut rectangle_scene = Scene::new();
        rectangle_scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            white,
            None,
            &path.bounding_box(),
        );
        let filename = output.join(format!("rectangle-{boost}-tolerance-{tolerance}.png"));
        plico_shell::renderer::render_to_png(
            &rectangle_scene,
            48,
            64,
            black,
            filename.to_str().unwrap(),
        )
        .unwrap();
        let image = image::open(filename).unwrap().to_rgba8();
        let columns: Vec<_> = (18..24).map(|x| image.get_pixel(x, 30).0[0]).collect();
        println!("rectangle fast path boost={boost} row={columns:?}");
    }
}
