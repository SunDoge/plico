use peniko::Color;
use plico_shell::{Scene, Shell, WindowOptions};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

struct Probe {
    scene: Option<Scene>,
    shell: Option<Shell>,
    width: u32,
    height: u32,
    scale: f64,
    background: Color,
}

impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let options = WindowOptions {
            title: "Plico text surface probe".into(),
            initial_inner_size: (self.width, self.height),
            decorations: false,
            resizable: false,
            ..Default::default()
        };
        let mut shell = Shell::create(event_loop, &options).expect("create probe window");
        assert_eq!(
            shell.scale_factor(),
            self.scale,
            "fixture and window DPI must match"
        );
        assert_eq!(
            shell.size(),
            (
                (self.width as f64 * self.scale).round() as u32,
                (self.height as f64 * self.scale).round() as u32
            )
        );
        shell.scene = self.scene.take().expect("single probe surface");
        shell.window.request_redraw();
        self.shell = Some(shell);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(shell) = &mut self.shell {
                    shell.present(self.background);
                    println!("probe-presented");
                }
            }
            _ => {}
        }
    }
}

pub fn show(scene: Scene, width: u32, height: u32, scale: f64, background: Color) {
    let event_loop = EventLoop::new().expect("probe event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(Probe {
            scene: Some(scene),
            shell: None,
            width,
            height,
            scale,
            background,
        })
        .expect("run probe window");
}
