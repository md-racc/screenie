//! Image encoding.

use std::io;

use crate::image::Image;

/// Encode as PNG: RGB (screenshots are opaque), at a zlib level 0-9. Level
/// 1, the default, is fast and only a few percent bigger.
pub fn png(img: &Image, level: u8) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
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
