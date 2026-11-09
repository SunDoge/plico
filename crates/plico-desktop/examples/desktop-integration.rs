//! cargo run -p plico-desktop --features desktop-integration --example desktop-integration
use plico::prelude::*;
use plico_desktop::{HostView, Source, WindowOptions, desktop::*};
struct Demo {
    native: DesktopClient,
    status: String,
    next: u64,
}
impl HostView for Demo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(32).gap(16).width_percent(100.) + |cx| {
            text(cx, "Native desktop services").font_size(24).weight(650);
            text(cx, "Close to hide; use the tray menu to restore or quit.");
            text(cx, &self.status);
            if button(cx).text("Send notification").clicked() {
                self.next += 1;
                let request = plico_desktop::NotificationRequest {
                    title: "Plico desktop example".into(),
                    body: Some("Click to restore this window.".into()),
                    icon: None,
                    silent: true,
                };
                self.status = match self.native.handle().notify(self.next, request) {
                    Ok(()) => "Notification queued".into(),
                    Err(error) => error.to_string(),
                };
            }
        };
    }
    fn poll_updates(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.native.try_recv() {
            self.status = format!("{event:?}");
            changed = true;
        }
        changed
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rgba = vec![0; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            if (x as i32 - 8).pow(2) + (y as i32 - 8).pow(2) < 45 {
                rgba[(y * 16 + x) * 4..][..4].copy_from_slice(&[45, 90, 120, 255]);
            }
        }
    }
    let (native, extension) = integration(DesktopOptions {
        app_id: "org.plico.desktop-example".into(),
        title: "Plico desktop example".into(),
        window: plico_desktop::initial_window_resource_key(0),
        image: TrayImage {
            rgba,
            width: 16,
            height: 16,
            macos_template: true,
        },
        menu: vec![
            TrayItem::Action {
                id: "open".into(),
                label: "Open example".into(),
                action: MenuAction::ShowWindow,
            },
            TrayItem::Action {
                id: "custom".into(),
                label: "Application action".into(),
                action: MenuAction::Emit,
            },
            TrayItem::Separator,
            TrayItem::Action {
                id: "quit".into(),
                label: "Quit".into(),
                action: MenuAction::Exit,
            },
        ],
        close_to_tray: true,
        notifications_enabled: true,
    })?;
    plico_desktop::run_source_with_extensions(
        Source::new(Demo {
            native,
            status: "Ready".into(),
            next: 0,
        }),
        WindowOptions::new().title("Plico desktop example"),
        vec![Box::new(extension)],
    )?;
    Ok(())
}
