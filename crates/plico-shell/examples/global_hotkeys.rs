//! Minimal host integration: press Ctrl/Cmd+Shift+K from another application.
//! On Wayland the desktop asks for approval and may assign a different chord.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use peniko::Color;
    use plico_shell::{
        ExtensionContext, FrameSource, TextContext, WindowOptions,
        hotkeys::{GlobalHotkeyBinding, GlobalHotkeyExtension, HotkeyEvent},
        initial_window_resource_key, run_windows_with_factory_and_extensions,
    };
    struct Background;
    impl FrameSource for Background {
        fn build_frame(
            &mut self,
            _: &mut TextContext,
            _: u32,
            _: u32,
        ) -> Vec<plico_shell::layout::PlacedNode> {
            Vec::new()
        }
        fn base_color(&self) -> Color {
            Color::from_rgb8(32, 36, 44)
        }
    }
    let window = initial_window_resource_key(0);
    let mut hidden = false;
    let extension = GlobalHotkeyExtension::new(
        vec![GlobalHotkeyBinding {
            id: "toggle-window".into(),
            accelerator: "CmdOrCtrl+Shift+K".into(),
            description: "Show or hide the Plico hotkey demo".into(),
            action: window,
        }],
        move |event: &HotkeyEvent,
              window: &plico_shell::WindowResourceKey,
              context: &mut ExtensionContext<'_>| {
            match event {
                HotkeyEvent::Pressed { .. } => {
                    let changed = if hidden {
                        context.show_window(*window)
                    } else {
                        context.hide_window(*window)
                    };
                    if changed {
                        hidden = !hidden;
                    }
                    println!("global activation: hidden={hidden}");
                }
                HotkeyEvent::Failed { message, .. } => eprintln!("global hotkey failed: {message}"),
                HotkeyEvent::Registered { trigger, .. } => println!("registered: {trigger}"),
                _ => {}
            }
        },
    );
    println!(
        "Focus another application, then press Ctrl/Cmd+Shift+K to toggle this window. Close it to exit."
    );
    run_windows_with_factory_and_extensions(
        vec![(
            Box::new(Background),
            WindowOptions::new()
                .title("Plico global hotkey — Ctrl/Cmd+Shift+K")
                .initial_inner_size(560, 240),
        )],
        None,
        vec![Box::new(extension)],
    )?;
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn main() {
    eprintln!("System global shortcuts are unavailable on this target");
}
