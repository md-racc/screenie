//! Opening an image for `--from FILE` and `edit FILE`: PNG, JPEG, WebP and
//! PPM/PGM, as an opaque picture (any transparency flattened over a
//! background, since screenie's pictures are screens).

use crate::image::{Image, Rgb};

/// Decode `bytes` by their format, flattening alpha over `background`.
pub fn decode(bytes: &[u8], background: Rgb) -> Result<Image, String> {
    // read_png premultiplies (the picture's convention); the rest don't.
    let ((w, h, mut rgba), premultiplied) = if bytes.starts_with(b"\x89PNG") {
        let (img, _) = crate::encode::read_png(bytes, "").map_err(|e| format!("PNG: {e}"))?;
        ((img.width(), img.height(), img.into_data()), true)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        (jpeg(bytes)?, false)
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        (webp(bytes)?, false)
    } else if bytes.len() > 2 && bytes[0] == b'P' && b"2356".contains(&bytes[1]) {
        (pnm(bytes)?, false)
    } else {
        return Err("not an image screenie reads: PNG, JPEG, WebP or PPM".into());
    };
    let bg = [background.0, background.1, background.2];
    for p in rgba.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        if a < 255 {
            for k in 0..3 {
                let c = if premultiplied {
                    u32::from(p[k]) * 255
                } else {
                    u32::from(p[k]) * a
                };
                let v = (c + u32::from(bg[k]) * (255 - a) + 127) / 255;
                p[k] = v as u8;
            }
            p[3] = 255;
        }
    }
    Image::from_rgba(w, h, rgba).ok_or_else(|| "the image's size doesn't match its data".into())
}

fn jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let px = d.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let info = d.info().ok_or("JPEG: no header")?;
    let img = Image::from_rgba(u32::from(info.width), u32::from(info.height), px)
        .ok_or("JPEG: the image's size doesn't match its data")?;
    // A camera's pixels are as the sensor saw them; EXIF says how to turn
    // them upright (here as a flip, then a counter-clockwise turn).
    let t = match d.exif().and_then(|e| exif_orientation(e)) {
        Some(2) => 4,
        Some(3) => 2,
        Some(4) => 6,
        Some(5) => 5,
        Some(6) => 3,
        Some(7) => 7,
        Some(8) => 1,
        _ => 0,
    };
    let img = crate::compose::orient(img, crate::compose::Transform(t));
    Ok((img.width(), img.height(), img.into_data()))
}

/// The orientation (1-8) in an EXIF block (a TIFF header and its first
/// directory), if it gives one.
fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..4)? {
        b"MM\0*" => true,
        b"II*\0" => false,
        _ => return None,
    };
    let u16_at = |i: usize| {
        let b: [u8; 2] = tiff.get(i..i + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |i: usize| {
        let b: [u8; 4] = tiff.get(i..i + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let dir = usize::try_from(u32_at(4)?).ok()?;
    // Twelve-byte entries: tag, type (3 is SHORT), count, value.
    (0..usize::from(u16_at(dir)?))
        .map(|k| dir + 2 + 12 * k)
        .find(|&e| u16_at(e) == Some(0x0112))
        .filter(|&e| u16_at(e + 2) == Some(3))
        .and_then(|e| u16_at(e + 8))
        .filter(|v| (1..=8).contains(v))
}

fn webp(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("WebP: {e}"))?;
    let (w, h) = d.dimensions();
    let alpha = d.has_alpha();
    let mut buf = vec![0; d.output_buffer_size().ok_or("WebP: too big")?];
    d.read_image(&mut buf).map_err(|e| format!("WebP: {e}"))?;
    let rgba = if alpha {
        buf
    } else {
        buf.chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    Ok((w, h, rgba))
}

/// PPM (P6, P3) and PGM (P5, P2), up to 16 bits a sample.
fn pnm(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let bad = |why: &str| format!("PPM: {why}");
    let kind = bytes[1];
    // The header: magic, width, height, maxval, with # comments.
    let mut at = 2;
    let mut fields = Vec::new();
    while fields.len() < 3 {
        while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b'#') {
            if bytes[at] == b'#' {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            } else {
                at += 1;
            }
        }
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        let n: u32 = std::str::from_utf8(&bytes[start..at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| bad("a bad header"))?;
        fields.push(n);
    }
    let (w, h, max) = (fields[0], fields[1], fields[2]);
    if w == 0 || h == 0 || max == 0 || max > 65535 {
        return Err(bad("a bad size or depth"));
    }
    if u64::from(w) * u64::from(h) > crate::image::MAX_PIXELS {
        return Err(bad("too big"));
    }
    let channels = if matches!(kind, b'3' | b'6') { 3 } else { 1 };
    let n = w as usize * h as usize * channels;
    // Samples are at most 65535, so this can't overflow.
    let scale = |v: u32| ((v * 255 + max / 2) / max).min(255) as u8;
    let samples: Vec<u8> = if matches!(kind, b'5' | b'6') {
        let data = &bytes[(at + 1).min(bytes.len())..]; // one whitespace, then data
        let wide = max > 255;
        let need = if wide { 2 * n } else { n };
        if data.len() < need {
            return Err(bad("cut short"));
        }
        (0..n)
            .map(|i| {
                scale(if wide {
                    u32::from(data[2 * i]) << 8 | u32::from(data[2 * i + 1])
                } else {
                    u32::from(data[i])
                })
            })
            .collect()
    } else {
        let text = std::str::from_utf8(&bytes[at..]).map_err(|_| bad("not text"))?;
        let v: Vec<u8> = text
            .split_ascii_whitespace()
            .take(n)
            .map(|t| t.parse::<u32>().ok().filter(|&v| v <= max).map(scale))
            .collect::<Option<_>>()
            .ok_or_else(|| bad("a bad sample"))?;
        if v.len() < n {
            return Err(bad("cut short"));
        }
        v
    };
    let rgba = if channels == 3 {
        samples
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    } else {
        samples.iter().flat_map(|&g| [g, g, g, 255]).collect()
    };
    Ok((w, h, rgba))
}
