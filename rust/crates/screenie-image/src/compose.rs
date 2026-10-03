//! Building one frame from per-monitor captures: pixel-format conversion,
//! orientation, scaling, and placement in the layout.

use screenie_core::geometry::{LogicalRect, PixelMap};

use crate::image::Image;

/// The byte layouts capture backends deliver, as they sit in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    /// B, G, R, A/X bytes (wl_shm ARGB8888 / XRGB8888 on little-endian).
    Bgra8,
    /// R, G, B, A/X bytes (wl_shm ABGR8888 / XBGR8888).
    Rgba8,
    /// 32-bit little-endian words, 2:10:10:10, red high (XRGB2101010).
    Xrgb2101010,
    /// 32-bit little-endian words, 2:10:10:10, blue high (XBGR2101010).
    Xbgr2101010,
}

/// Convert a captured buffer (opaque) into an `Image`. `stride` is bytes
/// per row; `y_invert` flips it top to bottom.
pub fn convert(
    data: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    format: SourceFormat,
    y_invert: bool,
) -> Option<Image> {
    let (w, h) = (width as usize, height as usize);
    if stride < w * 4 || data.len() < stride * h.saturating_sub(1) + w * 4 {
        return None;
    }
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let sy = if y_invert { h - 1 - y } else { y };
        let src = &data[sy * stride..sy * stride + w * 4];
        let dst = &mut out[y * w * 4..(y + 1) * w * 4];
        for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
            let (r, g, b) = match format {
                SourceFormat::Bgra8 => (s[2], s[1], s[0]),
                SourceFormat::Rgba8 => (s[0], s[1], s[2]),
                SourceFormat::Xrgb2101010 | SourceFormat::Xbgr2101010 => {
                    let v = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
                    let hi = ((v >> 20) & 0x3ff) >> 2;
                    let mid = ((v >> 10) & 0x3ff) >> 2;
                    let lo = (v & 0x3ff) >> 2;
                    if format == SourceFormat::Xrgb2101010 {
                        (hi as u8, mid as u8, lo as u8)
                    } else {
                        (lo as u8, mid as u8, hi as u8)
                    }
                }
            };
            d.copy_from_slice(&[r, g, b, 255]);
        }
    }
    Image::from_rgba(width, height, out)
}

/// An output's transform, as Wayland numbers them: rotations of 0, 90,
/// 180 and 270 degrees counter-clockwise, then the same flipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform(pub u8);

impl Transform {
    pub const NORMAL: Transform = Transform(0);

    pub fn swaps_axes(self) -> bool {
        self.0 % 2 == 1
    }
}

/// Turn a buffer in the output's native orientation into what the screen
/// shows: rotate counter-clockwise by the transform's angle, after a
/// horizontal flip for the flipped ones.
pub fn orient(img: &Image, t: Transform) -> Image {
    if t == Transform::NORMAL {
        return img.clone();
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    let (ow, oh) = if t.swaps_axes() { (h, w) } else { (w, h) };
    let mut out = vec![0u8; ow * oh * 4];
    let src = img.data();
    for y in 0..h {
        for x in 0..w {
            let fx = if t.0 >= 4 { w - 1 - x } else { x };
            let (nx, ny) = match t.0 % 4 {
                0 => (fx, y),
                1 => (y, w - 1 - fx),
                2 => (w - 1 - fx, h - 1 - y),
                _ => (h - 1 - y, fx),
            };
            let s = (y * w + x) * 4;
            let d = (ny * ow + nx) * 4;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    Image::from_rgba(ow as u32, oh as u32, out).expect("sized to fit")
}

/// Bilinear resize (a lower-scale monitor upsampled to the frame's scale).
pub fn resize(img: &Image, width: u32, height: u32) -> Image {
    if (img.width(), img.height()) == (width, height) {
        return img.clone();
    }
    let (sw, sh) = (img.width() as usize, img.height() as usize);
    let (dw, dh) = (width as usize, height as usize);
    let mut out = vec![0u8; dw * dh * 4];
    if sw == 0 || sh == 0 {
        return Image::from_rgba(width, height, out).expect("sized to fit");
    }
    let src = img.data();
    let sample = |x: usize, y: usize, c: usize| f32::from(src[(y * sw + x) * 4 + c]);
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            for c in 0..4 {
                let top = sample(x0, y0, c) * (1.0 - tx) + sample(x1, y0, c) * tx;
                let bottom = sample(x0, y1, c) * (1.0 - tx) + sample(x1, y1, c) * tx;
                out[(y * dw + x) * 4 + c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
    }
    Image::from_rgba(width, height, out).expect("sized to fit")
}

/// One frame of the whole layout at `scale` pixels per logical pixel, from
/// each monitor's capture (already oriented) and its logical rectangle.
/// Captures at another scale are resized to fit.
pub fn compose(bounds: LogicalRect, scale: f64, parts: &[(LogicalRect, Image)]) -> Image {
    let width = (bounds.w * scale).round().max(1.0) as u32;
    let height = (bounds.h * scale).round().max(1.0) as u32;
    let map = PixelMap::new(bounds, width as i32, height as i32);
    let mut out = Image::new(width, height);
    for (rect, img) in parts {
        let dest = map.to_pixels(*rect);
        let fitted = resize(img, dest.w.max(0) as u32, dest.h.max(0) as u32);
        out.blit(&fitted, fitted.extent(), dest.x, dest.y);
    }
    out
}

/// An image (`--from FILE`) shown on a monitor: on a canvas covering the
/// monitor at scale `k`, the monitor's own or more, so that every image
/// pixel is a canvas pixel (a big image is shown smaller, never resampled).
/// Centred, on `background`. The frame, and the image's logical rect.
pub fn place(
    img: Image,
    monitor: LogicalRect,
    scale: f64,
    background: crate::image::Rgb,
) -> (crate::frame::Frame, LogicalRect) {
    let (iw, ih) = (img.width(), img.height());
    let k = scale
        .max(f64::from(iw) / monitor.w)
        .max(f64::from(ih) / monitor.h);
    let (cw, ch) = ((monitor.w * k).ceil() as u32, (monitor.h * k).ceil() as u32);
    let (ox, oy) = ((cw - iw) / 2, (ch - ih) / 2);
    let mut canvas = Image::filled(cw, ch, background);
    canvas.blit(&img, img.extent(), ox as i32, oy as i32);
    // The canvas's own extent in logical px (a hair over the monitor when
    // k doesn't divide it), so its scale is exactly k.
    let bounds = LogicalRect::new(monitor.x, monitor.y, f64::from(cw) / k, f64::from(ch) / k);
    let rect = LogicalRect::new(
        monitor.x + f64::from(ox) / k,
        monitor.y + f64::from(oy) / k,
        f64::from(iw) / k,
        f64::from(ih) / k,
    );
    (crate::frame::Frame::new(canvas, bounds), rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rgb;
    use screenie_core::geometry::PixelPoint;

    fn px(img: &Image, x: i32, y: i32) -> Rgb {
        img.rgb(PixelPoint::new(x, y)).unwrap()
    }

    #[test]
    fn bgra_and_rgba_and_ten_bit() {
        let bgra = [3, 2, 1, 0, 30, 20, 10, 0];
        let img = convert(&bgra, 2, 1, 8, SourceFormat::Bgra8, false).unwrap();
        assert_eq!(
            (px(&img, 0, 0), px(&img, 1, 0)),
            (Rgb(1, 2, 3), Rgb(10, 20, 30))
        );
        let img = convert(&[1, 2, 3, 0], 1, 1, 4, SourceFormat::Rgba8, false).unwrap();
        assert_eq!(px(&img, 0, 0), Rgb(1, 2, 3));
        // red 0x3ff, green 0x200, blue 0: XRGB2101010.
        let v: u32 = (0x3ff << 20) | (0x200 << 10);
        let img = convert(&v.to_le_bytes(), 1, 1, 4, SourceFormat::Xrgb2101010, false).unwrap();
        assert_eq!(px(&img, 0, 0), Rgb(255, 128, 0));
        let img = convert(&v.to_le_bytes(), 1, 1, 4, SourceFormat::Xbgr2101010, false).unwrap();
        assert_eq!(px(&img, 0, 0), Rgb(0, 128, 255));
    }

    #[test]
    fn stride_padding_and_y_invert() {
        // Two rows of one pixel, each padded to 8 bytes.
        let data = [1, 1, 1, 0, 9, 9, 9, 9, 2, 2, 2, 0];
        let img = convert(&data, 1, 2, 8, SourceFormat::Rgba8, true).unwrap();
        assert_eq!(
            (px(&img, 0, 0), px(&img, 0, 1)),
            (Rgb(2, 2, 2), Rgb(1, 1, 1))
        );
        assert!(convert(&data[..10], 1, 2, 8, SourceFormat::Rgba8, false).is_none());
    }

    fn strip() -> Image {
        // 3x1: red, green, blue.
        Image::from_rgba(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]).unwrap()
    }

    #[test]
    fn rotations() {
        let img = strip();
        let r90 = orient(&img, Transform(1)); // counter-clockwise: red at the bottom
        assert_eq!((r90.width(), r90.height()), (1, 3));
        assert_eq!(
            (px(&r90, 0, 0), px(&r90, 0, 2)),
            (Rgb(0, 0, 255), Rgb(255, 0, 0))
        );
        let r180 = orient(&img, Transform(2));
        assert_eq!(px(&r180, 0, 0), Rgb(0, 0, 255));
        let flipped = orient(&img, Transform(4));
        assert_eq!(px(&flipped, 0, 0), Rgb(0, 0, 255));
        assert_eq!(orient(&orient(&img, Transform(1)), Transform(3)), img);
    }

    #[test]
    fn compose_places_monitors_and_upsamples_the_lower_scale_one() {
        let left = Image::filled(4, 2, Rgb(10, 10, 10)); // 2x2 logical at scale 2
        let right = Image::filled(2, 2, Rgb(200, 200, 200)); // 2x2 logical at scale 1
        let bounds = LogicalRect::new(0.0, 0.0, 4.0, 2.0);
        let out = compose(
            bounds,
            2.0,
            &[
                (LogicalRect::new(0.0, 0.0, 2.0, 2.0), left),
                (LogicalRect::new(2.0, 0.0, 2.0, 1.0), right),
            ],
        );
        assert_eq!((out.width(), out.height()), (8, 4));
        assert_eq!(px(&out, 3, 3), Rgb(10, 10, 10));
        assert_eq!(px(&out, 4, 0), Rgb(200, 200, 200));
        assert_eq!(px(&out, 7, 1), Rgb(200, 200, 200));
        assert_eq!(out.data()[(3 * 8 + 6) * 4 + 3], 0); // nothing there: transparent
    }

    #[test]
    fn resize_keeps_flat_colour_flat() {
        let img = Image::filled(3, 3, Rgb(77, 88, 99));
        let big = resize(&img, 7, 5);
        assert!((0..7).all(|x| px(&big, x, 4) == Rgb(77, 88, 99)));
    }
}
