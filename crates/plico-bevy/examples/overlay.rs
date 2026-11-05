//! cargo run -p plico-bevy --example overlay
use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{RenderCreation, WgpuSettings},
    },
    window::WindowResolution,
};
use plico_bevy::{PlicoInput, PlicoPlugin, PlicoUi};

fn main() {
    // Match Plico's desktop default. On hybrid-GPU Wayland machines the
    // high-performance adapter may not be able to present to the compositor.
    // Explicit WGPU_POWER_PREF / WGPU_ADAPTER_NAME settings still take precedence.
    let settings = WgpuSettings {
        power_preference: wgpu::PowerPreference::from_env()
            .unwrap_or(wgpu::PowerPreference::LowPower),
        ..default()
    };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(Box::new(settings)),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Plico × Bevy".into(),
                        resolution: WindowResolution::new(1120, 760),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(PlicoPlugin::<Settings>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (animate, screenshot))
        .run();
}

#[derive(Component)]
struct Shape;
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Transform::from_xyz(6., 4., 8.).looking_at(Vec3::new(-1.6, 0.7, 0.), Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(16., 16.))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.27, 0.32))),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.6, 1.6, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.43, 0.84),
            metallic: 0.25,
            perceptual_roughness: 0.32,
            ..default()
        })),
        Transform::from_xyz(0., 1.2, 0.),
        Shape,
    ));
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.65))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.53, 0.18),
            perceptual_roughness: 0.3,
            ..default()
        })),
        Transform::from_xyz(2., 0.65, 0.7),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 7000.,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4., 7., 5.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn animate(
    time: Res<Time>,
    settings: NonSend<PlicoUi<Settings>>,
    capture: Res<PlicoInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut shapes: Query<&mut Transform, With<Shape>>,
) {
    if !settings.view().paused {
        for mut transform in &mut shapes {
            transform.rotate_y(time.delta_secs() * settings.view().speed as f32);
            transform.rotate_x(time.delta_secs() * settings.view().speed as f32 * 0.3);
            if !capture.wants_keyboard && keys.pressed(KeyCode::ArrowRight) {
                transform.translation.x += time.delta_secs() * 120.;
            }
        }
    }
}
// Optional reproducible window capture; no screenshot dependency in normal use.
fn screenshot(mut commands: Commands, mut frames: Local<u32>) {
    *frames += 1;
    if *frames == 40
        && let Ok(path) = std::env::var("PLICO_BEVY_CAPTURE")
    {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(path));
    }
}
struct Settings {
    count: i32,
    paused: bool,
    speed: f64,
    name: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            count: 0,
            paused: false,
            speed: 0.6,
            name: "你好，Bevy！".into(),
        }
    }
}
impl plico::View for Settings {
    fn theme(&self) -> plico::Theme {
        plico::Theme::default()
    }

    #[plico::component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut plico::UiContext) {
        use plico::prelude::*;
        column(cx).padding(28).gap(20).width(390.) + |cx| {
            column(cx).padding(24).gap(16).radius(16.).background(Color::Panel).border(1., Color::Border) + |cx| {
                text(cx, "Plico × Bevy").font_size(28).weight(700);
                text(cx, "One window. One GPU. Ordinary Rust UI.").color(Color::Muted);
                text_input_string(cx, "Name", &mut self.name).width_percent(100.);
                row(cx).gap(10).align(Align::Center) + |cx| {
                    if button(cx).text("−").ghost().clicked() {
                        self.count -= 1;
                    }
                    text!(cx, "{}", self.count).font_size(22).grow(1.);
                    if button(cx).text("+").primary().clicked() {
                        self.count += 1;
                    }
                };
                switch(cx, "Pause scene", &mut self.paused);
                slider(cx, "Rotation speed", &mut self.speed, 0. ..=2.).width_percent(100.);
                text(cx, "UI state stays on the main thread. The 3D scene is rendered by Bevy.").color(Color::Muted);
            };
            column(cx).height(210.).scroll_y(true).padding(16).gap(12).radius(12.).background(Color::Panel) + |cx| {
                text(cx, "Scrollable controls").weight(650);
                for i in 0_usize..12 {
                    row(cx).key(i).gap(8) + |cx| {
                        text!(cx, "Item {:02}", i + 1).grow(1.);
                        if button(cx).text("Add").ghost().clicked() {
                            self.count += 1;
                        }
                    };
                }
            };
        };
    }
}
