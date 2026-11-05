use super::*;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyCode, KeyboardInput},
        mouse::{MouseButton, MouseButtonInput},
    },
    window::{CursorMoved, WindowEvent, WindowFocused},
};
use plico::prelude::{button, text_input_string};
use plico_renderer::Point;

#[derive(Default)]
struct TestView {
    count: u32,
    text: String,
}
impl plico::View for TestView {
    fn theme(&self) -> plico::Theme {
        Default::default()
    }
    #[plico::component]
    fn show(&mut self, cx: &mut plico::UiContext) {
        if button(cx)
            .key("increment")
            .text("Increment")
            .width(150.)
            .height(40.)
            .clicked()
        {
            self.count += 1;
        }
        text_input_string(cx, "Name", &mut self.text)
            .key("name")
            .width(180.)
            .height(40.);
    }
}
fn app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_message::<PlicoClipboardRequest>()
        .init_resource::<PlicoInput>()
        .insert_non_send(PlicoUi::new(TestView::default()))
        .add_systems(Update, input::handle_input::<TestView>);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    layout(&mut app);
    (app, window)
}
fn layout(app: &mut App) {
    let mut ui = app.world_mut().non_send_mut::<PlicoUi<TestView>>();
    let ui = &mut *ui;
    ui.source
        .build_and_paint(&mut ui.scene, &mut ui.text, 400, 300, 1.);
}
fn send(app: &mut App, event: impl Into<WindowEvent>) {
    app.world_mut().write_message(event.into());
}
fn click(app: &mut App, window: Entity, position: Vec2) {
    send(
        app,
        CursorMoved {
            window,
            position,
            delta: None,
        },
    );
    for state in [ButtonState::Pressed, ButtonState::Released] {
        send(
            app,
            MouseButtonInput {
                button: MouseButton::Left,
                state,
                window,
            },
        );
    }
}
#[test]
fn ordered_clicks_settle_and_outside_clicks_pass_through() {
    let (mut app, window) = app();
    click(&mut app, window, Vec2::new(20., 20.));
    click(&mut app, window, Vec2::new(20., 20.));
    app.update();
    layout(&mut app);
    assert_eq!(app.world().non_send::<PlicoUi<TestView>>().view().count, 2);
    assert!(app.world().resource::<PlicoInput>().pointer_consumed);
    click(&mut app, window, Vec2::new(350., 250.));
    app.update();
    let capture = app.world().resource::<PlicoInput>();
    assert!(!capture.pointer_consumed);
    assert!(!capture.wants_pointer);
    assert!(!capture.wants_keyboard);
}
#[test]
fn keyboard_text_and_ime_commit_reach_the_existing_editor() {
    let (mut app, window) = app();
    click(&mut app, window, Vec2::new(20., 60.));
    send(
        &mut app,
        KeyboardInput {
            window,
            key_code: KeyCode::KeyA,
            logical_key: Key::Character("a".into()),
            state: ButtonState::Pressed,
            text: Some("a".into()),
            repeat: false,
        },
    );
    send(
        &mut app,
        bevy::window::Ime::Preedit {
            window,
            value: "ni".into(),
            cursor: Some((2, 2)),
        },
    );
    send(
        &mut app,
        bevy::window::Ime::Commit {
            window,
            value: "你好".into(),
        },
    );
    app.update();
    layout(&mut app);
    assert_eq!(
        app.world().non_send::<PlicoUi<TestView>>().view().text,
        "a你好"
    );
    assert!(app.world().resource::<PlicoInput>().wants_keyboard);
}
#[test]
fn focus_loss_clears_capture_and_unrelated_windows_are_ignored() {
    let (mut app, window) = app();
    let other = app.world_mut().spawn_empty().id();
    click(&mut app, other, Vec2::new(20., 20.));
    send(
        &mut app,
        WindowFocused {
            window,
            focused: false,
        },
    );
    app.update();
    layout(&mut app);
    assert_eq!(app.world().non_send::<PlicoUi<TestView>>().view().count, 0);
    assert!(!app.world().resource::<PlicoInput>().wants_pointer);
}
#[test]
fn transparent_embedding_keeps_scene_and_retained_nodes() {
    let (mut app, _) = app();
    let ui = app.world().non_send::<PlicoUi<TestView>>();
    assert_eq!(ui.source.base_color(), peniko::Color::TRANSPARENT);
    assert!(ui.source.hit_test(Point { x: 20., y: 20. }).is_some());
    assert!(ui.source.hit_test(Point { x: 350., y: 250. }).is_none());
    let before = ui.scene.clone();
    let mut ui = app.world_mut().non_send_mut::<PlicoUi<TestView>>();
    let ui = &mut *ui;
    ui.source
        .render_retained_frame(&mut ui.scene, &mut ui.text, (400, 300), 1.);
    assert!(ui.scene.shares_commands_with(&before));
}
