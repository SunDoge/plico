use plico::prelude::*;
use plico_dev::{services::Services, tasks::TaskStatus};
#[cfg(not(target_arch = "wasm32"))]
use plico_dev::{
    services::{HostService as _, Wake},
    tasks::TaskService,
};

#[cfg_attr(
    any(target_arch = "wasm32", feature = "wasm-host"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    AnalyzeImage { image: u32 },
}

#[cfg_attr(
    any(target_arch = "wasm32", feature = "wasm-host"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub width: u32,
    pub height: u32,
    pub checksum: u64,
}

pub type Snapshot = TaskStatus<Analysis, String>;
pub type ReaderServices = Services<Snapshot, Command>;

pub fn show(cx: &mut UiContext<'_>, image: u32, services: &mut ReaderServices) {
    if button(cx)
        .text("Analyze image")
        .disabled(matches!(services.snapshot(), TaskStatus::Pending))
        .clicked()
    {
        services.send(Command::AnalyzeImage { image });
    }
    match services.snapshot() {
        TaskStatus::Idle => {
            text(cx, "Image analysis idle");
        }
        TaskStatus::Pending => {
            text(cx, "Analyzing image…");
        }
        TaskStatus::Ready(Ok(result)) => {
            text!(
                cx,
                "Image {} × {} · {:016x}",
                result.width,
                result.height,
                result.checksum
            );
        }
        TaskStatus::Ready(Err(error)) => {
            text(cx, error.as_str());
        }
        TaskStatus::Cancelled => {
            text(cx, "Image analysis cancelled");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Worker {
    image_id: u32,
    task: TaskService<u32, Analysis, String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Worker {
    pub fn new(image_id: u32, source: plico::ImageAsset) -> Self {
        use std::{
            path::PathBuf,
            time::{Duration, Instant},
        };
        let gate = std::env::var_os("PLICO_WASM_PROBE_JOB_GATE").map(PathBuf::from);
        let task = TaskService::new(
            move |id| {
                let source = source.clone();
                let gate = gate.clone();
                async move {
                    println!("JOB START {id}");
                    // The probe can hold completion until after a reload.
                    if let Some(gate) = gate {
                        let deadline = Instant::now() + Duration::from_secs(20);
                        while !gate.is_file() {
                            if Instant::now() >= deadline {
                                return Err("analysis gate timed out".into());
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                    let image = image::load_from_memory(source.data())
                        .map_err(|error| error.to_string())?
                        .into_luma8();
                    let checksum = image
                        .as_raw()
                        .iter()
                        .fold(0xcbf29ce484222325u64, |hash, value| {
                            (hash ^ u64::from(*value)).wrapping_mul(0x100000001b3)
                        });
                    let analysis = Analysis {
                        width: image.width(),
                        height: image.height(),
                        checksum,
                    };
                    println!(
                        "JOB DONE {id} {} {} {checksum:016x}",
                        analysis.width, analysis.height
                    );
                    Ok(analysis)
                }
            },
            |future| {
                std::thread::spawn(|| pollster::block_on(future));
            },
        );
        Self { image_id, task }
    }

    pub fn poll(&mut self) -> bool {
        self.task.poll()
    }

    pub fn set_wake_callback(&mut self, wake: Wake) {
        self.task.set_wake_callback(wake);
    }

    #[cfg(not(feature = "wasm-host"))]
    pub fn status(&self) -> &Snapshot {
        self.task.status()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.task.snapshot()
    }

    pub fn dispatch(&mut self, commands: Vec<Command>) -> bool {
        let mut started = false;
        for Command::AnalyzeImage { image } in commands {
            if image == self.image_id && !matches!(self.task.status(), TaskStatus::Pending) {
                self.task.send(plico_dev::tasks::TaskCommand::Start(image));
                started = true;
            }
        }
        started
    }
}
