use image::{DynamicImage, GenericImageView, ImageFormat};
use plico::ImageAsset;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    fs,
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const IMAGES: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];
pub const VIDEOS: &[&str] = &["mp4", "mov", "mkv", "webm", "m4v", "avi"];

#[derive(Debug, Snafu)]
pub enum MediaError {
    #[snafu(display("Could not read {}: {source}", path.display()))]
    Read { path: PathBuf, source: io::Error },
    #[snafu(display("Could not decode image: {source}"))]
    Decode { source: image::ImageError },
    #[snafu(display("This file type is not supported"))]
    Unsupported,
    #[snafu(display("Could not run {program}: {source}"))]
    Tool {
        program: &'static str,
        source: io::Error,
    },
    #[snafu(display("{program} could not read this video"))]
    Probe { program: &'static str },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Photo,
    Video,
}

#[derive(Clone)]
pub struct Asset {
    pub id: u64,
    pub demo: Option<&'static str>,
    pub path: Option<PathBuf>,
    pub title: String,
    pub group: &'static str,
    pub kind: Kind,
    pub dimensions: (u32, u32),
    pub bytes: u64,
    pub format: String,
    pub duration: f64,
    pub palette: Vec<[u8; 3]>,
    pub poster_dimensions: (u32, u32),
    pub poster: ImageAsset,
    pub wall_preview: ImageAsset,
    pub thumbnail: ImageAsset,
    pub favorite: bool,
}

impl Asset {
    pub fn source(&self) -> &str {
        self.path
            .as_deref()
            .and_then(Path::to_str)
            .unwrap_or("Framebank sample library")
    }

    pub fn search(&self, normalized_query: &str) -> bool {
        normalized_query.split_whitespace().all(|term| {
            self.title.to_lowercase().contains(term)
                || self.group.to_lowercase().contains(term)
                || self.format.to_lowercase().contains(term)
                || self.source().to_lowercase().contains(term)
        })
    }

    pub fn fingerprint(&self) -> String {
        self.demo.map_or_else(
            || {
                self.path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default()
            },
            |demo| format!("demo:{demo}"),
        )
    }

    pub fn similar(&self, other: &Self) -> u32 {
        self.palette
            .iter()
            .map(|color| {
                other
                    .palette
                    .iter()
                    .map(|other| {
                        color
                            .iter()
                            .zip(other)
                            .map(|(&a, &b)| u32::from(a.abs_diff(b)).pow(2))
                            .sum()
                    })
                    .min()
                    .unwrap_or(u32::MAX / 8)
            })
            .sum()
    }
}

struct Preview {
    image: DynamicImage,
    dimensions: (u32, u32),
    duration: f64,
}

impl Preview {
    fn photo(bytes: &[u8]) -> Result<Self, MediaError> {
        let image = image::load_from_memory(bytes).context(DecodeSnafu)?;
        Ok(Self {
            dimensions: image.dimensions(),
            image,
            duration: 0.,
        })
    }

    fn video(path: &Path) -> Result<Self, MediaError> {
        let info = video_tool("ffprobe", |command| {
            command
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height:format=duration",
                    "-of",
                    "json",
                ])
                .arg(path);
        })?;
        let info: VideoInfo =
            serde_json::from_slice(&info).map_err(|_| MediaError::Probe { program: "ffprobe" })?;
        let stream = info
            .streams
            .first()
            .ok_or(MediaError::Probe { program: "ffprobe" })?;
        let frame = video_tool("ffmpeg", |command| {
            command
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args([
                    "-map",
                    "0:v:0",
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=1280:1280:force_original_aspect_ratio=decrease:force_divisible_by=2",
                    "-threads",
                    "1",
                    "-f",
                    "image2pipe",
                    "-vcodec",
                    "mjpeg",
                    "pipe:1",
                ]);
        })?;
        let image = image::load_from_memory(&frame).context(DecodeSnafu)?;
        let duration = info
            .format
            .duration
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|seconds| seconds.is_finite() && *seconds > 0.)
            .unwrap_or_default();
        Ok(Self {
            image,
            dimensions: (stream.width, stream.height),
            duration,
        })
    }
}

#[derive(Deserialize)]
struct VideoInfo {
    streams: Vec<VideoStream>,
    format: VideoFormat,
}
#[derive(Deserialize)]
struct VideoStream {
    width: u32,
    height: u32,
}
#[derive(Deserialize)]
struct VideoFormat {
    duration: Option<String>,
}

fn video_tool(
    program: &'static str,
    configure: impl FnOnce(&mut Command),
) -> Result<Vec<u8>, MediaError> {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    configure(&mut command);
    let result = command.output().context(ToolSnafu { program })?;
    if !result.status.success() {
        return Err(MediaError::Probe { program });
    }
    Ok(result.stdout)
}

pub fn load(path: PathBuf) -> Result<Asset, MediaError> {
    let path = path.canonicalize().context(ReadSnafu { path: &path })?;
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (kind, preview) = if IMAGES.contains(&ext.as_str()) {
        let bytes = fs::read(&path).context(ReadSnafu { path: &path })?;
        (Kind::Photo, Preview::photo(&bytes)?)
    } else if VIDEOS.contains(&ext.as_str()) {
        (Kind::Video, Preview::video(&path)?)
    } else {
        return Err(MediaError::Unsupported);
    };
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let bytes = fs::metadata(&path)
        .context(ReadSnafu { path: &path })?
        .len();
    Ok(Asset {
        title,
        bytes,
        kind,
        format: ext.to_ascii_uppercase(),
        path: Some(path),
        group: "Imported",
        ..asset_preview(preview)?
    })
}

fn asset_preview(preview: Preview) -> Result<Asset, MediaError> {
    const POSTER_EDGE: u32 = 2400;
    const WALL_EDGE: u32 = 768;
    let source = preview.image;
    let longest_edge = source.width().max(source.height());
    let thumb = source.resize_to_fill(640, 400, image::imageops::FilterType::Triangle);
    // Keep the masonry wall's uncropped images small enough to stay resident
    // alongside the detail poster during the shared-element transition.
    let wall_preview = if longest_edge > WALL_EDGE {
        Some(encoded(source.thumbnail(WALL_EDGE, WALL_EDGE))?)
    } else {
        None
    };
    let poster = if longest_edge > POSTER_EDGE {
        source.thumbnail(POSTER_EDGE, POSTER_EDGE)
    } else {
        source
    };
    let poster_dimensions = poster.dimensions();
    let palette = palette(&poster);
    let poster = encoded(poster)?;
    Ok(Asset {
        id: 0,
        demo: None,
        path: None,
        title: String::new(),
        group: "",
        kind: Kind::Photo,
        dimensions: preview.dimensions,
        bytes: 0,
        format: "JPG".into(),
        duration: preview.duration,
        palette,
        poster_dimensions,
        wall_preview: wall_preview.unwrap_or_else(|| poster.clone()),
        poster,
        thumbnail: encoded(thumb)?,
        favorite: false,
    })
}

fn encoded(image: DynamicImage) -> Result<ImageAsset, MediaError> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .into_rgb8()
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .context(DecodeSnafu)?;
    Ok(ImageAsset::from_bytes(bytes.get_ref()))
}

fn palette(image: &DynamicImage) -> Vec<[u8; 3]> {
    let mut bins = [(0_u32, [0_u32; 3]); 512];
    for pixel in image.thumbnail(48, 48).into_rgb8().pixels() {
        let [r, g, b] = pixel.0;
        let index = (usize::from(r / 32) << 6) | (usize::from(g / 32) << 3) | usize::from(b / 32);
        let (count, sum) = &mut bins[index];
        *count += 1;
        for (sum, channel) in sum.iter_mut().zip(pixel.0) {
            *sum += u32::from(channel);
        }
    }
    bins.sort_unstable_by_key(|&(count, _)| std::cmp::Reverse(count));
    let mut colors: Vec<[u8; 3]> = Vec::new();
    for (count, sum) in bins.into_iter().take_while(|(count, _)| *count != 0) {
        let next = sum.map(|value| (value / count) as u8);
        if colors.iter().all(|color| {
            color
                .iter()
                .zip(next)
                .map(|(&a, b)| u32::from(a.abs_diff(b)).pow(2))
                .sum::<u32>()
                > 2304
        }) {
            colors.push(next);
        }
        if colors.len() == 5 {
            break;
        }
    }
    colors
}

const DEMO_VIDEO: &[u8] = include_bytes!("../assets/woods.mp4");
pub fn demo_clip() -> io::Result<tempfile::NamedTempFile> {
    let mut file = tempfile::Builder::new().suffix(".mp4").tempfile()?;
    file.write_all(DEMO_VIDEO)?;
    file.flush()?;
    Ok(file)
}

pub fn samples() -> Result<Vec<Asset>, MediaError> {
    let items: &[(&str, &str, &str, &[u8])] = &[
        (
            "ridge",
            "Quiet elevations",
            "Field notes",
            include_bytes!("../assets/ridge.jpg"),
        ),
        (
            "woods",
            "Into the still",
            "Field notes",
            include_bytes!("../assets/woods.jpg"),
        ),
        (
            "shore",
            "Tidal study",
            "Field notes",
            include_bytes!("../assets/shore.jpg"),
        ),
        (
            "architecture",
            "Glass city",
            "Form studies",
            include_bytes!("../assets/architecture.jpg"),
        ),
        (
            "coast",
            "Road to red rock",
            "Form studies",
            include_bytes!("../assets/coast.jpg"),
        ),
        (
            "alpine",
            "After the blue hour",
            "Field notes",
            include_bytes!("../assets/alpine.jpg"),
        ),
        (
            "desert",
            "Dune geometry",
            "Form studies",
            include_bytes!("../assets/desert.jpg"),
        ),
        (
            "garden",
            "The understory",
            "Field notes",
            include_bytes!("../assets/garden.jpg"),
        ),
    ];
    items
        .iter()
        .map(|&(key, title, group, bytes)| {
            let mut asset = asset_preview(Preview::photo(bytes)?)?;
            asset.demo = Some(key);
            asset.title = title.into();
            asset.group = group;
            asset.bytes = bytes.len() as u64;
            if key == "woods" {
                asset.kind = Kind::Video;
                asset.format = "MP4".into();
                asset.duration = 6.;
                asset.dimensions = (768, 432);
                asset.bytes = DEMO_VIDEO.len() as u64;
            }
            Ok(asset)
        })
        .collect()
}

#[derive(Default, Serialize, Deserialize)]
pub struct Catalog {
    pub files: Vec<PathBuf>,
    pub favorites: Vec<String>,
}

impl Catalog {
    pub fn path(name: &str) -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", name)
            .map(|dirs| dirs.data_dir().join("library.json"))
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        match fs::File::open(path) {
            Ok(file) => serde_json::from_reader(file).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_preview_is_bounded_and_preserves_aspect_ratio() {
        let dimensions = (1000, 1500);
        let asset = asset_preview(Preview {
            image: DynamicImage::new_rgb8(dimensions.0, dimensions.1),
            dimensions,
            duration: 0.,
        })
        .unwrap();
        let wall = image::load_from_memory(asset.wall_preview.data()).unwrap();
        assert_eq!(wall.dimensions(), (512, 768));
        assert_eq!(asset.poster_dimensions, dimensions);
    }

    #[test]
    fn small_photos_share_the_original_size_preview() {
        let dimensions = (600, 400);
        let asset = asset_preview(Preview {
            image: DynamicImage::new_rgb8(dimensions.0, dimensions.1),
            dimensions,
            duration: 0.,
        })
        .unwrap();
        assert_eq!(asset.poster_dimensions, dimensions);
        assert_eq!(asset.wall_preview, asset.poster);
    }
}
