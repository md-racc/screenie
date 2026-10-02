//! Image encoding.

use std::io;

use crate::image::Image;

/// Encode as PNG: RGB (screenshots are opaque), at a zlib level 0-9. Level
/// 1, the default, is fast and only a few percent bigger.
pub fn png(img: &Image, level: u8) -> io::Result<Vec<u8>> {
    png_with_text(img, level, None)
}

/// PNG with a UTF-8 text chunk (iTXt) under `keyword`.
pub fn png_with_text(img: &Image, level: u8, text: Option<(&str, &str)>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
    if let Some((keyword, text)) = text {
        enc.add_itxt_chunk(keyword.to_string(), text.to_string())
            .map_err(io::Error::other)?;
    }
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_deflate_compression(match level {
        0 => png::DeflateCompression::NoCompression,
        1 => png::DeflateCompression::FdeflateUltraFast,
        n => png::DeflateCompression::Level(n.min(9)),
    });
    enc.set_filter(if level <= 1 {
        png::Filter::Up
    } else {
        png::Filter::Adaptive
    });
    let mut w = enc.write_header().map_err(io::Error::other)?;
    let rgb: Vec<u8> = img
        .data()
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    w.write_image_data(&rgb).map_err(io::Error::other)?;
    w.finish().map_err(io::Error::other)?;
    Ok(out)
}

/// A PNG's width and height, from its header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let r = png::Decoder::new(io::Cursor::new(bytes)).read_info().ok()?;
    let i = r.info();
    Some((i.width, i.height))
}

/// Read a PNG (8-bit RGB, RGBA, grey) into an image, with the text of its
/// `keyword` iTXt chunk if it has one.
pub fn read_png(bytes: &[u8], keyword: &str) -> io::Result<(Image, Option<String>)> {
    let mut dec = png::Decoder::new(io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(io::Error::other)?;
    let text = r
        .info()
        .utf8_text
        .iter()
        .find(|t| t.keyword == keyword)
        .and_then(|t| t.get_text().ok());
    let mut buf = vec![
        0;
        r.output_buffer_size()
            .ok_or_else(|| io::Error::other("too big"))?
    ];
    let info = r.next_frame(&mut buf).map_err(io::Error::other)?;
    let (w, h) = (info.width, info.height);
    let n = (w * h) as usize;
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..n * 4].to_vec(),
        png::ColorType::Rgb => buf[..n * 3]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf[..n].iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..n * 2]
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err(io::Error::other("indexed PNG left unexpanded")),
    };
    // Premultiply (screenshots are opaque; this is for other images).
    let rgba = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let a = u16::from(p[3]);
            [0, 1, 2]
                .map(|k| ((u16::from(p[k]) * a + 127) / 255) as u8)
                .into_iter()
                .chain([p[3]])
        })
        .collect();
    let img = Image::from_rgba(w, h, rgba).ok_or_else(|| io::Error::other("bad size"))?;
    Ok((img, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rgb;
    use screenie_core::geometry::PixelPoint;

    fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut r = png::Decoder::new(io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    #[test]
    fn a_text_chunk_travels_with_the_pixels() {
        let img = Image::filled(3, 2, Rgb(1, 2, 3));
        let bytes = png_with_text(&img, 1, Some(("screenie", r#"{"version":1}"#))).unwrap();
        let (back, text) = read_png(&bytes, "screenie").unwrap();
        assert_eq!(back, img);
        assert_eq!(text.as_deref(), Some(r#"{"version":1}"#));
        let (_, none) = read_png(&png(&img, 1).unwrap(), "screenie").unwrap();
        assert_eq!(none, None);
    }

    #[test]
    fn round_trips_at_every_level() {
        let mut img = Image::filled(5, 3, Rgb(10, 20, 30));
        let i = (2 * 5 + 4) * 4;
        img.data_mut()[i..i + 4].copy_from_slice(&[200, 100, 50, 255]);
        for level in [0, 1, 6, 9] {
            let (w, h, rgb) = decode(&png(&img, level).unwrap());
            assert_eq!((w, h), (5, 3));
            assert_eq!(&rgb[0..3], &[10, 20, 30]);
            assert_eq!(&rgb[(2 * 5 + 4) * 3..][..3], &[200, 100, 50]);
        }
        assert_eq!(img.rgb(PixelPoint::new(4, 2)), Some(Rgb(200, 100, 50)));
    }
}
