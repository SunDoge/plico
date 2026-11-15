// Composition and raw FFmpeg pipe contracts adapted from MyGO plugins/render
// at 13a0b3e7 (MIT). Plico's engine, clock and wgpu capture are independent.
use crate::Frame;
use ffmpeg_sidecar::command::FfmpegCommand;
use plico::{Theme, UiContext, VirtualClock};
use plico_desktop::{CaptureSession, CapturedFrame, HostView, RenderBackend, Source};
use snafu::Snafu;
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const DEFAULT_FPS: f64 = 30.;

pub struct Composition<F> {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: u64,
    pub scale: f64,
    pub theme: Theme,
    pub motion_blur: MotionBlur,
    view: F,
}

impl<F> Composition<F>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    pub fn new(width: u32, height: u32, frame_count: u64, view: F) -> Self {
        Self {
            width,
            height,
            fps: DEFAULT_FPS,
            frame_count,
            scale: 1.,
            theme: Theme::light(),
            motion_blur: MotionBlur::default(),
            view,
        }
    }

    pub fn fps(mut self, fps: f64) -> Self {
        self.fps = fps;
        self
    }

    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn motion_blur(mut self, motion_blur: MotionBlur) -> Self {
        self.motion_blur = motion_blur;
        self
    }

    fn validate(&self) -> Result<Dimensions, Error> {
        if self.width == 0 || self.height == 0 || self.width > 16_384 || self.height > 16_384 {
            return Err(Error::InvalidComposition {
                message: "dimensions must be in 1..=16384 physical pixels".into(),
            });
        }
        if self.frame_count == 0 {
            return Err(Error::InvalidComposition {
                message: "a composition needs at least one frame".into(),
            });
        }
        if !self.fps.is_finite() || !(0.01..=1000.).contains(&self.fps) {
            return Err(Error::InvalidComposition {
                message: "fps must be finite and in 0.01..=1000".into(),
            });
        }
        if !self.scale.is_finite() || self.scale <= 0. {
            return Err(Error::InvalidComposition {
                message: "scale must be finite and positive".into(),
            });
        }
        let logical_width = (f64::from(self.width) / self.scale).round();
        let logical_height = (f64::from(self.height) / self.scale).round();
        if logical_width < 1.
            || logical_height < 1.
            || (logical_width * self.scale).round() as u32 != self.width
            || (logical_height * self.scale).round() as u32 != self.height
        {
            return Err(Error::InvalidComposition {
                message: "physical dimensions must map to whole logical pixels at this scale"
                    .into(),
            });
        }
        self.motion_blur.validate()?;
        Ok(Dimensions {
            logical_width: logical_width as u32,
            logical_height: logical_height as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionBlur {
    pub samples: u8,
    pub shutter_degrees: f64,
}

impl MotionBlur {
    fn validate(self) -> Result<(), Error> {
        if self.samples > 64
            || !self.shutter_degrees.is_finite()
            || !(0. ..=360.).contains(&self.shutter_degrees)
        {
            return Err(Error::InvalidComposition {
                message: "motion blur takes at most 64 samples and a 0..=360 degree shutter".into(),
            });
        }
        Ok(())
    }

    fn sampling(self) -> (usize, f64) {
        if self.samples <= 1 {
            (1, 0.)
        } else {
            (
                usize::from(self.samples),
                if self.shutter_degrees == 0. {
                    0.5
                } else {
                    self.shutter_degrees / 360.
                },
            )
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Codec {
    #[default]
    H264,
    Vp9,
}

pub struct Options {
    pub from: u64,
    pub to: Option<u64>,
    pub renderer: RenderBackend,
    pub codec: Codec,
    pub bitrate: Option<u64>,
    pub ffmpeg: Option<PathBuf>,
    pub overwrite: bool,
    pub progress: Option<Box<dyn FnMut(Progress)>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            from: 0,
            to: None,
            renderer: RenderBackend::Auto,
            codec: Codec::H264,
            bitrate: None,
            ffmpeg: None,
            overwrite: false,
            progress: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub frame: u64,
    pub done: u64,
    pub frames: u64,
    pub elapsed: Duration,
    pub renderer: Option<String>,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("invalid composition: {message}"))]
    InvalidComposition { message: String },
    #[snafu(display("invalid frame range {from}..{to} for {frames} frames"))]
    InvalidRange { from: u64, to: u64, frames: u64 },
    #[snafu(display("capture failed: {source}"))]
    Capture { source: plico_desktop::CaptureError },
    #[snafu(display("composition clock failed: {source}"))]
    Clock { source: plico::ClockError },
    #[snafu(display("{operation} failed for {}: {source}", path.display()))]
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("image pattern must contain one %d or %0Nd placeholder"))]
    ImagePattern,
    #[snafu(display("output already exists: {}", path.display()))]
    OutputExists { path: PathBuf },
    #[snafu(display("ffmpeg failed: {message}"))]
    Ffmpeg { message: String },
}

struct Dimensions {
    logical_width: u32,
    logical_height: u32,
}

struct CompositionView<'a, F> {
    frame: Frame,
    theme: Theme,
    view: &'a mut F,
}

impl<F> HostView for CompositionView<'_, F>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    fn show(&mut self, ui: &mut UiContext<'_>) {
        (self.view)(ui, self.frame);
    }

    fn theme(&self) -> Theme {
        self.theme
    }
}

struct Runner<'a, F>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    frame_count: u64,
    fps: f64,
    motion_blur: MotionBlur,
    clock: VirtualClock,
    source: Source<CompositionView<'a, F>>,
    capture: CaptureSession,
    width: u32,
    height: u32,
}

impl<'a, F> Runner<'a, F>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    fn new(comp: &'a mut Composition<F>, backend: RenderBackend) -> Result<Self, Error> {
        let dimensions = comp.validate()?;
        let clock = VirtualClock::default();
        let view = CompositionView {
            frame: Frame::new(0, comp.frame_count, comp.fps),
            theme: comp.theme,
            view: &mut comp.view,
        };
        let source = Source::with_clock(view, clock.clone());
        let capture = CaptureSession::new(
            backend,
            dimensions.logical_width,
            dimensions.logical_height,
            comp.scale,
        )
        .map_err(|source| Error::Capture { source })?;
        Ok(Self {
            frame_count: comp.frame_count,
            fps: comp.fps,
            motion_blur: comp.motion_blur,
            clock,
            source,
            capture,
            width: comp.width,
            height: comp.height,
        })
    }

    fn render(&mut self, index: u64) -> Result<CapturedFrame, Error> {
        let (samples, shutter) = self.motion_blur.sampling();
        if samples == 1 {
            return self.render_sample(index, 0.);
        }
        let mut sum = vec![0_u32; self.width as usize * self.height as usize * 4];
        for sample in 0..samples {
            let fraction = if samples == 1 {
                0.
            } else {
                sample as f64 / (samples - 1) as f64
            };
            let subframe = -shutter + shutter * fraction;
            let frame = self.render_sample(index, subframe)?;
            for (sum, value) in sum.iter_mut().zip(frame.rgba()) {
                *sum += u32::from(*value);
            }
        }
        let half = (samples / 2) as u32;
        let pixels = sum
            .into_iter()
            .map(|value| ((value + half) / samples as u32) as u8)
            .collect();
        CapturedFrame::from_premultiplied_rgba(self.width, self.height, pixels)
            .map_err(|source| Error::Capture { source })
    }

    fn render_sample(&mut self, index: u64, subframe: f64) -> Result<CapturedFrame, Error> {
        let frame = Frame::sampled(index as i64, self.frame_count, self.fps, subframe);
        self.clock
            .set(frame.time())
            .map_err(|source| Error::Clock { source })?;
        self.source.update(|view| view.frame = frame);
        self.capture
            .render(&mut self.source)
            .map_err(|source| Error::Capture { source })
    }
}

pub fn frames<F>(
    comp: &mut Composition<F>,
    mut options: Options,
    mut consume: impl FnMut(u64, &CapturedFrame) -> Result<(), Error>,
) -> Result<(), Error>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    let from = options.from;
    let to = options.to.unwrap_or(comp.frame_count);
    if from >= to || to > comp.frame_count {
        return Err(Error::InvalidRange {
            from,
            to,
            frames: comp.frame_count,
        });
    }
    let started = Instant::now();
    let total = to - from;
    let mut runner = Runner::new(comp, options.renderer)?;
    for index in 0..to {
        let image = runner.render(index)?;
        if index < from {
            continue;
        }
        consume(index, &image)?;
        if let Some(progress) = &mut options.progress {
            progress(Progress {
                frame: index,
                done: index - from + 1,
                frames: total,
                elapsed: started.elapsed(),
                renderer: runner.capture.renderer_name().map(str::to_owned),
            });
        }
    }
    Ok(())
}

pub fn still<F>(
    comp: &mut Composition<F>,
    frame: u64,
    mut options: Options,
) -> Result<CapturedFrame, Error>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    options.from = frame;
    options.to = Some(frame.saturating_add(1));
    let mut result = None;
    frames(comp, options, |_, image| {
        result = Some(image.clone());
        Ok(())
    })?;
    result.ok_or_else(|| Error::InvalidRange {
        from: frame,
        to: frame.saturating_add(1),
        frames: comp.frame_count,
    })
}

pub fn images<F>(
    comp: &mut Composition<F>,
    pattern: impl AsRef<str>,
    options: Options,
) -> Result<(), Error>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    let pattern = pattern.as_ref();
    let _ = frame_path(pattern, 0)?;
    frames(comp, options, |index, image| {
        let path = frame_path(pattern, index)?;
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                operation: "create image directory",
                path: parent.to_owned(),
                source,
            })?;
        }
        image
            .write_png(&path)
            .map_err(|source| Error::Capture { source })
    })
}

fn frame_path(pattern: &str, index: u64) -> Result<PathBuf, Error> {
    let bytes = pattern.as_bytes();
    let mut placeholder = None;
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'%' {
            cursor += 1;
            continue;
        }
        if bytes.get(cursor + 1) == Some(&b'%') {
            cursor += 2;
            continue;
        }
        let start = cursor;
        cursor += 1;
        let zero = bytes.get(cursor) == Some(&b'0');
        if zero {
            cursor += 1;
        }
        let digits = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'd') || placeholder.is_some() {
            return Err(Error::ImagePattern);
        }
        let width = if digits == cursor {
            0
        } else {
            pattern[digits..cursor]
                .parse::<usize>()
                .map_err(|_| Error::ImagePattern)?
        };
        placeholder = Some((start, cursor + 1, zero, width));
        cursor += 1;
    }
    let (start, end, zero, width) = placeholder.ok_or(Error::ImagePattern)?;
    let number = if zero {
        format!("{index:0width$}")
    } else if width > 0 {
        format!("{index:width$}")
    } else {
        index.to_string()
    };
    Ok(PathBuf::from(format!(
        "{}{}{}",
        pattern[..start].replace("%%", "%"),
        number,
        pattern[end..].replace("%%", "%")
    )))
}

pub fn video<F>(
    comp: &mut Composition<F>,
    path: impl AsRef<Path>,
    options: Options,
) -> Result<(), Error>
where
    F: FnMut(&mut UiContext<'_>, Frame),
{
    comp.validate()?;
    let path = path.as_ref();
    if path.exists() && !options.overwrite {
        return Err(Error::OutputExists {
            path: path.to_owned(),
        });
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let container = match extension.as_str() {
        "mp4" | "m4v" => "mp4",
        "webm" => "webm",
        _ => {
            return Err(Error::Ffmpeg {
                message: "video output must end in .mp4, .m4v or .webm".into(),
            });
        }
    };
    if (options.codec == Codec::Vp9) != (container == "webm") {
        return Err(Error::Ffmpeg {
            message: "H.264 requires MP4/M4V and VP9 requires WebM".into(),
        });
    }
    let temporary = temporary_video_path(path, &extension);
    if let Some(parent) = temporary
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            operation: "create video directory",
            path: parent.to_owned(),
            source,
        })?;
    }
    let binary: OsString = options
        .ffmpeg
        .as_deref()
        .map(Path::as_os_str)
        .unwrap_or_else(|| std::ffi::OsStr::new("ffmpeg"))
        .to_owned();
    let (rate_num, rate_den) = frame_rate(comp.fps);
    let mut command = FfmpegCommand::new_with_path(binary);
    command.args([
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-s".into(),
        format!("{}x{}", comp.width, comp.height),
        "-framerate".into(),
        format!("{rate_num}/{rate_den}"),
        "-i".into(),
        "pipe:0".into(),
    ]);
    match options.codec {
        Codec::H264 => {
            command.args([
                "-c:v", "libx264", "-preset", "medium", "-pix_fmt", "yuv420p",
            ]);
            if let Some(bitrate) = options.bitrate {
                command.args(["-b:v".into(), bitrate.to_string()]);
            } else {
                command.args(["-crf", "18"]);
            }
            command.args(["-movflags", "+faststart", "-f", container]);
        }
        Codec::Vp9 => {
            command.args(["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-row-mt", "1"]);
            if let Some(bitrate) = options.bitrate {
                command.args(["-b:v".into(), bitrate.to_string()]);
            } else {
                command.args(["-crf", "31", "-b:v", "0"]);
            }
            command.args(["-f", container]);
        }
    }
    command.output(&temporary);
    let mut child = command.spawn().map_err(|source| Error::Io {
        operation: "start ffmpeg",
        path: PathBuf::from("ffmpeg"),
        source,
    })?;
    let mut stdin = child.take_stdin().ok_or_else(|| Error::Ffmpeg {
        message: "ffmpeg did not expose stdin".into(),
    })?;
    let mut stderr = child.take_stderr().ok_or_else(|| Error::Ffmpeg {
        message: "ffmpeg did not expose stderr".into(),
    })?;
    let stderr_reader = std::thread::spawn(move || {
        let mut message = String::new();
        let _ = stderr.read_to_string(&mut message);
        message
    });
    let render_result = frames(comp, options, |_, image| {
        stdin.write_all(image.rgba()).map_err(|source| Error::Io {
            operation: "write ffmpeg frame",
            path: temporary.clone(),
            source,
        })
    });
    drop(stdin);
    if render_result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|source| Error::Io {
        operation: "wait for ffmpeg",
        path: temporary.clone(),
        source,
    })?;
    let stderr = stderr_reader.join().unwrap_or_default();
    if let Err(error) = render_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if !status.success() {
        let _ = std::fs::remove_file(&temporary);
        return Err(Error::Ffmpeg {
            message: stderr.trim().to_owned(),
        });
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|source| Error::Io {
            operation: "replace video",
            path: path.to_owned(),
            source,
        })?;
    }
    std::fs::rename(&temporary, path).map_err(|source| Error::Io {
        operation: "publish video",
        path: path.to_owned(),
        source,
    })
}

fn frame_rate(fps: f64) -> (u64, u64) {
    if fps.fract() == 0. {
        return (fps as u64, 1);
    }
    let ntsc = (fps * 1001.).round();
    if (ntsc / 1001. - fps).abs() < 0.001 && ntsc % 1000. == 0. {
        return (ntsc as u64, 1001);
    }
    ((fps * 1000.).round() as u64, 1000)
}

fn temporary_video_path(path: &Path, extension: &str) -> PathBuf {
    let mut name = path
        .file_stem()
        .map_or_else(|| OsString::from("video"), OsString::from);
    name.push(format!(
        ".plico-rendering-{}.{}",
        std::process::id(),
        extension
    ));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimensions_ranges_motion_blur_and_rates() {
        let mut comp = Composition::new(1920, 1080, 90, |_, _| {})
            .scale(2.)
            .fps(29.97);
        assert!(comp.validate().is_ok());
        assert_eq!(frame_rate(comp.fps), (30_000, 1001));
        comp.motion_blur.samples = 65;
        assert!(comp.validate().is_err());
    }

    #[test]
    fn image_patterns_are_explicit_and_zero_pad() {
        assert_eq!(
            frame_path("frames/frame-%04d.png", 12).unwrap(),
            PathBuf::from("frames/frame-0012.png")
        );
        assert!(frame_path("frames/frame.png", 12).is_err());
        assert!(frame_path("%d-%d.png", 12).is_err());
    }
}
