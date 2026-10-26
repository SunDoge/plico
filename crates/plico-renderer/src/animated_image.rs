//! Bounded animated WebP/GIF decoding and a host-clock-driven player.
//!
//! Decode on a worker, retain composited frames, and wake at `next_frame_after`.
//! No wall clock, executor, thread, or rendering loop is created by this module.
#![warn(missing_docs)]
use crate::image::{DecodeLimits, RasterImage, memory_limit};
use image::{AnimationDecoder, ImageDecoder, metadata::LoopCount};
use std::{io::Cursor, sync::Arc, time::Duration};

/// Resource budgets for eager frame caching. Decoder scratch memory is additional.
#[derive(Clone, Copy, Debug)]
pub struct AnimationLimits {
    /// Per-frame/input limits.
    pub image: DecodeLimits,
    /// Maximum accepted frame count.
    pub max_frames: usize,
    /// Maximum total retained, composited RGBA bytes.
    pub max_retained_bytes: u64,
}
impl Default for AnimationLimits {
    fn default() -> Self {
        Self {
            image: DecodeLimits::default(),
            max_frames: 256,
            max_retained_bytes: 256 * 1024 * 1024,
        }
    }
}
/// A bounded set of full-canvas frames. WebP blending/disposal is handled by the decoder.
pub struct AnimatedImage {
    frames: Vec<Arc<RasterImage>>,
    ends: Vec<Duration>,
    loops: LoopCount,
    byte_len: u64,
}
impl AnimatedImage {
    /// Decode animated (or still) WebP without scaling. Rejects rather than truncates
    /// animations that exceed a budget. Frames returned by the decoder are composited.
    pub fn decode_webp(bytes: &[u8], limits: AnimationLimits) -> image::ImageResult<Self> {
        limits.image.check_input(bytes)?;
        let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?;
        let (width, height) = decoder.dimensions();
        let frame_bytes = limits.image.check_dimensions(width, height)?;
        if limits.max_frames == 0 || frame_bytes > limits.max_retained_bytes {
            return Err(memory_limit());
        }
        if !decoder.has_animation() {
            return Ok(Self {
                frames: vec![Arc::new(RasterImage::decode_with_limits(
                    bytes,
                    limits.image,
                )?)],
                ends: vec![Duration::from_millis(100)],
                loops: LoopCount::Finite(std::num::NonZeroU32::new(1).unwrap()),
                byte_len: frame_bytes,
            });
        }
        // Explicit transparent canvas also enables disposal in image-webp.
        // The file background color is a viewer hint, not opaque UI content.
        decoder.set_background_color(image::Rgba([0, 0, 0, 0]))?;
        let loops = decoder.loop_count();
        Self::collect(decoder.into_frames(), width, height, loops, limits)
    }
    /// Decode composited GIF frames, preserving the file loop count and delays.
    pub fn decode_gif(bytes: &[u8], limits: AnimationLimits) -> image::ImageResult<Self> {
        limits.image.check_input(bytes)?;
        let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
        let (width, height) = decoder.dimensions();
        limits.image.check_dimensions(width, height)?;
        let mut budget = image::Limits::default();
        budget.max_image_width = Some(limits.image.max_dimension);
        budget.max_image_height = Some(limits.image.max_dimension);
        budget.max_alloc = Some(limits.image.max_rgba_bytes);
        decoder.set_limits(budget)?;
        // GIF repeat counts are additional plays; a missing loop extension is
        // Finite(0), meaning one play. image's generic conversion loses this distinction.
        let header = gif::DecodeOptions::new()
            .read_info(Cursor::new(bytes))
            .map_err(|error| {
                image::ImageError::Decoding(image::error::DecodingError::new(
                    image::error::ImageFormatHint::Exact(image::ImageFormat::Gif),
                    error,
                ))
            })?;
        let loops = match header.repeat() {
            gif::Repeat::Infinite => LoopCount::Infinite,
            gif::Repeat::Finite(n) => {
                LoopCount::Finite(std::num::NonZeroU32::new(u32::from(n) + 1).unwrap())
            }
        };
        Self::collect(decoder.into_frames(), width, height, loops, limits)
    }
    fn collect(
        frames_in: image::Frames<'_>,
        width: u32,
        height: u32,
        loops: LoopCount,
        limits: AnimationLimits,
    ) -> image::ImageResult<Self> {
        let frame_bytes = limits.image.check_dimensions(width, height)?;
        let mut frames = Vec::new();
        let mut ends = Vec::new();
        let mut total = Duration::ZERO;
        let mut byte_len = 0u64;
        for frame in frames_in {
            let frame = frame?;
            byte_len = byte_len.checked_add(frame_bytes).ok_or_else(memory_limit)?;
            if frames.len() >= limits.max_frames || byte_len > limits.max_retained_bytes {
                return Err(memory_limit());
            }
            let (n, d) = frame.delay().numer_denom_ms();
            // A zero-delay frame must not create a busy loop in a host scheduler.
            let delay = if n == 0 {
                Duration::from_millis(10)
            } else {
                Duration::from_nanos((u64::from(n) * 1_000_000).div_ceil(u64::from(d)))
            };
            total = total.checked_add(delay).ok_or_else(memory_limit)?;
            let rgba = frame.into_buffer();
            if rgba.dimensions() != (width, height) {
                return Err(memory_limit());
            }
            frames.push(Arc::new(RasterImage::from_rgba(rgba)));
            ends.push(total);
        }
        if frames.is_empty() {
            return Err(memory_limit());
        }
        Ok(Self {
            frames,
            ends,
            loops,
            byte_len,
        })
    }
    /// Full-canvas pixel dimensions, shared by every frame.
    pub fn size(&self) -> [f32; 2] {
        self.frames[0].size()
    }
    /// Number of cached frames.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    /// Total retained RGBA bytes, excluding decoder scratch/GPU copies.
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }
    /// Duration of a single pass.
    pub fn cycle_duration(&self) -> Duration {
        *self.ends.last().unwrap()
    }
    /// Loop metadata from the file; finite counts are total plays.
    pub fn loop_count(&self) -> LoopCount {
        self.loops
    }
    /// Inspect one full composited frame without starting playback.
    pub fn frame(&self, index: usize) -> Option<&Arc<RasterImage>> {
        self.frames.get(index)
    }
    /// Select a frame at an elapsed position, including exact boundaries and completion.
    pub fn sample(&self, elapsed: Duration) -> AnimationSample {
        let cycle = self.cycle_duration();
        let finished = self.frames.len() == 1
            || match self.loops {
                LoopCount::Infinite => false,
                LoopCount::Finite(count) => elapsed >= cycle.saturating_mul(count.get()),
            };
        if finished {
            return AnimationSample {
                image: self.frames.last().unwrap().clone(),
                index: self.frames.len() - 1,
                next_frame_after: None,
                finished: true,
            };
        }
        let phase = elapsed.as_nanos() % cycle.as_nanos();
        let index = self.ends.partition_point(|end| end.as_nanos() <= phase);
        let remaining = self.ends[index].as_nanos() - phase;
        AnimationSample {
            image: self.frames[index].clone(),
            index,
            next_frame_after: Some(Duration::from_nanos(remaining as u64)),
            finished: false,
        }
    }
}
/// A cheap retained frame handle and the next actual frame deadline.
pub struct AnimationSample {
    /// Ready-to-paint pixels (resource identity is reused on repeated samples).
    pub image: Arc<RasterImage>,
    /// Zero-based frame number.
    pub index: usize,
    /// Time until a different frame, or None while stopped/finished/still.
    pub next_frame_after: Option<Duration>,
    /// Whether finite playback has completed (still images count as complete).
    pub finished: bool,
}
/// Playback state using only caller-injected monotonic time.
pub struct ImagePlayer {
    image: Arc<AnimatedImage>,
    position: Duration,
    anchor: Option<Duration>,
    playing: bool,
}
impl ImagePlayer {
    /// Start playback on the first sample's timestamp.
    pub fn new(image: Arc<AnimatedImage>) -> Self {
        Self {
            image,
            position: Duration::ZERO,
            anchor: None,
            playing: true,
        }
    }
    fn position_at(&self, now: Duration) -> Duration {
        self.position.saturating_add(if self.playing {
            self.anchor
                .map_or(Duration::ZERO, |anchor| now.saturating_sub(anchor))
        } else {
            Duration::ZERO
        })
    }
    /// Sample using a host clock. Paused players request no future frame.
    pub fn sample(&mut self, now: Duration) -> AnimationSample {
        if self.playing && self.anchor.is_none() {
            self.anchor = Some(now);
        }
        let mut sample = self.image.sample(self.position_at(now));
        if !self.playing {
            sample.next_frame_after = None;
        }
        sample
    }
    /// Freeze at the current position.
    pub fn pause(&mut self, now: Duration) {
        self.position = self.position_at(now);
        self.playing = false;
        self.anchor = None;
    }
    /// Resume from the current position. Use restart to replay a completed file.
    pub fn play(&mut self, now: Duration) {
        if !self.playing {
            self.anchor = Some(now);
            self.playing = true;
        }
    }
    /// Start again from frame zero.
    pub fn restart(&mut self, now: Duration) {
        self.position = Duration::ZERO;
        self.anchor = Some(now);
        self.playing = true;
    }
    /// Seek to a frame in the first pass; returns false for an invalid index.
    pub fn seek_frame(&mut self, index: usize, now: Duration) -> bool {
        if index >= self.image.frame_count() {
            return false;
        }
        self.position = if index == 0 {
            Duration::ZERO
        } else {
            self.image.ends[index - 1]
        };
        self.anchor = Some(now);
        true
    }
}

#[cfg(test)]
#[path = "../tests/support/webp.rs"]
mod webp;
#[cfg(test)]
mod tests {
    use super::*;
    fn decoded(loops: u16) -> AnimatedImage {
        AnimatedImage::decode_webp(&webp::fixture(loops), AnimationLimits::default()).unwrap()
    }
    #[test]
    fn frames_are_full_canvas_and_partial_alpha_and_disposal_are_composited() {
        let image = decoded(0);
        assert_eq!(image.size(), [4., 2.]);
        assert_eq!(image.frame_count(), 3);
        assert_eq!(image.byte_len(), 96);
        assert_eq!(&image.frame(0).unwrap().rgba()[8..12], &[255, 0, 0, 255]);
        let blend = &image.frame(1).unwrap().rgba()[8..12];
        assert!(
            blend[0].abs_diff(127) <= 1
                && blend[1].abs_diff(128) <= 1
                && blend[2] == 0
                && blend[3] == 255,
            "{blend:?}"
        );
        assert_eq!(&image.frame(2).unwrap().rgba()[..4], &[0, 0, 255, 255]);
        assert_eq!(&image.frame(2).unwrap().rgba()[8..12], &[0, 0, 0, 0]);
    }
    #[test]
    fn boundaries_loops_and_final_hold_are_exact_and_resources_reused() {
        let image = decoded(0);
        for (ms, index) in [
            (0, 0),
            (39, 0),
            (40, 1),
            (119, 1),
            (120, 2),
            (239, 2),
            (240, 0),
            (280, 1),
        ] {
            let sample = image.sample(Duration::from_millis(ms));
            assert_eq!(sample.index, index);
            assert!(Arc::ptr_eq(&sample.image, image.frame(index).unwrap()));
        }
        assert_eq!(
            image.sample(Duration::from_millis(40)).next_frame_after,
            Some(Duration::from_millis(80))
        );
        let image = decoded(2);
        assert_eq!(image.sample(Duration::from_millis(479)).index, 2);
        let end = image.sample(Duration::from_millis(480));
        assert!(end.finished);
        assert_eq!(end.index, 2);
        assert_eq!(end.next_frame_after, None);
    }
    #[test]
    fn pause_resume_seek_and_restart_use_only_injected_time() {
        let image = Arc::new(decoded(0));
        let mut player = ImagePlayer::new(image);
        player.sample(Duration::from_secs(10));
        assert_eq!(player.sample(Duration::from_millis(10050)).index, 1);
        player.pause(Duration::from_millis(10050));
        let paused = player.sample(Duration::from_secs(100));
        assert_eq!(paused.index, 1);
        assert_eq!(paused.next_frame_after, None);
        player.play(Duration::from_secs(100));
        assert_eq!(player.sample(Duration::from_millis(100070)).index, 2);
        player.pause(Duration::from_millis(100070));
        assert!(player.seek_frame(0, Duration::from_secs(101)));
        assert_eq!(player.sample(Duration::from_secs(102)).index, 0);
        assert!(!player.seek_frame(3, Duration::from_secs(102)));
        player.restart(Duration::from_secs(103));
        assert_eq!(player.sample(Duration::from_secs(103)).index, 0);
    }
    #[test]
    fn budgets_reject_instead_of_truncating_and_static_api_rejects_animation() {
        let bytes = webp::fixture(0);
        assert!(RasterImage::decode(&bytes).is_err());
        assert!(
            AnimatedImage::decode_webp(
                &bytes,
                AnimationLimits {
                    max_frames: 2,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            AnimatedImage::decode_webp(
                &bytes,
                AnimationLimits {
                    max_retained_bytes: 95,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            AnimatedImage::decode_webp(
                &bytes,
                AnimationLimits {
                    max_frames: 3,
                    max_retained_bytes: 96,
                    ..Default::default()
                }
            )
            .is_ok()
        );
        assert!(
            AnimatedImage::decode_webp(&bytes[..bytes.len() - 8], AnimationLimits::default())
                .is_err()
        );
    }
    #[test]
    fn zero_duration_is_bounded_and_still_webp_never_requests_frames() {
        let bytes = webp::animation(
            1,
            1,
            0,
            vec![
                webp::Part {
                    left: 0,
                    top: 0,
                    width: 1,
                    height: 1,
                    rgba: vec![255; 4],
                    millis: 0,
                    blend: false,
                    dispose: false,
                },
                webp::Part {
                    left: 0,
                    top: 0,
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 255],
                    millis: 0,
                    blend: false,
                    dispose: false,
                },
            ],
        );
        let a = AnimatedImage::decode_webp(&bytes, AnimationLimits::default()).unwrap();
        assert_eq!(a.cycle_duration(), Duration::from_millis(20));
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .write_image(&[255; 4], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let a = AnimatedImage::decode_webp(&bytes, AnimationLimits::default()).unwrap();
        assert_eq!(a.sample(Duration::ZERO).next_frame_after, None);
    }
}

#[cfg(test)]
pub(crate) mod gif_tests {
    use super::*;
    pub(crate) fn fixture(repeat: Option<image::codecs::gif::Repeat>) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            if let Some(repeat) = repeat {
                encoder.set_repeat(repeat).unwrap();
            }
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(4, 4, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }
    #[test]
    fn gif_frames_delays_and_loop_counts_are_preserved() {
        use image::codecs::gif::Repeat;
        for (repeat, loops) in [(None, 1), (Some(Repeat::Finite(1)), 2)] {
            let animation =
                AnimatedImage::decode_gif(&fixture(repeat), AnimationLimits::default()).unwrap();
            assert_eq!(animation.frame_count(), 2);
            assert_eq!(animation.cycle_duration(), Duration::from_millis(200));
            assert!(matches!(animation.loop_count(),LoopCount::Finite(n) if n.get()==loops));
            assert_eq!(
                animation.sample(Duration::ZERO).image.rgba()[..4],
                [255, 0, 0, 255]
            );
            assert_eq!(
                animation.sample(Duration::from_millis(100)).image.rgba()[..4],
                [0, 0, 255, 255]
            );
            assert!(
                animation
                    .sample(Duration::from_millis(200 * loops as u64))
                    .finished
            );
        }
        let bytes = fixture(Some(Repeat::Infinite));
        let animation = AnimatedImage::decode_gif(&bytes, AnimationLimits::default()).unwrap();
        assert!(matches!(animation.loop_count(), LoopCount::Infinite));
        assert!(!animation.sample(Duration::from_secs(10)).finished);
        assert!(
            AnimatedImage::decode_gif(
                &bytes,
                AnimationLimits {
                    max_frames: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            AnimatedImage::decode_gif(
                &bytes,
                AnimationLimits {
                    max_retained_bytes: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
