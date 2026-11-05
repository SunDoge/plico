//! Embed Plico UI in a Bevy window without a second window loop or GPU device.
//!
//! Add [`PlicoPlugin`] after Bevy's render/window plugins. Application UI is an
//! ordinary [`plico::View`]; Bevy systems can access it through
//! `NonSendMut<PlicoUi<MyView>>`. Only GPU texture handles cross into Bevy's
//! render world. UI state, native text caches and short-lived borrows stay on
//! the main thread.
mod composite;
mod input;

use std::{marker::PhantomData, sync::Arc};

use bevy::{
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp,
        renderer::{RenderAdapter, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
    },
    window::{CursorIcon, PrimaryWindow, SystemCursorIcon},
};
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{NativeView, Source};

pub use input::PlicoInput;

/// Order gameplay input after `Input`. UI preparation happens during Bevy's
/// synchronous extraction, after the main world has completed its updates.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlicoSystems {
    Input,
}

/// Errors remain visible in [`PlicoUi::last_error`] and are logged by the plugin.
#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("Plico GPU rendering failed: {source}"))]
    Render { source: plico_wgpu::Error },
    #[snafu(display("Plico UI declaration failed: {source}"))]
    Declaration { source: plico_runtime::SourceError },
    #[snafu(display("Plico clock failed: {source}"))]
    Clock { source: plico::ClockError },
}

/// Plico's main-thread UI state. It is deliberately a non-send Bevy resource.
///
/// Use [`Self::update`] when a Bevy resource changes. Input-driven changes inside
/// `View::show` already invalidate themselves; no manual rebuild is needed.
pub struct PlicoUi<V: plico::View + 'static> {
    source: Source<NativeView<V>>,
    clock: plico::VirtualClock,
    text: TextContext,
    renderer: Option<plico_wgpu::Renderer>,
    textures: Vec<Arc<OutputTexture>>,
    size: [u32; 2],
    scale: f64,
    scene: Scene,
    input: input::InputState,
    error: Option<Error>,
    /// Number of complete UI texture updates (not final scene composites).
    pub texture_updates: u64,
}
impl<V: plico::View + 'static> PlicoUi<V> {
    pub fn new(view: V) -> Self {
        let clock = plico::VirtualClock::default();
        Self {
            source: Source::with_clock(NativeView(view), clock.clone())
                .with_background(peniko::Color::TRANSPARENT),
            clock,
            text: TextContext::new(),
            renderer: None,
            textures: Vec::new(),
            size: [0; 2],
            scale: 0.,
            scene: Scene::new(),
            input: Default::default(),
            error: None,
            texture_updates: 0,
        }
    }
    pub fn view(&self) -> &V {
        &self.source.view.0
    }
    pub fn update<R>(&mut self, update: impl FnOnce(&mut V) -> R) -> R {
        self.source.update(|view| update(&mut view.0))
    }
    pub fn request_rebuild(&mut self) {
        self.source.request_rebuild();
    }
    pub fn last_error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
    /// Native protocol injection, also useful for deterministic application tests.
    pub fn send_event(&mut self, event: plico_renderer::UiEvent) -> plico_renderer::EventResponse {
        self.source.prepare_for_event(&mut self.text);
        self.source.handle_event(event)
    }
    /// Inspect retained layout and batching counters without a GPU readback.
    pub fn source(&self) -> &Source<NativeView<V>> {
        &self.source
    }
}
impl<V: plico::View + Default + 'static> Default for PlicoUi<V> {
    fn default() -> Self {
        Self::new(V::default())
    }
}

/// Overlay one Plico view on the primary window. Bevy keeps control of cameras,
/// window creation, presentation and its normal game update loop.
///
/// Insert `PlicoUi::new(view)` before adding this plugin to supply initial state.
/// A camera targeting the primary window is required. Only one Plico UI plugin
/// is supported per Bevy App; secondary windows/world-space UI are not wired yet.
pub struct PlicoPlugin<V>(PhantomData<fn() -> V>);
impl<V> Default for PlicoPlugin<V> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<V: plico::View + Default + 'static> Plugin for PlicoPlugin<V> {
    fn build(&self, app: &mut App) {
        assert!(
            !app.world().contains_resource::<PlicoOutput>(),
            "only one PlicoPlugin may own a window overlay"
        );
        app.init_non_send::<PlicoUi<V>>()
            .init_resource::<PlicoOutput>()
            .init_resource::<PlicoInput>()
            .add_message::<PlicoHostAction>()
            .add_message::<PlicoClipboardRequest>()
            .add_systems(
                PreUpdate,
                input::handle_input::<V>
                    .in_set(PlicoSystems::Input)
                    .after(bevy::input::InputSystems),
            );
        #[cfg(feature = "clipboard")]
        app.init_resource::<bevy::clipboard::Clipboard>()
            .add_systems(
                PreUpdate,
                input::handle_clipboard::<V>
                    .in_set(PlicoSystems::Input)
                    .after(input::handle_input::<V>),
            );
        let render_app = app
            .get_sub_app_mut(RenderApp)
            .expect("add PlicoPlugin after Bevy's RenderPlugin");
        render_app
            .init_resource::<composite::Compositor>()
            .add_systems(ExtractSchedule, extract_output::<V>)
            // Bevy has submitted camera draws, but hasn't captured/presented the
            // window yet. Thus our overlay is included in Bevy screenshots too.
            .add_systems(
                RenderGraph,
                composite::composite.in_set(RenderGraphSystems::Finish),
            );
    }
}

/// OS actions emitted by widgets. The embedding app decides how to execute them.
#[derive(Message)]
pub struct PlicoHostAction(pub plico_renderer::HostAction);
/// Clipboard requests from text controls. Respond to reads with
/// `ui.send_event(UiEvent::Paste(text))`; writes contain the selected text.
#[derive(Message)]
pub struct PlicoClipboardRequest(pub plico_renderer::ClipboardRequest);

#[derive(Clone)]
struct OutputTexture {
    view: wgpu::TextureView,
    window: Entity,
    size: [u32; 2],
}
#[derive(Resource, Default, Clone)]
struct PlicoOutput(Option<Arc<OutputTexture>>);
fn extract_output<V: plico::View + 'static>(world: &mut World) {
    // The previous render App has finished before extraction starts. GPU work
    // here cannot race Bevy's Surface::configure on its render thread. Source,
    // renderer and native text caches still run on their owning main thread.
    world.resource_scope(|world, mut main: Mut<MainWorld>| {
        if let Err(error) = main.run_system_cached(prepare::<V>) {
            tracing::error!(%error, "Plico Bevy preparation system failed");
            world.insert_resource(PlicoOutput::default());
            return;
        }
        world.insert_resource(main.resource::<PlicoOutput>().clone());
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters describe independent host resources"
)]
fn prepare<V: plico::View + 'static>(
    mut ui: NonSendMut<PlicoUi<V>>,
    mut commands: Commands,
    time: Res<Time>,
    mut windows: Query<(Entity, &mut Window, Option<&CursorIcon>), With<PrimaryWindow>>,
    device: Option<Res<RenderDevice>>,
    queue: Option<Res<RenderQueue>>,
    adapter: Option<Res<RenderAdapter>>,
    mut output: ResMut<PlicoOutput>,
    mut host_actions: MessageWriter<PlicoHostAction>,
) {
    let Ok((entity, mut window, current_cursor)) = windows.single_mut() else {
        output.0 = None;
        return;
    };
    let (Some(device), Some(queue), Some(adapter)) = (device, queue, adapter) else {
        return;
    };
    let ui = &mut *ui;
    let result = (|| {
        ui.clock
            .advance(time.delta())
            .map_err(|source| Error::Clock { source })?;
        if ui.renderer.is_none() {
            ui.renderer = Some(
                pollster::block_on(plico_wgpu::Renderer::from_device(
                    adapter.as_ref().clone().into_inner(),
                    device.wgpu_device().clone(),
                    queue.as_ref().clone().into_inner(),
                ))
                .map_err(|source| Error::Render { source })?,
            );
        }
        let size = [window.physical_width(), window.physical_height()];
        if size.contains(&0) {
            output.0 = None;
            return Ok(());
        }
        let scale = f64::from(window.scale_factor());
        let renderer = ui.renderer.as_mut().unwrap();
        if ui.scale != scale {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            renderer
                .configure_text(&mut ui.text, scale)
                .map_err(|source| Error::Render { source })?;
            ui.source.set_device_scale(scale);
            ui.scale = scale;
        }
        let resized = size != ui.size;
        ui.source.poll_async();
        let previous = ui.scene.clone();
        ui.source.render_retained_frame(
            &mut ui.scene,
            &mut ui.text,
            (
                window.width().round() as u32,
                window.height().round() as u32,
            ),
            scale,
        );
        if let Some(source) = ui.source.last_error.take() {
            return Err(Error::Declaration { source });
        }
        if resized
            || output.0.as_ref().is_none_or(|frame| frame.window != entity)
            || !ui.scene.shares_commands_with(&previous)
        {
            // Main and render worlds can run concurrently. Never overwrite a
            // texture still held by an extracted frame. Released slots are safe
            // to reuse: Bevy has already submitted their last GPU read.
            ui.textures
                .retain(|slot| slot.size == size && slot.window == entity);
            let slot =
                if let Some(slot) = ui.textures.iter().find(|slot| Arc::strong_count(slot) == 1) {
                    slot.clone()
                } else {
                    let texture = device
                        .wgpu_device()
                        .create_texture(&wgpu::TextureDescriptor {
                            label: Some("Plico Bevy UI"),
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
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        });
                    let slot = Arc::new(OutputTexture {
                        view: texture.create_view(&Default::default()),
                        window: entity,
                        size,
                    });
                    ui.textures.push(slot.clone());
                    slot
                };
            renderer
                .render_scene_to_view(
                    &ui.scene,
                    size,
                    [0.; 4],
                    false,
                    &slot.view,
                    wgpu::TextureFormat::Rgba8Unorm,
                )
                .map_err(|source| Error::Render { source })?;
            ui.texture_updates += 1;
            ui.size = size;
            output.0 = Some(slot);
        }
        let cursor = match ui.source.pointer_cursor() {
            plico_renderer::style::CursorStyle::Default => SystemCursorIcon::Default,
            plico_renderer::style::CursorStyle::Pointer => SystemCursorIcon::Pointer,
            plico_renderer::style::CursorStyle::Text => SystemCursorIcon::Text,
            plico_renderer::style::CursorStyle::Crosshair => SystemCursorIcon::Crosshair,
            plico_renderer::style::CursorStyle::Move => SystemCursorIcon::Move,
            plico_renderer::style::CursorStyle::Wait => SystemCursorIcon::Wait,
            plico_renderer::style::CursorStyle::NotAllowed => SystemCursorIcon::NotAllowed,
            plico_renderer::style::CursorStyle::EwResize => SystemCursorIcon::EwResize,
            plico_renderer::style::CursorStyle::NsResize => SystemCursorIcon::NsResize,
        };
        let cursor = CursorIcon::System(cursor);
        if current_cursor != Some(&cursor) {
            commands.entity(entity).insert(cursor);
        }
        let ime = ui.source.ime_state();
        window.ime_enabled = ime.is_some();
        if let Some(ime) = ime {
            window.ime_position = Vec2::new(
                ime.cursor_area[0] as f32,
                (ime.cursor_area[1] + ime.cursor_area[3]) as f32,
            );
        }
        while let Some(action) = ui.source.take_host_action() {
            host_actions.write(PlicoHostAction(action));
        }
        Ok(())
    })();
    match result {
        Ok(()) => ui.error = None,
        Err(error) => {
            tracing::error!(%error, "Plico Bevy frame failed");
            ui.error = Some(error);
            // Never display a stale-sized texture after a resize failure.
            output.0 = None;
        }
    }
}

#[cfg(test)]
mod tests;
