//! Tiny reproducible lossless WebP animations, generated without external assets/tools.
use image::ImageEncoder;
pub struct Part {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub millis: u32,
    pub blend: bool,
    pub dispose: bool,
}
fn u24(value: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&value.to_le_bytes()[..3]);
}
fn chunk(tag: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(0);
    }
}
pub fn animation(width: u32, height: u32, loops: u16, parts: Vec<Part>) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![0x12, 0, 0, 0];
    u24(width - 1, &mut vp8x);
    u24(height - 1, &mut vp8x);
    chunk(b"VP8X", &vp8x, &mut body);
    let mut anim = vec![0, 0, 0, 0];
    anim.extend_from_slice(&loops.to_le_bytes());
    chunk(b"ANIM", &anim, &mut body);
    for part in parts {
        assert_eq!(part.left % 2, 0);
        assert_eq!(part.top % 2, 0);
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .write_image(
                &part.rgba,
                part.width,
                part.height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        let mut data = Vec::new();
        u24(part.left / 2, &mut data);
        u24(part.top / 2, &mut data);
        u24(part.width - 1, &mut data);
        u24(part.height - 1, &mut data);
        u24(part.millis, &mut data);
        data.push(u8::from(part.dispose) | (u8::from(!part.blend) << 1));
        data.extend_from_slice(&encoded[12..]);
        chunk(b"ANMF", &data, &mut body);
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes
}
pub fn fixture(loops: u16) -> Vec<u8> {
    animation(
        4,
        2,
        loops,
        vec![
            Part {
                left: 0,
                top: 0,
                width: 4,
                height: 2,
                rgba: [255, 0, 0, 255].repeat(8),
                millis: 40,
                blend: false,
                dispose: false,
            },
            Part {
                left: 2,
                top: 0,
                width: 2,
                height: 2,
                rgba: [0, 255, 0, 128].repeat(4),
                millis: 80,
                blend: true,
                dispose: true,
            },
            Part {
                left: 0,
                top: 0,
                width: 2,
                height: 2,
                rgba: [0, 0, 255, 255].repeat(4),
                millis: 120,
                blend: false,
                dispose: false,
            },
        ],
    )
}
