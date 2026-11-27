//! Opt-in diagnostic recording of retained native window frames.

use crate::window_renderer::WindowRenderer;
use plico_wgpu::RetainedFrameReadback;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SLOTS: usize = 4;
const MAX_SAMPLES: u32 = 600;

pub(crate) struct WindowRecording {
    directory: Option<PathBuf>,
    mp4: bool,
    active: Option<Session>,
    worker: Option<JoinHandle<()>>,
}

struct Session {
    directory: PathBuf,
    started: Instant,
    submitted: u32,
    dropped: u32,
    size: [u32; 2],
    jobs: SyncSender<Job>,
    done: Receiver<Option<RetainedFrameReadback>>,
    in_flight: usize,
    buffers: Vec<RetainedFrameReadback>,
}

enum Job {
    Frame(Sample, RetainedFrameReadback),
    Finish { duration: Duration, dropped: u32 },
}

#[derive(Serialize)]
struct Sample {
    index: u32,
    elapsed_us: u64,
    telemetry_frame_id: Option<u64>,
    retained_initial_frame: bool,
    build_ms: f64,
    scene_ms: f64,
    present_ms: f64,
    file: String,
}

impl WindowRecording {
    pub(crate) fn from_environment() -> Self {
        Self {
            directory: std::env::var_os("PLICO_RECORD_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            mp4: std::env::var_os("PLICO_RECORD_MP4").is_some_and(|v| v == "1"),
            active: None,
            worker: None,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.directory.is_some()
    }

    /// Returns whether to request the first sample from an idle window.
    pub(crate) fn toggle(&mut self, size: [u32; 2]) -> bool {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.join_worker();
        }
        if self.active.is_some() {
            self.stop();
            return false;
        }
        if self.worker.is_some() {
            eprintln!("Plico is still saving the previous recording");
            return false;
        }
        let Some(root) = self.directory.as_deref() else {
            return false;
        };
        match self.start(root, size) {
            Ok((session, worker)) => {
                eprintln!("Plico recording started: {}", session.directory.display());
                self.active = Some(session);
                self.worker = Some(worker);
                true
            }
            Err(error) => {
                eprintln!("Plico recording unavailable: {error}");
                false
            }
        }
    }

    fn start(&self, root: &Path, size: [u32; 2]) -> io::Result<(Session, JoinHandle<()>)> {
        let directory = new_directory(root)?;
        let (jobs, pending) = mpsc::sync_channel(SLOTS + 1);
        let (completed, done) = mpsc::channel();
        let output = directory.clone();
        let mp4 = self.mp4;
        let worker = thread::Builder::new()
            .name("plico-window-recording".into())
            .spawn(
                move || match write_recording(&output, size, mp4, pending, completed) {
                    Ok(()) => eprintln!("Plico recording saved: {}", output.display()),
                    Err(error) => {
                        eprintln!("Plico recording failed ({}): {error}", output.display())
                    }
                },
            )?;
        Ok((
            Session {
                directory,
                started: Instant::now(),
                size,
                submitted: 0,
                dropped: 0,
                jobs,
                done,
                in_flight: 0,
                buffers: Vec::new(),
            },
            worker,
        ))
    }

    pub(crate) fn sample(
        &mut self,
        renderer: &WindowRenderer,
        size: [u32; 2],
        unchanged: bool,
        frame_id: u64,
        timings: [f64; 3],
    ) {
        let Some(session) = self.active.as_mut() else {
            return;
        };
        // A single session has a fixed physical size for pixel comparisons and
        // video. A resize ends the session before it can mix resolutions.
        if size != session.size {
            self.stop();
            return;
        }
        if unchanged && session.submitted > 0 {
            return;
        }
        for buffer in session.done.try_iter() {
            session.in_flight -= 1;
            session.buffers.extend(buffer);
        }
        if session.in_flight == SLOTS {
            session.dropped = session.dropped.saturating_add(1);
            return;
        }
        let elapsed_us = micros(session.started.elapsed());
        let mut measured = crate::telemetry::span("frame", "recording.copy");
        measured.attribute("parent", frame_id);
        let Some(copy) = renderer.copy_retained_frame(session.buffers.pop()) else {
            return;
        };
        if copy.size() != session.size {
            self.stop();
            return;
        }
        let index = session.submitted.saturating_add(session.dropped);
        let [build_ms, scene_ms, present_ms] = timings;
        let sample = Sample {
            index,
            elapsed_us,
            telemetry_frame_id: (frame_id != 0).then_some(frame_id),
            retained_initial_frame: unchanged,
            build_ms,
            scene_ms,
            present_ms,
            file: format!("frame-{index:06}.png"),
        };
        if session.jobs.try_send(Job::Frame(sample, copy)).is_err() {
            self.stop();
            return;
        }
        session.in_flight += 1;
        session.submitted += 1;
        if session.submitted == MAX_SAMPLES {
            self.stop();
        }
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn stop(&mut self) {
        if let Some(session) = self.active.take() {
            // One extra slot is reserved for Finish, even if all copies are in flight.
            let _ = session.jobs.try_send(Job::Finish {
                duration: session.started.elapsed(),
                dropped: session.dropped,
            });
            eprintln!("Plico recording stopped: {}", session.directory.display());
        }
    }
}

impl Drop for WindowRecording {
    fn drop(&mut self) {
        self.stop();
        self.join_worker();
    }
}

fn new_directory(root: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(root)?;
    let time = micros(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default(),
    );
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    for index in 0..100 {
        let path = root.join(format!("window-{time}-{}-{index}", std::process::id()));
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("could not reserve a recording directory"))
}

#[derive(Serialize)]
struct Manifest {
    version: u32,
    width: u32,
    height: u32,
    duration_us: u64,
    dropped_busy: u32,
    failed_readbacks: u32,
    frames: Vec<Sample>,
}

fn write_recording(
    directory: &Path,
    [width, height]: [u32; 2],
    mp4: bool,
    pending: Receiver<Job>,
    completed: mpsc::Sender<Option<RetainedFrameReadback>>,
) -> io::Result<()> {
    let mut manifest = Manifest {
        version: 1,
        width,
        height,
        duration_us: 0,
        dropped_busy: 0,
        failed_readbacks: 0,
        frames: Vec::new(),
    };
    for job in pending {
        match job {
            Job::Frame(sample, readback) => match readback.read() {
                Ok(mut pixels) => {
                    let _ = completed.send(Some(readback));
                    crate::renderer::unpremultiply(&mut pixels);
                    write_png(
                        directory.join(&sample.file).as_path(),
                        width,
                        height,
                        &pixels,
                    )?;
                    manifest.frames.push(sample);
                }
                Err(error) => {
                    eprintln!(
                        "Plico recording frame {} readback failed: {error}",
                        sample.index
                    );
                    manifest.failed_readbacks += 1;
                    let _ = completed.send(None);
                }
            },
            Job::Finish { duration, dropped } => {
                manifest.duration_us = micros(duration);
                manifest.dropped_busy = dropped;
                break;
            }
        }
    }
    let mut manifest_file = BufWriter::new(File::create(directory.join("frames.json"))?);
    serde_json::to_writer_pretty(&mut manifest_file, &manifest)?;
    manifest_file.flush()?;
    if mp4 && !manifest.frames.is_empty() {
        encode_mp4(directory, &manifest)?;
    }
    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
    use image::ImageEncoder as _;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut file = BufWriter::new(File::create(path)?);
    PngEncoder::new_with_quality(&mut file, CompressionType::Fast, FilterType::NoFilter)
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .map_err(io::Error::other)?;
    file.flush()
}

fn encode_mp4(directory: &Path, manifest: &Manifest) -> io::Result<()> {
    let mut concat = BufWriter::new(File::create(directory.join("timeline.ffconcat"))?);
    writeln!(concat, "ffconcat version 1.0")?;
    for (index, frame) in manifest.frames.iter().enumerate() {
        let next = manifest
            .frames
            .get(index + 1)
            .map_or(manifest.duration_us, |frame| frame.elapsed_us);
        let duration_us = next.saturating_sub(frame.elapsed_us).max(1_000);
        writeln!(
            concat,
            "file {}\noption framerate 1000\nduration {:.6}",
            frame.file,
            duration_us as f64 / 1_000_000.
        )?;
    }
    // The concat demuxer needs a following packet to hold the final image.
    let last = &manifest.frames.last().expect("nonempty recording").file;
    writeln!(concat, "file {last}\noption framerate 1000")?;
    concat.flush()?;
    let status = Command::new("ffmpeg")
        .current_dir(directory)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            "timeline.ffconcat",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "recording.tmp.mp4",
        ])
        .stdin(Stdio::null())
        .spawn()?
        .wait()?;
    if !status.success() {
        let _ = fs::remove_file(directory.join("recording.tmp.mp4"));
        return Err(io::Error::other(format!(
            "FFmpeg exited with {status}; PNGs and timestamps are available"
        )));
    }
    fs::rename(
        directory.join("recording.tmp.mp4"),
        directory.join("recording.mp4"),
    )
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}
