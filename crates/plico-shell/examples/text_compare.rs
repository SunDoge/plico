//! Stacks reference and candidate renderings into one labeled PNG for
//! side-by-side review: the Chrome reference page, then each boost setting.
//! Inputs are produced by text_probe; this only composes the review sheet.
fn main() {
    let rows: &[(&str, &str)] = &[
        ("CHROME reference", "/tmp/chrome-text.png"),
        ("PLICO boost=0", "/tmp/compare-boost0.png"),
        ("PLICO boost=5", "/tmp/compare-boost5.png"),
        ("PLICO boost=10", "/tmp/compare-boost10.png"),
        ("PLICO boost=15", "/tmp/compare-boost15.png"),
        ("PLICO boost=20", "/tmp/compare-boost20.png"),
    ];
    let label_h = 22u32;
    let gap = 8u32;
    let mut images = Vec::new();
    for (label, path) in rows {
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .to_rgba8();
        images.push((*label, img));
    }
    let width = images
        .iter()
        .map(|(_, i)| i.width())
        .max()
        .unwrap()
        .max(640);
    let height: u32 = images.iter().map(|(_, i)| i.height() + label_h + gap).sum();
    let bg = [0xe4u8, 0xe4, 0xe8];
    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            out.put_pixel(x, y, image::Rgba([bg[0], bg[1], bg[2], 255]));
        }
    }
    let mut y = 0u32;
    for (label, img) in &images {
        // Label: dark text, drawn as simple glyph-less blocks would be
        // unreadable — instead bake the label into the left margin with the
        // font used by the probe.
        draw_label(&mut out, label, 4, y + 4);
        y += label_h;
        for (dy, row) in img.rows().enumerate() {
            for (dx, p) in row.enumerate() {
                out.put_pixel(dx as u32, y + dy as u32, *p);
            }
        }
        y += img.height() + gap;
    }
    out.save("/tmp/text-comparison.png").unwrap();
    println!("wrote /tmp/text-comparison.png ({width}x{height})");
}

fn draw_label(img: &mut image::RgbaImage, text: &str, x: u32, y: u32) {
    // Minimal 5x7 bitmap font for uppercase ASCII + '=' + '()' + digits —
    // review sheets only; no text stack dependency.
    const FONT: &str = concat!(
        "A011101000110001111111000110001\nB111111000110001111110100011011\n",
        "C011111100010000100001000010001\nD111110100011000111111000110001\n",
        "E111111000010000111110000010000\nF111111000010000111100000010000\n",
        "G011111100010000101111000110011\nH100011000110001111111000110001\n",
        "I111110010000100001000010001111\nJ000011000010000100001000010001\n",
        "K100011000110100111100100010001\nL100001000010000100001000010001\n",
        "M10001110111010110001100011000\nN100011000110101101011000110001\n",
        "O011101000110001100011000101110\nP111110100011000111110000010000\n",
        "Q011101000110001100011010110011\nR111110100011000111100100010001\n",
        "S011111100010000011100000110001\nT111110001000100010001000100010\n",
        "U100011000110001100011000101110\nV100011000110001100010100010001\n",
        "W100011000110101101110101011010\nX100011000101010001010100010001\n",
        "Y100011000101010001000100010001\nZ111111000100010001000100011111\n",
        "0000000000000000000000000000000\n1111100001100011111100011011111\n",
        "2011101000110000010000100001111\n3011100000100000011000001100010\n",
        "4100011000101001010111111000100\n5011111000010000111000001100010\n",
        "6001111000010000111101000100010\n7000100001000100010001000111110\n",
        "8011101000100010111001000100010\n9000111000100010111100001000100\n",
        "=000000000011111000000000000000\n(0001000100010000000000000000000\n",
        ")000000000000000010001000100000\n 0000000000000000000000000000000\n",
    );
    let glyphs: Vec<&str> = FONT.lines().collect();
    let index = |c: char| -> Option<usize> {
        let base = match c {
            'A'..='Z' => c as usize - 'A' as usize,
            '0'..='9' => 26 + c as usize - '0' as usize,
            '=' => 36,
            '(' => 37,
            ')' => 38,
            _ => 39,
        };
        Some(base)
    };
    let mut cx = x;
    let max_x = img.width();
    for c in text.chars() {
        if cx + 12 >= max_x {
            break;
        }
        let bits = index(c.to_ascii_uppercase()).and_then(|i| glyphs.get(i).copied());
        if let Some(bits) = bits {
            let bytes = bits.as_bytes();
            for (row_i, row) in bytes.chunks(5).enumerate() {
                for (col, b) in row.iter().enumerate() {
                    if *b == b'1' {
                        for dy in 0..2u32 {
                            for dx in 0..2u32 {
                                img.put_pixel(
                                    cx + col as u32 * 2 + dx,
                                    y + row_i as u32 * 2 + dy,
                                    image::Rgba([0x20, 0x20, 0x24, 255]),
                                );
                            }
                        }
                    }
                }
            }
        }
        cx += 14;
    }
}
