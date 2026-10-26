#![cfg(all(feature = "global-hotkeys", target_os = "linux"))]
use plico_native::hotkeys::{GlobalShortcuts, HotkeyEvent};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
#[test]
#[ignore = "requires an isolated X11 display and xdotool; run under xvfb-run"]
fn native_registration_delivery_conflict_and_release() {
    let (tx, rx) = mpsc::channel();
    let mut keys = GlobalShortcuts::new(Arc::new(move || {
        let _ = tx.send(());
    }))
    .unwrap();
    keys.register("toggle", "Ctrl+Shift+K", "Toggle test window", 17)
        .unwrap();
    assert!(matches!(
        keys.try_next(),
        Some(HotkeyEvent::Registered { .. })
    ));
    while rx.try_recv().is_ok() {}
    let mut second = GlobalShortcuts::<u32>::new(Arc::new(|| {})).unwrap();
    assert!(
        second
            .register("duplicate", "Ctrl+Shift+K", "Conflict", 0)
            .is_err()
    );
    // Root has focus, not any application window. This exercises the OS grab.
    assert!(
        std::process::Command::new("xdotool")
            .args(["key", "ctrl+shift+k"])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut pressed = false;
    while !pressed {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        while let Some(event) = keys.try_next() {
            if let HotkeyEvent::Pressed { id, .. } = event {
                assert_eq!(keys.action(&id), Some(&17));
                pressed = true;
            }
        }
    }
    keys.unregister("toggle").unwrap();
    assert!(!keys.is_registered("toggle"));
    second
        .register("released", "Ctrl+Shift+K", "Released", 18)
        .unwrap();
    second.unregister_all().unwrap();
    // Drop must release native grabs too.
    keys.register("drop", "Ctrl+Shift+K", "Drop", 19).unwrap();
    drop(keys);
    second
        .register("after-drop", "Ctrl+Shift+K", "After drop", 20)
        .unwrap();
}

#[test]
#[ignore = "launches an isolated child without an X11 display"]
fn missing_display_is_an_error_instead_of_success() {
    if std::env::var_os("PLICO_HOTKEY_TEST_CHILD").is_some() {
        assert!(GlobalShortcuts::<()>::new(Arc::new(|| {})).is_err());
        return;
    }
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "missing_display_is_an_error_instead_of_success"
            ])
            .env("PLICO_HOTKEY_TEST_CHILD", "1")
            .env("XDG_SESSION_TYPE", "x11")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .status()
            .unwrap()
            .success()
    );
}
