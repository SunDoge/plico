#[path = "../ui.rs"]
mod ui;

use plico::prelude::*;
#[cfg(any(feature = "wasm-host", feature = "frame-rate"))]
use std::time::Instant;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
#[cfg(all(feature = "wasm-host", not(feature = "tree")))]
use ui::ArchivedReaderUi;
use ui::ReaderState;
#[cfg(feature = "tree")]
use ui::tree;

#[cfg(feature = "wasm-host")]
fn wasm_engine() -> plico_wasm::Engine {
    let mut config = plico_wasm::Config::new();
    if std::env::var_os("PLICO_WASM_PROBE_FAST_JIT").is_some()
        || std::env::var_os("PLICO_WASM_MODULE").is_some()
    {
        config.cranelift_opt_level(plico_wasm::OptLevel::None);
    }
    plico_wasm::Engine::new(&config).unwrap()
}

#[cfg(feature = "wasm-host")]
struct Guest {
    module: plico_wasm::Module,
    #[cfg(feature = "tree")]
    frame: Option<tree::Output>,
}

#[cfg(feature = "wasm-host")]
impl Guest {
    fn load(engine: &plico_wasm::Engine, path: &std::path::Path) -> Self {
        Self::try_load(engine, path).expect("load initial UI module")
    }

    fn try_load(engine: &plico_wasm::Engine, path: &std::path::Path) -> plico_wasm::Result<Self> {
        Ok(Self {
            module: plico_wasm::Module::load(engine, path, ui::ABI, ui::BUFFER_CAPACITY)?,
            #[cfg(feature = "tree")]
            frame: None,
        })
    }

    #[cfg(not(feature = "tree"))]
    fn update(&mut self, state: ReaderState) -> (&ArchivedReaderUi, usize, usize) {
        let (bytes, input) = self.module.call(&state).unwrap();
        (
            rkyv::access::<ArchivedReaderUi, rkyv::rancor::Error>(bytes).unwrap(),
            input,
            bytes.len(),
        )
    }

    #[cfg(feature = "tree")]
    fn frame(&mut self, request: tree::Input) -> (&mut tree::Output, usize, usize) {
        let (bytes, input) = self.module.call(&request).expect("reader UI update");
        (decode_frame(&mut self.frame, bytes), input, bytes.len())
    }
}

#[cfg(all(feature = "wasm-host", feature = "tree"))]
fn decode_frame<'a>(frame: &'a mut Option<tree::Output>, bytes: &[u8]) -> &'a mut tree::Output {
    #[cfg(feature = "tree-patch")]
    let (is_patch, payload) = match bytes.split_first() {
        Some((0, payload)) => (false, payload),
        Some((1, payload)) => (true, payload),
        _ => panic!("invalid reader update tag"),
    };
    #[cfg(not(feature = "tree-patch"))]
    let payload = bytes;
    #[cfg(feature = "tree-patch")]
    if is_patch {
        let patch =
            rkyv::access::<tree::patch::ArchivedPatch, rkyv::rancor::Error>(payload).unwrap();
        tree::patch::apply(frame, patch);
        return frame.as_mut().unwrap();
    }
    // Keep the baseline across future calls, which reuse the same guest buffer.
    *frame = Some(rkyv::from_bytes::<tree::Output, rkyv::rancor::Error>(payload).unwrap());
    frame.as_mut().unwrap()
}

#[cfg(all(feature = "reader-service", feature = "wasm-host"))]
use plico_wasm::Reloads;

struct Reader {
    image: plico::ImageAsset,
    progress: Arc<AtomicU32>,
    job: Option<std::thread::JoinHandle<()>>,
    #[cfg(feature = "reader-service")]
    worker: ui::service::Worker,
    #[cfg(all(feature = "reader-service", feature = "wasm-host"))]
    reloads: Option<Reloads<Guest>>,
    #[cfg(all(feature = "reader-service", not(feature = "wasm-host")))]
    services: ui::service::ReaderServices,
    last: Option<(u32, u32)>,
    #[cfg(feature = "frame-rate")]
    paced: Option<(u32, u32)>,
    #[cfg(feature = "tree")]
    selected: bool,
    #[cfg(all(feature = "tree", feature = "wasm-host"))]
    clicks: Vec<Vec<plico::PathKey>>,
    #[cfg(feature = "wasm-host")]
    module: PathBuf,
    #[cfg(all(feature = "wasm-host", not(feature = "reader-service")))]
    loaded: std::time::SystemTime,
    #[cfg(feature = "wasm-host")]
    engine: plico_wasm::Engine,
    #[cfg(feature = "wasm-host")]
    guest: Guest,
}

fn state(progress: u32, regions: u32, job_id: u32) -> ReaderState {
    ReaderState {
        page: 7,
        image: 17,
        zoom: 140,
        job_id,
        progress,
        regions,
    }
}

impl plico_runtime::HostView for Reader {
    fn theme(&self) -> Theme {
        Theme::light()
    }

    #[cfg(feature = "reader-service")]
    fn poll_updates(&mut self) -> bool {
        let changed = self.worker.poll();
        #[cfg(feature = "wasm-host")]
        let mut changed = changed;
        #[cfg(feature = "wasm-host")]
        if let Some(candidate) = self.reloads.as_ref().and_then(Reloads::take_candidate) {
            self.guest = candidate;
            changed = true;
        }
        changed
    }

    #[cfg(feature = "reader-service")]
    fn set_wake_callback(&mut self, wake: plico_dev::services::Wake) {
        self.worker.set_wake_callback(wake.clone());
        #[cfg(feature = "wasm-host")]
        {
            let engine = self.engine.clone();
            self.reloads = Some(
                Reloads::watch(
                    self.module.clone(),
                    move |path| {
                        println!("LOAD START module");
                        let start = Instant::now();
                        match Guest::try_load(&engine, path) {
                            Ok(candidate) => {
                                println!("LOAD {:.3}", start.elapsed().as_secs_f64() * 1000.);
                                Some(candidate)
                            }
                            Err(error) => {
                                eprintln!("LOAD ERROR {error:#}");
                                None
                            }
                        }
                    },
                    move || wake(),
                )
                .expect("watch reader UI module"),
            );
        }
    }

    fn show(&mut self, cx: &mut UiContext<'_>) {
        let progress = self.progress.load(Ordering::Relaxed);
        #[cfg(feature = "frame-rate")]
        let (progress, regions) = self.paced.unwrap_or((progress, 16));
        #[cfg(not(feature = "frame-rate"))]
        let regions = 16;
        let state = state(progress, regions, std::process::id());
        #[cfg(all(feature = "wasm-host", not(feature = "reader-service")))]
        {
            let modified = self.module.metadata().unwrap().modified().unwrap();
            if modified != self.loaded {
                let start = Instant::now();
                let candidate = Guest::load(&self.engine, &self.module);
                println!("LOAD {:.3}", start.elapsed().as_secs_f64() * 1000.);
                self.guest = candidate;
                self.loaded = modified;
            }
        }
        #[cfg(not(feature = "tree"))]
        self.show_model(cx, state);
        #[cfg(feature = "tree")]
        self.show_tree(cx, state);
        #[cfg(not(feature = "reader-service"))]
        cx.request_rebuild_at(cx.time().now + Duration::from_millis(8));
    }
}

#[cfg(not(feature = "tree"))]
impl Reader {
    fn show_model(&mut self, cx: &mut UiContext<'_>, state: ReaderState) {
        let job_id = self
            .job
            .as_ref()
            .map(|job| job.thread().id())
            .unwrap_or_else(|| std::thread::current().id());
        let progress = state.progress;
        #[cfg(feature = "wasm-host")]
        let model = self.guest.update(state).0;
        #[cfg(not(feature = "wasm-host"))]
        let model = ui::update(state);
        #[cfg(feature = "wasm-host")]
        let revision = model.revision.to_native();
        #[cfg(not(feature = "wasm-host"))]
        let revision = model.revision;
        if self.last != Some((revision, progress)) {
            println!(
                "READY {} {} {} {} {} {job_id:?} {progress}",
                model.revision,
                model.state.page,
                model.state.zoom,
                model.state.image,
                model.state.job_id
            );
            self.last = Some((revision, progress));
        }
        assert_eq!(model.state.image, 17);
        let marker_color = if revision % 2 == 0 {
            0xcc2233
        } else {
            0x22cc55
        };
        let _ = column(cx).padding(12).gap(12).children(|cx| {
            let _ = row(cx).gap(12).align(Align::Center).children(|cx| {
                column(cx)
                    .width(32.)
                    .height(32.)
                    .background(Color::Literal(marker_color))
                    .end();
                text(cx, model.title.as_str());
            });
            let _ = column(cx)
                .position(Position::Relative)
                .width(376.)
                .height(480.)
                .children(|cx| {
                    cx.memo(state.image, |cx| {
                        image(cx, &self.image).size(376., 480.);
                    });
                    for overlay in model.overlays.iter() {
                        #[cfg(feature = "wasm-host")]
                        let (x, y) = (overlay.x.to_native(), overlay.y.to_native());
                        #[cfg(not(feature = "wasm-host"))]
                        let (x, y) = (overlay.x, overlay.y);
                        column(cx)
                            .absolute(f32::from(x), f32::from(y))
                            .width(24.)
                            .height(14.)
                            .border(1., Color::Literal(0x2c83e4))
                            .end();
                    }
                });
        });
    }
}

#[cfg(feature = "tree")]
impl Reader {
    fn show_tree(&mut self, cx: &mut UiContext<'_>, state: ReaderState) {
        let previous_selected = self.selected;
        #[cfg(feature = "wasm-host")]
        let frame = {
            let request = tree::Input {
                reader: state,
                selected: self.selected,
                clicks: std::mem::take(&mut self.clicks),
                #[cfg(feature = "reader-service")]
                analysis: self.worker.snapshot(),
            };
            let (frame, _, _) = self.guest.frame(request);
            plico_dev::validate_nodes(&frame.nodes).unwrap();
            self.selected = frame.selected;
            frame
        };
        #[cfg(feature = "wasm-host")]
        let revision = frame.revision;
        #[cfg(not(feature = "wasm-host"))]
        let revision = tree::TREE_REVISION;
        #[cfg(all(feature = "reader-service", not(feature = "wasm-host")))]
        if self.services.snapshot() != self.worker.status() {
            self.services.replace_snapshot(self.worker.snapshot());
        }
        column(cx)
            .padding(12)
            .gap(12)
            .position(Position::Relative)
            .children(|cx| {
                cx.memo(state.image, |cx| {
                    let image_top = if cfg!(feature = "reader-service") {
                        100.
                    } else {
                        56.
                    };
                    column(cx).absolute(12., image_top).children(|cx| {
                        image(cx, &self.image).size(376., 480.);
                    });
                });
                #[cfg(feature = "wasm-host")]
                {
                    tree::replay(cx, &frame.nodes, &mut Vec::new(), &mut self.clicks);
                    if !self.clicks.is_empty() {
                        cx.request_rebuild();
                    }
                }
                #[cfg(not(feature = "wasm-host"))]
                tree::show(
                    cx,
                    state,
                    &mut self.selected,
                    #[cfg(feature = "reader-service")]
                    &mut self.services,
                );
            });
        #[cfg(feature = "reader-service")]
        {
            #[cfg(feature = "wasm-host")]
            let commands = std::mem::take(&mut frame.commands);
            #[cfg(not(feature = "wasm-host"))]
            let commands = self.services.take_commands();
            if self.worker.dispatch(commands) {
                cx.request_rebuild();
            }
        }
        #[cfg(feature = "frame-rate")]
        if self.paced.is_some() {
            return;
        }
        if self.last != Some((revision, state.progress)) || previous_selected != self.selected {
            println!(
                "READY {revision} {} {} {} {} {:?} {} {}",
                state.page,
                state.zoom,
                state.image,
                state.job_id,
                self.job
                    .as_ref()
                    .map(|job| job.thread().id())
                    .unwrap_or_else(|| std::thread::current().id()),
                state.progress,
                self.selected
            );
            self.last = Some((revision, state.progress));
        }
    }
}

#[cfg(all(feature = "wasm-host", not(feature = "tree")))]
fn measure(module: &std::path::Path) {
    use std::hint::black_box;
    let engine = wasm_engine();
    let start = Instant::now();
    let mut guest = Guest::load(&engine, module);
    println!(
        "module_load_ms {:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
    println!("regions,mode,median_us,input_bytes,output_bytes");
    for regions in [0, 16, 128, 512] {
        let mut native = Vec::new();
        let mut wasm = Vec::new();
        let count = 10_000;
        for _ in 0..7 {
            let start = Instant::now();
            for i in 0..count {
                black_box(ui::update(black_box(state(i % 100, regions, 17))));
            }
            native.push(start.elapsed().as_secs_f64() * 1e6 / f64::from(count));
            let start = Instant::now();
            for i in 0..count {
                black_box(guest.update(black_box(state(i % 100, regions, 17))));
            }
            wasm.push(start.elapsed().as_secs_f64() * 1e6 / f64::from(count));
        }
        native.sort_by(f64::total_cmp);
        wasm.sort_by(f64::total_cmp);
        let state = state(50, regions, 17);
        let (actual, input, output) = guest.update(state);
        let expected = ui::update(state);
        assert_eq!(actual.title, expected.title);
        assert!(
            actual
                .overlays
                .iter()
                .zip(&expected.overlays)
                .all(|(left, right)| left.x == right.x && left.y == right.y)
        );
        assert_eq!(actual.overlays.len(), expected.overlays.len());
        println!("{regions},native,{:.3},0,0", native[3]);
        println!("{regions},wasm,{:.3},{input},{output}", wasm[3]);
    }
}

#[cfg(all(feature = "wasm-host", feature = "tree"))]
fn measure(module: &std::path::Path) {
    use plico::Engine;
    use std::hint::black_box;
    let engine = wasm_engine();
    let start = Instant::now();
    let mut guest = Guest::load(&engine, module);
    println!(
        "module_load_ms {:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
    println!("regions,mode,median_us,input_bytes,output_bytes");
    for regions in [0, 16, 128, 512] {
        let mut native = vec![];
        let mut wasm = vec![];
        let mut direct = Engine::default();
        #[cfg(feature = "reader-service")]
        let mut service = ui::service::ReaderServices::default();
        let mut replayed = Engine::default();
        let count = 500;
        for _ in 0..7 {
            let start = Instant::now();
            for i in 0..count {
                tree::declare(
                    &mut direct,
                    state(i % 100, regions, 17),
                    &mut false,
                    #[cfg(feature = "reader-service")]
                    &mut service,
                );
                black_box(&direct);
            }
            native.push(start.elapsed().as_secs_f64() * 1e6 / f64::from(count));
            let start = Instant::now();
            for i in 0..count {
                let request = tree::Input {
                    reader: state(i % 100, regions, 17),
                    selected: false,
                    clicks: vec![],
                    #[cfg(feature = "reader-service")]
                    analysis: Default::default(),
                };
                let (frame, _, _) = guest.frame(request);
                plico_dev::validate_nodes(&frame.nodes).unwrap();
                replayed
                    .frame_declarations(|cx| {
                        tree::replay(cx, &frame.nodes, &mut vec![], &mut vec![])
                    })
                    .unwrap();
                black_box(&replayed);
            }
            wasm.push(start.elapsed().as_secs_f64() * 1e6 / f64::from(count));
        }
        native.sort_by(f64::total_cmp);
        wasm.sort_by(f64::total_cmp);
        let reader = state(50, regions, 17);
        let (actual, input, output) = guest.frame(tree::Input {
            reader,
            selected: false,
            clicks: vec![],
            #[cfg(feature = "reader-service")]
            analysis: Default::default(),
        });
        tree::declare(
            &mut direct,
            reader,
            &mut false,
            #[cfg(feature = "reader-service")]
            &mut service,
        );
        assert_eq!(actual.nodes, direct.snapshot());
        let mut path = Vec::new();
        assert!(button_path(&actual.nodes, &mut path));
        let response = guest
            .frame(tree::Input {
                reader,
                selected: false,
                clicks: vec![path],
                #[cfg(feature = "reader-service")]
                analysis: Default::default(),
            })
            .0;
        assert!(response.selected);
        println!("{regions},native,{:.3},0,0", native[3]);
        println!("{regions},wasm,{:.3},{input},{output}", wasm[3]);
    }
}

#[cfg(feature = "tree")]
fn button_path(nodes: &[plico::ViewNode], path: &mut Vec<plico::PathKey>) -> bool {
    for node in nodes {
        path.push(node.key.clone());
        if node.kind == plico::Kind::ButtonBase
            || node.kind == plico::Kind::Button
            || button_path(&node.children, path)
        {
            return true;
        }
        path.pop();
    }
    false
}

#[cfg(all(feature = "wasm-host", feature = "tree"))]
fn browser_fixtures(module: &std::path::Path, directory: &std::path::Path) {
    let mut guest =
        plico_wasm::Module::load(&wasm_engine(), module, ui::ABI, ui::BUFFER_CAPACITY).unwrap();
    let mut retained = None;
    let mut button = Vec::new();
    let mut selected = false;
    for (index, click) in [false, true, false, true].into_iter().enumerate() {
        let request = tree::Input {
            reader: state(50, 16, 17),
            selected,
            clicks: if click { vec![button.clone()] } else { vec![] },
            #[cfg(feature = "reader-service")]
            analysis: Default::default(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&request).unwrap();
        let (reply, _) = guest.call(&request).unwrap();
        let frame = decode_frame(&mut retained, reply);
        plico_dev::validate_nodes(&frame.nodes).unwrap();
        assert_eq!(frame.selected, selected ^ click);
        selected = frame.selected;
        button.clear();
        assert!(button_path(&frame.nodes, &mut button));
        std::fs::write(directory.join(format!("{index}.request")), bytes).unwrap();
        std::fs::write(directory.join(format!("{index}.reply")), reply).unwrap();
    }
    println!("{} {}", ui::ABI, ui::BUFFER_CAPACITY);
}

#[cfg(feature = "tree")]
fn check_interaction(
    marked_label: &str,
    mut frame: impl FnMut(bool, Vec<Vec<plico::PathKey>>) -> (bool, Vec<plico::ViewNode>),
) {
    fn has_text(nodes: &[plico::ViewNode], label: &str) -> bool {
        nodes
            .iter()
            .any(|node| node.text == label || has_text(&node.children, label))
    }
    let mut selected = false;
    let mut path = Vec::new();
    for click in [false, true, false, true, false] {
        let (next, nodes) = frame(selected, if click { vec![path.clone()] } else { vec![] });
        assert_eq!(next, selected ^ click);
        selected = next;
        if !click {
            let label = if selected { marked_label } else { "Mark" };
            assert!(has_text(&nodes, label), "missing reader caption {label:?}");
        }
        path.clear();
        assert!(button_path(&nodes, &mut path));
    }
}

#[cfg(all(feature = "tree", not(feature = "wasm-host")))]
fn check_native(label: &str) {
    let reader = state(50, 16, 17);
    let mut engine = plico::Engine::default();
    #[cfg(feature = "reader-service")]
    let mut services = Default::default();
    check_interaction(label, |mut selected, clicks| {
        for path in clicks {
            engine.queue_click_path(path);
        }
        tree::declare(
            &mut engine,
            reader,
            &mut selected,
            #[cfg(feature = "reader-service")]
            &mut services,
        );
        (selected, engine.snapshot())
    });
}

#[cfg(all(test, feature = "tree", not(feature = "wasm-host")))]
#[test]
fn mark_and_unmark() {
    check_native(&std::env::var("PLICO_EXPECT_MARKED").unwrap_or_else(|_| "Marked".into()));
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_ansi(false)
            .init();
    }

    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().expect("IMAGE or MEASURE"));
    #[cfg(feature = "wasm-host")]
    let module = std::path::absolute(
        std::env::var_os("PLICO_WASM_MODULE")
            .or_else(|| args.next())
            .expect("WASM module"),
    )
    .unwrap();
    #[cfg(feature = "tree")]
    if path.as_os_str() == "CHECK" {
        let label = args
            .next()
            .expect("expected marked caption")
            .into_string()
            .unwrap();
        #[cfg(feature = "wasm-host")]
        {
            let reader = state(50, 16, 17);
            let mut guest = Guest::load(&wasm_engine(), &module);
            check_interaction(&label, |selected, clicks| {
                let (frame, _, _) = guest.frame(tree::Input {
                    reader,
                    selected,
                    clicks,
                    #[cfg(feature = "reader-service")]
                    analysis: Default::default(),
                });
                (frame.selected, frame.nodes.clone())
            });
        }
        #[cfg(not(feature = "wasm-host"))]
        check_native(&label);
        return;
    }
    #[cfg(feature = "wasm-host")]
    if path.as_os_str() == "MEASURE" {
        return measure(&module);
    }
    #[cfg(all(feature = "wasm-host", feature = "tree"))]
    if path.as_os_str() == "BROWSER" {
        return browser_fixtures(
            &module,
            &PathBuf::from(args.next().expect("fixture directory")),
        );
    }
    let bytes = std::fs::read(path).unwrap();
    let image = plico::ImageAsset::from_bytes(&bytes);
    #[cfg(feature = "reader-service")]
    let worker = ui::service::Worker::new(17, image.clone());
    let progress = Arc::new(AtomicU32::new(0));
    let job = (!cfg!(feature = "reader-service")).then(|| {
        std::thread::spawn({
            let progress = progress.clone();
            move || loop {
                std::thread::sleep(Duration::from_millis(250));
                progress.fetch_add(1, Ordering::Relaxed);
            }
        })
    });
    println!(
        "RESOURCES image=17 bytes={} job={:?}",
        image.data().len(),
        job.as_ref().map(|job| job.thread().id())
    );
    #[cfg(feature = "wasm-host")]
    let engine = wasm_engine();
    let reader = Reader {
        image,
        progress,
        job,
        #[cfg(feature = "reader-service")]
        worker,
        #[cfg(all(feature = "reader-service", feature = "wasm-host"))]
        reloads: None,
        #[cfg(all(feature = "reader-service", not(feature = "wasm-host")))]
        services: Default::default(),
        last: None,
        #[cfg(feature = "frame-rate")]
        paced: None,
        #[cfg(feature = "tree")]
        selected: false,
        #[cfg(all(feature = "tree", feature = "wasm-host"))]
        clicks: Vec::new(),
        #[cfg(all(feature = "wasm-host", not(feature = "reader-service")))]
        loaded: module.metadata().unwrap().modified().unwrap(),
        #[cfg(feature = "wasm-host")]
        guest: Guest::load(&engine, &module),
        #[cfg(feature = "wasm-host")]
        engine,
        #[cfg(feature = "wasm-host")]
        module,
    };
    #[cfg(feature = "frame-rate")]
    if std::env::var_os("PLICO_WASM_PROBE_120FPS").is_some() {
        return frame_rate(reader);
    }
    plico_desktop::run_host_view_with_options(
        reader,
        plico_desktop::WindowOptions::new()
            .title("Plico WASM UI experiment")
            .initial_inner_size(420, 570),
    )
    .unwrap();
}

#[cfg(feature = "frame-rate")]
fn frame_rate(reader: Reader) {
    use plico_renderer::{Scene, TextContext};
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    let gpu = renderer.external_gpu_context();
    let size = [420, 570];
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let surface = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("120 FPS probe"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let destination = surface.create_view(&Default::default());
    let mut source = plico_runtime::Source::new(reader);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    let damage = std::env::var_os("PLICO_WASM_PROBE_FULL_REDRAW").is_none();
    let mut frame = |tick, regions| {
        let start = Instant::now();
        source.update(|reader| reader.paced = Some((tick, regions)));
        source
            .try_build_and_paint(&mut scene, &mut text, size[0], size[1], 1.)
            .unwrap();
        assert!(
            renderer
                .render_scene_to_view(&scene, size, [1.; 4], damage, &destination, format)
                .unwrap()
        );
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        start.elapsed().as_secs_f64() * 1000.
    };
    println!(
        "adapter={:?} backend={:?}",
        gpu.adapter.name, gpu.adapter.backend
    );
    println!("regions,p50_ms,p95_ms,p99_ms,over_budget,late_deadline,effective_fps");
    for regions in [16, 512] {
        const SAMPLES: u32 = 360;
        let period = Duration::from_secs_f64(1. / 120.);
        for tick in 0..60 {
            frame(tick, regions);
        }
        let origin = Instant::now();
        let mut durations = Vec::new();
        let mut late = 0;
        for index in 0..SAMPLES {
            let scheduled = origin + period * index;
            if let Some(wait) = scheduled.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            durations.push(frame(index + 60, regions));
            if Instant::now() > scheduled + period {
                late += 1;
            }
        }
        let measured = origin.elapsed();
        durations.sort_by(f64::total_cmp);
        let over_budget = durations
            .iter()
            .filter(|&&ms| ms > period.as_secs_f64() * 1000.)
            .count();
        println!(
            "{regions},{:.3},{:.3},{:.3},{over_budget},{late},{:.2}",
            durations[179],
            durations[341],
            durations[356],
            f64::from(SAMPLES - 1) / measured.as_secs_f64()
        );
    }
}
