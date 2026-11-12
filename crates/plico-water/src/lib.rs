//! Persistent GPU water simulation for Plico.
//!
//! The wave update follows Evan Wallace's MIT-licensed WebGL Water technique:
//! <https://github.com/evanw/webgl-water>. This implementation and its WGSL
//! presentation shaders are native Plico code rather than copied WebGL source.

mod caustic;
mod mesh;
mod scene;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use plico_renderer::{
    ExternalSurface, GpuContext, KeyPhase, PaintContext, PointerButton, PointerPhase, UiEvent,
    WakeCallback, Widget, WidgetChanges, WidgetEventResult, WidgetGeometry,
};
use plico_wgpu::{ExternalGpuContext, ExternalTexture};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;

const SIMULATION_SIZE: u32 = 256;
const CAUSTIC_SIZE: u32 = 1024;
const MAX_OUTPUT_EDGE: f32 = 1920.0;
const MAX_SIM_PASSES: usize = 24;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimParams {
    drop: [f32; 4],
    old_sphere: [f32; 4],
    new_sphere: [f32; 4],
    control: [f32; 4],
}

/// Interactive, continuously animated water rendered on Plico's wgpu device.
pub struct WaterSurface {
    engine: Option<Engine>,
    geometry: WidgetGeometry,
    pending_drop: Option<[f32; 2]>,
    drag: DragMode,
    angle_x: f32,
    angle_y: f32,
    sphere: Vec3,
    rendered_sphere: Vec3,
    velocity: Vec3,
    gravity_enabled: bool,
    paused: bool,
    light_tracks_camera: bool,
    initialized: bool,
    visible: bool,
    revision: u64,
    surface_id: u64,
    shared: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Copy, Default)]
enum DragMode {
    #[default]
    None,
    Drop,
    Orbit {
        last: [f64; 2],
    },
    Sphere {
        previous_hit: Vec3,
        plane_normal: Vec3,
    },
}

#[derive(Clone, Copy)]
struct DropCommand {
    position: [f32; 2],
    radius: f32,
    strength: f32,
}

struct SharedState {
    commands: VecDeque<DropCommand>,
    wake: WakeCallback,
}

/// Cloneable command handle for ordinary Plico controls layered over the water.
#[derive(Clone)]
pub struct WaterController {
    shared: Arc<Mutex<SharedState>>,
}

impl WaterController {
    /// Add a circular disturbance at normalized surface coordinates.
    pub fn drop_at(&self, position: [f32; 2], radius: f32, strength: f32) {
        let wake = {
            let mut shared = self.shared.lock().unwrap();
            shared.commands.push_back(DropCommand {
                position: [position[0].clamp(0., 1.), position[1].clamp(0., 1.)],
                radius: radius.clamp(0.005, 0.25),
                strength: strength.clamp(-0.1, 0.1),
            });
            shared.wake.clone()
        };
        wake();
    }
}

impl Default for WaterSurface {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let surface_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let commands = initial_drops(surface_id);
        Self {
            engine: None,
            geometry: WidgetGeometry::default(),
            pending_drop: None,
            drag: DragMode::None,
            angle_x: -25.0,
            angle_y: -200.5,
            sphere: Vec3::new(-0.4, -0.75, 0.2),
            rendered_sphere: Vec3::new(-0.4, -0.75, 0.2),
            velocity: Vec3::ZERO,
            gravity_enabled: false,
            paused: false,
            light_tracks_camera: false,
            initialized: false,
            visible: true,
            revision: 0,
            surface_id,
            shared: Arc::new(Mutex::new(SharedState {
                commands,
                wake: Arc::new(|| {}),
            })),
        }
    }
}

fn initial_drops(seed: u64) -> VecDeque<DropCommand> {
    // The original uses Math.random(). A tiny deterministic generator keeps
    // captures reproducible while retaining its scattered alternating drops.
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    (0..20)
        .map(|index| {
            let mut next = || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state as u32) as f32 / u32::MAX as f32
            };
            DropCommand {
                position: [next(), next()],
                radius: 0.03,
                strength: if index & 1 == 0 { -0.01 } else { 0.01 },
            }
        })
        .collect()
}

impl WaterSurface {
    /// Construct the water surface with one initial ripple.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a surface and a handle usable by surrounding ordinary UI.
    pub fn controlled() -> (WaterController, Self) {
        let surface = Self::new();
        let controller = WaterController {
            shared: surface.shared.clone(),
        };
        (controller, surface)
    }

    fn normalized_pointer(&self, x: f64, y: f64) -> Option<[f32; 2]> {
        let [width, height] = self.geometry.content_size;
        let (eye, ray) = scene::camera_ray(self.angle_x, self.angle_y, [width, height], [x, y])?;
        let distance = -eye.y / ray.y;
        let point = eye + ray * distance;
        (distance > 0. && point.x.abs() <= 1.01 && point.z.abs() <= 1.01).then(|| {
            [
                (point.x * 0.5 + 0.5).clamp(0.0, 1.0),
                (point.z * 0.5 + 0.5).clamp(0.0, 1.0),
            ]
        })
    }
}

fn ray_sphere(origin: Vec3, ray: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let offset = origin - center;
    let b = offset.dot(ray);
    let discriminant = b * b - (offset.length_squared() - radius * radius);
    (discriminant > 0.0)
        .then(|| -b - discriminant.sqrt())
        .filter(|distance| *distance > 0.0)
}

impl Widget for WaterSurface {
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.shared.lock().unwrap().wake = wake;
    }

    fn set_gpu_context(&mut self, context: Option<GpuContext>) {
        self.engine = context
            .and_then(|context| context.downcast_ref::<ExternalGpuContext>().cloned())
            .map(Engine::new);
        self.revision = 0;
    }

    fn layout_changed(&mut self, geometry: WidgetGeometry) {
        self.geometry = geometry;
    }

    fn visibility_changed(&mut self, visible: bool) -> WidgetChanges {
        self.visible = visible;
        WidgetChanges::REDRAW
    }

    fn handle_event(&mut self, event: &UiEvent) -> WidgetEventResult {
        if let UiEvent::Key(key) = event {
            match (key.phase, key.key.as_str()) {
                (KeyPhase::Down, " " | "Space") if !key.repeat => self.paused = !self.paused,
                (KeyPhase::Down, "g" | "G") if !key.repeat => {
                    self.gravity_enabled = !self.gravity_enabled;
                }
                (KeyPhase::Down, "l" | "L") => self.light_tracks_camera = true,
                (KeyPhase::Up, "l" | "L") => self.light_tracks_camera = false,
                _ => return WidgetEventResult::IGNORED,
            }
            return WidgetEventResult::HANDLED;
        }
        let UiEvent::Pointer(pointer) = event else {
            return WidgetEventResult::IGNORED;
        };
        let position = [pointer.position.x, pointer.position.y];
        match pointer.phase {
            PointerPhase::Down if pointer.button == Some(PointerButton::Primary) => {
                let size = self.geometry.content_size;
                let Some((eye, ray)) =
                    scene::camera_ray(self.angle_x, self.angle_y, size, position)
                else {
                    return WidgetEventResult::HANDLED;
                };
                if let Some(distance) = ray_sphere(eye, ray, self.sphere, 0.25) {
                    let center_ray = scene::camera_ray(
                        self.angle_x,
                        self.angle_y,
                        size,
                        [f64::from(size[0]) * 0.5, f64::from(size[1]) * 0.5],
                    )
                    .map_or(ray, |(_, ray)| ray);
                    self.drag = DragMode::Sphere {
                        previous_hit: eye + ray * distance,
                        plane_normal: -center_ray,
                    };
                } else if let Some(drop) = self.normalized_pointer(position[0], position[1]) {
                    self.pending_drop = Some(drop);
                    self.drag = DragMode::Drop;
                } else {
                    self.drag = DragMode::Orbit { last: position };
                }
            }
            PointerPhase::Move => match self.drag {
                DragMode::Drop => {
                    if let Some(drop) = self.normalized_pointer(position[0], position[1]) {
                        self.pending_drop = Some(drop);
                    }
                }
                DragMode::Orbit { last } => {
                    self.angle_y -= (position[0] - last[0]) as f32;
                    self.angle_x =
                        (self.angle_x - (position[1] - last[1]) as f32).clamp(-89.999, 89.999);
                    self.drag = DragMode::Orbit { last: position };
                }
                DragMode::Sphere {
                    previous_hit,
                    plane_normal,
                } => {
                    if let Some((eye, ray)) = scene::camera_ray(
                        self.angle_x,
                        self.angle_y,
                        self.geometry.content_size,
                        position,
                    ) {
                        let denominator = plane_normal.dot(ray);
                        if denominator.abs() > 1.0e-5 {
                            let distance = -plane_normal.dot(eye - previous_hit) / denominator;
                            let next_hit = eye + ray * distance;
                            self.sphere += next_hit - previous_hit;
                            self.sphere.x = self.sphere.x.clamp(-0.75, 0.75);
                            self.sphere.y = self.sphere.y.clamp(-0.75, 10.0);
                            self.sphere.z = self.sphere.z.clamp(-0.75, 0.75);
                            self.drag = DragMode::Sphere {
                                previous_hit: next_hit,
                                plane_normal,
                            };
                        }
                    }
                }
                DragMode::None => {}
            },
            PointerPhase::Up | PointerPhase::Cancel => self.drag = DragMode::None,
            _ => {}
        }
        WidgetEventResult::HANDLED
    }

    fn animation_deadline(&self) -> Option<Instant> {
        (self.visible && self.engine.is_some() && !self.paused)
            .then(|| Instant::now() + Duration::from_millis(16))
    }

    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        let [logical_width, logical_height] = cx.size();
        let scale = cx.device_scale() as f32;
        let longest = (logical_width * scale).max(logical_height * scale).max(1.);
        let output_scale = (MAX_OUTPUT_EDGE / longest).min(1.);
        let size = [
            (logical_width * scale * output_scale).round().max(1.) as u32,
            (logical_height * scale * output_scale).round().max(1.) as u32,
        ];
        if !self.paused {
            if matches!(self.drag, DragMode::Sphere { .. }) {
                self.velocity = Vec3::ZERO;
            } else if self.gravity_enabled {
                let seconds = 1.0 / 60.0;
                let submerged = ((0.25 - self.sphere.y) / 0.5).clamp(0.0, 1.0);
                self.velocity += Vec3::new(0.0, -4.0, 0.0) * (seconds - 1.1 * seconds * submerged);
                let speed = self.velocity.length();
                if speed > 1.0e-5 {
                    self.velocity -= self.velocity / speed * submerged * seconds * speed * speed;
                }
                self.sphere += self.velocity * seconds;
                if self.sphere.y < -0.75 {
                    self.sphere.y = -0.75;
                    self.velocity.y = self.velocity.y.abs() * 0.7;
                }
            }
        }
        let mut disturbances = Vec::new();
        if !self.initialized {
            disturbances.extend(self.shared.lock().unwrap().commands.drain(..));
            self.initialized = true;
        } else if let Some(command) = self.shared.lock().unwrap().commands.pop_front() {
            disturbances.push(command);
        } else if let Some(position) = self.pending_drop.take() {
            disturbances.push(DropCommand {
                position,
                radius: 0.03,
                strength: 0.01,
            });
        }
        let light = if self.light_tracks_camera {
            scene::camera_ray(
                self.angle_x,
                self.angle_y,
                self.geometry.content_size,
                [
                    f64::from(logical_width) * 0.5,
                    f64::from(logical_height) * 0.5,
                ],
            )
            .map_or([2.0, 2.0, -1.0], |(_, ray)| ray.to_array())
        } else {
            [2.0, 2.0, -1.0]
        };
        let old_sphere = self.rendered_sphere;
        let new_sphere = self.sphere;
        let scene = scene::SceneState {
            angle_x: self.angle_x,
            angle_y: self.angle_y,
            sphere: [new_sphere.x, new_sphere.y, new_sphere.z, 0.25],
            light,
        };
        let Some(engine) = &mut self.engine else {
            return;
        };
        engine.render(
            size,
            &disturbances,
            old_sphere,
            new_sphere,
            !self.paused,
            scene,
        );
        self.rendered_sphere = new_sphere;
        self.revision = self.revision.wrapping_add(1);
        cx.draw_external_surface_in(
            ExternalSurface::new(
                self.surface_id,
                self.revision,
                size,
                "wgpu",
                engine.output.clone(),
            ),
            [0., 0., f64::from(logical_width), f64::from(logical_height)],
        );
    }
}

struct Engine {
    gpu: ExternalGpuContext,
    sim_pipeline: wgpu::ComputePipeline,
    sim_bind: [[wgpu::BindGroup; 2]; MAX_SIM_PASSES],
    sim_params: [wgpu::Buffer; MAX_SIM_PASSES],
    caustic: caustic::CausticRenderer,
    caustic_view: wgpu::TextureView,
    _caustic_texture: wgpu::Texture,
    scene: scene::SceneRenderer,
    current: usize,
    output: Arc<ExternalTexture>,
    output_size: [u32; 2],
}

impl Engine {
    fn new(gpu: ExternalGpuContext) -> Self {
        let device = &gpu.device;
        let state: [wgpu::Texture; 2] = std::array::from_fn(|index| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(if index == 0 {
                    "Plico water state A"
                } else {
                    "Plico water state B"
                }),
                size: wgpu::Extent3d {
                    width: SIMULATION_SIZE,
                    height: SIMULATION_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let state_views: [wgpu::TextureView; 2] =
            std::array::from_fn(|index| state[index].create_view(&Default::default()));
        let zeroes = vec![0_u8; (SIMULATION_SIZE * SIMULATION_SIZE * 8) as usize];
        for texture in &state {
            gpu.queue.write_texture(
                texture.as_image_copy(),
                &zeroes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIMULATION_SIZE * 8),
                    rows_per_image: Some(SIMULATION_SIZE),
                },
                texture.size(),
            );
        }
        let sim_module = device.create_shader_module(wgpu::include_wgsl!("simulation.wgsl"));
        let sim_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Plico water simulation"),
            layout: None,
            module: &sim_module,
            entry_point: Some("simulate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let initial_sim = SimParams {
            drop: [0.; 4],
            old_sphere: [-0.4, -0.75, 0.2, 0.25],
            new_sphere: [-0.4, -0.75, 0.2, 0.25],
            control: [0.; 4],
        };
        let sim_params = std::array::from_fn(|_index| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Plico water simulation pass params"),
                contents: bytemuck::bytes_of(&initial_sim),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });
        let sim_layout = sim_pipeline.get_bind_group_layout(0);
        let sim_bind = std::array::from_fn(|slot| {
            std::array::from_fn(|index| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Plico water simulation bind group"),
                    layout: &sim_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&state_views[index]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&state_views[1 - index]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: sim_params[slot].as_entire_binding(),
                        },
                    ],
                })
            })
        });
        let caustic_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico water projected caustics"),
            size: wgpu::Extent3d {
                width: CAUSTIC_SIZE,
                height: CAUSTIC_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let caustic_view = caustic_texture.create_view(&Default::default());
        let caustic = caustic::CausticRenderer::new(device, &state_views);
        let scene = scene::SceneRenderer::new(device, &gpu.queue, &state_views, &caustic_view);
        let output_size = [1, 1];
        let output = Self::create_output(device, output_size);
        Self {
            gpu,
            sim_pipeline,
            sim_bind,
            sim_params,
            caustic,
            caustic_view,
            _caustic_texture: caustic_texture,
            scene,
            current: 0,
            output,
            output_size,
        }
    }

    fn create_output(device: &wgpu::Device, size: [u32; 2]) -> Arc<ExternalTexture> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico water output"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Arc::new(ExternalTexture::new(texture, size))
    }

    fn render(
        &mut self,
        size: [u32; 2],
        disturbances: &[DropCommand],
        old_sphere: Vec3,
        new_sphere: Vec3,
        advance: bool,
        scene_state: scene::SceneState,
    ) {
        if size != self.output_size {
            self.output = Self::create_output(&self.gpu.device, size);
            self.output_size = size;
        }
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Plico water frame"),
            });
        let mut passes = Vec::with_capacity(MAX_SIM_PASSES);
        if old_sphere != new_sphere {
            passes.push(SimParams {
                drop: [0.; 4],
                old_sphere: [old_sphere.x, old_sphere.y, old_sphere.z, 0.25],
                new_sphere: [new_sphere.x, new_sphere.y, new_sphere.z, 0.25],
                control: [3., 0., 0., 0.],
            });
        }
        for drop in disturbances {
            passes.push(SimParams {
                drop: [
                    drop.position[0],
                    drop.position[1],
                    drop.radius,
                    drop.strength,
                ],
                old_sphere: [0.; 4],
                new_sphere: [0.; 4],
                control: [1., 0., 0., 0.],
            });
        }
        if advance {
            for _ in 0..2 {
                passes.push(SimParams {
                    drop: [0.; 4],
                    old_sphere: [0.; 4],
                    new_sphere: [0.; 4],
                    control: [0., 0., 0., 0.],
                });
            }
        }
        passes.push(SimParams {
            drop: [0.; 4],
            old_sphere: [0.; 4],
            new_sphere: [0.; 4],
            control: [2., 0., 0., 0.],
        });
        for (slot, params) in passes.into_iter().enumerate() {
            self.gpu
                .queue
                .write_buffer(&self.sim_params[slot], 0, bytemuck::bytes_of(&params));
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico water simulation step"),
                ..Default::default()
            });
            pass.set_pipeline(&self.sim_pipeline);
            pass.set_bind_group(0, &self.sim_bind[slot][self.current], &[]);
            pass.dispatch_workgroups(SIMULATION_SIZE.div_ceil(8), SIMULATION_SIZE.div_ceil(8), 1);
            std::mem::drop(pass);
            self.current = 1 - self.current;
        }
        self.caustic.encode(
            &self.gpu.queue,
            &mut encoder,
            &self.caustic_view,
            self.current,
            scene_state.sphere,
            scene_state.light,
        );
        let output_view = self.output.texture().create_view(&Default::default());
        self.scene.encode(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            scene::SceneFrame {
                output: &output_view,
                water_index: self.current,
                size,
                state: scene_state,
            },
        );
        self.gpu.queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::{KeyEvent, Modifiers};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn pointer_coordinates_are_normalized_and_clamped() {
        let mut water = WaterSurface::new();
        water.geometry.content_size = [400., 200.];
        let center = water
            .normalized_pointer(200., 100.)
            .expect("center ray must hit the water");
        assert!(
            center
                .into_iter()
                .all(|coordinate| (0. ..=1.).contains(&coordinate))
        );
        assert_eq!(water.normalized_pointer(-100., -100.), None);
    }

    #[test]
    fn scene_uses_displaced_mesh_and_separate_water_faces() {
        let shader = include_str!("scene.wgsl");
        assert!(shader.contains("fn water_vertex"));
        assert!(shader.contains("fn water_above"));
        assert!(shader.contains("fn water_below"));
    }

    #[test]
    fn controller_wakes_the_surface_and_clamps_drop_parameters() {
        let (controller, mut water) = WaterSurface::controlled();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = wakes.clone();
        water.set_wake_callback(Arc::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        }));

        controller.drop_at([-2.0, 3.0], 1.0, -2.0);

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        let mut shared = water.shared.lock().unwrap();
        let command = shared.commands.pop_back().unwrap();
        assert_eq!(command.position, [0.0, 1.0]);
        assert_eq!(command.radius, 0.25);
        assert_eq!(command.strength, -0.1);
    }

    #[test]
    fn initial_state_matches_the_original_twenty_alternating_drops() {
        let drops = initial_drops(1);
        assert_eq!(drops.len(), 20);
        for (index, drop) in drops.iter().enumerate() {
            assert!(
                drop.position
                    .into_iter()
                    .all(|value| (0.0..=1.0).contains(&value))
            );
            assert_eq!(drop.strength, if index & 1 == 0 { -0.01 } else { 0.01 });
        }
    }

    #[test]
    fn original_keyboard_controls_toggle_pause_gravity_and_light() {
        let mut water = WaterSurface::new();
        let key = |phase, name| UiEvent::Key(KeyEvent::logical(phase, name, Modifiers::empty()));

        assert!(
            water
                .handle_event(&key(KeyPhase::Down, "Space"))
                .is_handled()
        );
        assert!(water.paused);
        assert!(water.handle_event(&key(KeyPhase::Down, "G")).is_handled());
        assert!(water.gravity_enabled);
        assert!(water.handle_event(&key(KeyPhase::Down, "L")).is_handled());
        assert!(water.light_tracks_camera);
        assert!(water.handle_event(&key(KeyPhase::Up, "L")).is_handled());
        assert!(!water.light_tracks_camera);
    }
}
