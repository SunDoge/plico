//! Manual native swapchain pixel oracle. Runs 180 frames and exits by itself.
//! `cargo run -p plico-skia --features window --example damage_window`
use plico_renderer::{Glyph, PaintScene, Scene};
use plico_skia::SkiaWindow;
use std::sync::Arc;
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

#[derive(Default)]
struct Demo {
    renderer: Option<SkiaWindow>,
    window: Option<Arc<dyn Window>>,
    frame: usize,
    partial: usize,
    hints: usize,
    hint_supported: bool,
    font: Option<(vello_common::peniko::FontData, Vec<u16>)>,
}
impl ApplicationHandler for Demo {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let window: Arc<dyn Window> = Arc::from(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Skia damage pixel oracle")
                        .with_surface_size(winit::dpi::LogicalSize::new(320., 240.)),
                )
                .unwrap(),
        );
        let device = std::env::var("PLICO_SKIA_DEVICE").ok();
        let mut renderer = SkiaWindow::new(window.clone(), false, device.as_deref()).unwrap();
        renderer.set_damage_tracking(true);
        renderer.set_damage_validation(true);
        let enabled = !matches!(
            std::env::var("PLICO_SKIA_INCREMENTAL_PRESENT").as_deref(),
            Ok("0" | "false")
        );
        renderer.set_incremental_present(enabled);
        self.hint_supported = enabled && renderer.incremental_present_supported();
        println!(
            "incremental_present_supported={}",
            renderer.incremental_present_supported()
        );
        println!("device={}", renderer.device_name());
        let data = vello_common::peniko::FontData::new(
            vello_common::peniko::Blob::new(Arc::new(
                include_bytes!("../../../backend/examples/web-preview/public/DejaVuSans.ttf")
                    .to_vec(),
            )),
            0,
        );
        let face = skia_safe::FontMgr::new()
            .new_from_bytes(data.data.as_ref(), 0)
            .unwrap();
        let font = skia_safe::Font::new(face, 36.);
        self.font = Some((data, font.text_to_glyphs_vec("sWjg")));
        self.renderer = Some(renderer);
        window.request_redraw();
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::SurfaceResized(size) => {
                self.renderer
                    .as_mut()
                    .unwrap()
                    .resize(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                let mut scene = Scene::new();
                if !self.frame.is_multiple_of(17) {
                    scene.fill(
                        Fill::NonZero,
                        Affine::translate(((self.frame % 120) as f64, 0.)),
                        Color::from_rgba8(230, 70, 40, 170),
                        None,
                        &Rect::new(8., 12., 48., 64.),
                    );
                }
                scene.push_layer(
                    vello_common::peniko::BlendMode::default(),
                    0.5,
                    Affine::IDENTITY,
                    &Rect::new(20., 24., 120., 100.),
                    None,
                    None,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgb8(30, 140, 240),
                    None,
                    &Rect::new(40., 32., 160., 80.),
                );
                scene.pop_layer();
                let (font, ids) = self.font.as_ref().unwrap();
                scene.draw_glyphs(
                    font,
                    36.,
                    true,
                    &[],
                    Default::default(),
                    Fill::NonZero,
                    if self.frame.is_multiple_of(5) {
                        Color::from_rgb8(180, 80, 40)
                    } else {
                        Color::from_rgb8(40, 80, 180)
                    },
                    1.,
                    Affine::translate(((self.frame % 40) as f64 * 0.25, 0.)),
                    None,
                    ids.iter().enumerate().map(|(i, id)| Glyph {
                        id: *id as u32,
                        x: 40. + i as f32 * 32.,
                        y: 180.,
                    }),
                );
                let renderer = self.renderer.as_mut().unwrap();
                if renderer
                    .present(
                        &scene,
                        if self.frame < 90 {
                            Color::WHITE
                        } else {
                            Color::BLACK
                        },
                    )
                    .unwrap()
                {
                    self.partial += usize::from(matches!(
                        renderer.last_damage(),
                        plico_renderer::DamageRegion::Rect(_)
                    ));
                    self.hints += usize::from(matches!(
                        renderer.last_present_damage(),
                        plico_renderer::DamageRegion::Rect(_)
                    ));
                    self.frame += 1;
                }
                if self.frame == 60 || self.frame == 120 {
                    let size = if self.frame == 60 {
                        (400., 280.)
                    } else {
                        (320., 240.)
                    };
                    let _ = window
                        .request_surface_size(winit::dpi::LogicalSize::new(size.0, size.1).into());
                }
                if self.frame >= 180 {
                    assert!(
                        self.partial > 100,
                        "must exercise partial swapchain drawing"
                    );
                    if self.hint_supported {
                        assert!(self.hints > 100, "must exercise incremental present hints");
                    }
                    println!(
                        "PASS frames={} partial={} present_hints={} (all matched full GPU redraw)",
                        self.frame, self.partial, self.hints
                    );
                    event_loop.exit();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}
fn main() {
    EventLoop::new().unwrap().run_app(Demo::default()).unwrap();
}
