//! Pixel buffers.
//!
//! The one internal representation is 8-bit premultiplied RGBA, row-major,
//! with no row padding. Capture backends convert into it once.

use screenie_core::geometry::{PixelPoint, PixelRect};

/// An opaque colour, 8 bits a channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// An owned, mutable premultiplied RGBA8 image.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

/// The most pixels an image read in may have: 16384², the JPEG and WebP
/// decoders' own bound (a gigabyte as RGBA).
pub const MAX_PIXELS: u64 = 1 << 28;

/// The side of a damage tile, px.
pub const TILE: usize = 64;
/// More damaged rectangles than this become their bounds.
pub const MAX_DAMAGE: usize = 32;

impl Image {
    /// A transparent image.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize * 4],
        }
    }

    /// An image filled with one opaque colour.
    pub fn filled(width: u32, height: u32, c: Rgb) -> Self {
        let mut img = Self::new(width, height);
        for px in img.data.chunks_exact_mut(4) {
            px.copy_from_slice(&[c.0, c.1, c.2, 255]);
        }
        img
    }

    /// Wrap premultiplied RGBA bytes. `None` if the length is wrong.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize * 4).then_some(Self {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// The image's own extent, as a pixel rectangle at the origin.
    pub fn extent(&self) -> PixelRect {
        PixelRect::new(0, 0, self.width as i32, self.height as i32)
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    /// A pixel's colour (its alpha ignored), or `None` outside the image.
    pub fn rgb(&self, p: PixelPoint) -> Option<Rgb> {
        let (x, y) = (u32::try_from(p.x).ok()?, u32::try_from(p.y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = self.offset(x, y);
        Some(Rgb(self.data[i], self.data[i + 1], self.data[i + 2]))
    }

    /// A copy of a rectangle. The part outside the image is transparent.
    pub fn crop(&self, r: PixelRect) -> Image {
        let mut out = Image::new(r.w.max(0) as u32, r.h.max(0) as u32);
        out.blit(self, r, 0, 0);
        out
    }

    /// Where this and `other` differ, as `TILE`-pixel tiles merged into
    /// rectangles (runs along a band, then runs of the same span down
    /// bands); at most `MAX_DAMAGE` of them, else their bounds. Empty where
    /// nothing differs. Different sizes differ everywhere.
    pub fn damage(&self, other: &Image) -> Vec<PixelRect> {
        if (self.width, self.height) != (other.width, other.height) {
            return vec![self.extent()];
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let row = w * 4;
        let mut rects: Vec<PixelRect> = Vec::new();
        // The runs of the band above, still open for growing down.
        let mut open: Vec<usize> = Vec::new();
        for band in (0..h).step_by(TILE) {
            let rows = band..(band + TILE).min(h);
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for tx in (0..w).step_by(TILE) {
                let (x0, x1) = (tx * 4, (tx + TILE).min(w) * 4);
                let differs = rows.clone().any(|y| {
                    let r = y * row;
                    self.data[r + x0..r + x1] != other.data[r + x0..r + x1]
                });
                if differs {
                    match runs.last_mut() {
                        Some(run) if run.1 == tx => run.1 = (tx + TILE).min(w),
                        _ => runs.push((tx, (tx + TILE).min(w))),
                    }
                }
            }
            let y1 = rows.end as i32;
            let mut next = Vec::new();
            for (x0, x1) in runs {
                let above = open.iter().copied().find(|&i| {
                    let r = rects[i];
                    (r.x, r.x + r.w, r.y + r.h) == (x0 as i32, x1 as i32, band as i32)
                });
                match above {
                    Some(i) => {
                        rects[i].h = y1 - rects[i].y;
                        next.push(i);
                    }
                    None => {
                        next.push(rects.len());
                        rects.push(PixelRect::new(
                            x0 as i32,
                            band as i32,
                            (x1 - x0) as i32,
                            y1 - band as i32,
                        ));
                    }
                }
            }
            open = next;
        }
        if rects.len() > MAX_DAMAGE {
            let (x0, y0) = rects
                .iter()
                .fold((i32::MAX, i32::MAX), |(x, y), r| (x.min(r.x), y.min(r.y)));
            let (x1, y1) = rects
                .iter()
                .fold((0, 0), |(x, y), r| (x.max(r.x + r.w), y.max(r.y + r.h)));
            return vec![PixelRect::new(x0, y0, x1 - x0, y1 - y0)];
        }
        rects
    }

    /// Copy `src`'s rectangle `from` to `(x, y)` here, clipped to both.
    pub fn blit(&mut self, src: &Image, from: PixelRect, x: i32, y: i32) {
        // Clip the source rectangle to the source image...
        let sx0 = from.x.max(0);
        let sy0 = from.y.max(0);
        let sx1 = (from.x + from.w).min(src.width as i32);
        let sy1 = (from.y + from.h).min(src.height as i32);
        // ...and its destination to this one.
        let (dx0, dy0) = (x + (sx0 - from.x), y + (sy0 - from.y));
        let cx0 = dx0.max(0);
        let cy0 = dy0.max(0);
        let cx1 = (dx0 + (sx1 - sx0)).min(self.width as i32);
        let cy1 = (dy0 + (sy1 - sy0)).min(self.height as i32);
        if cx1 <= cx0 || cy1 <= cy0 {
            return;
        }
        let n = (cx1 - cx0) as usize * 4;
        for dy in cy0..cy1 {
            let sy = sy0 + (dy - dy0);
            let sx = sx0 + (cx0 - dx0);
            let s = src.offset(sx as u32, sy as u32);
            let d = self.offset(cx0 as u32, dy as u32);
            self.data[d..d + n].copy_from_slice(&src.data[s..s + n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(img: &mut Image, x: i32, y: i32, w: u32, h: u32) {
        let red = Image::filled(w, h, Rgb(255, 0, 0));
        img.blit(&red, red.extent(), x, y);
    }

    #[test]
    fn damage_is_the_changed_tiles_merged() {
        let a = Image::filled(300, 200, Rgb(1, 2, 3));
        let mut b = a.clone();
        assert!(a.damage(&b).is_empty());
        // A box's two edges, down three bands: two columns, not their span.
        dot(&mut b, 10, 5, 1, 150);
        dot(&mut b, 200, 5, 1, 150);
        assert_eq!(
            a.damage(&b),
            [
                PixelRect::new(0, 0, 64, 192),
                PixelRect::new(192, 0, 64, 192)
            ]
        );
        // The last tiles are cut to the image.
        dot(&mut b, 299, 199, 1, 1);
        assert!(a.damage(&b).contains(&PixelRect::new(256, 192, 44, 8)));
        assert_eq!(a.damage(&Image::new(5, 5)), [a.extent()]);
        // Too many become their bounds.
        let other = Image::filled(8000, 64, Rgb(0, 0, 0));
        let mut many = other.clone();
        for i in 0..40 {
            dot(&mut many, i * 200, 0, 1, 1);
        }
        assert_eq!(many.damage(&other), [PixelRect::new(0, 0, 7808, 64)]);
    }

    fn gradient(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = img.offset(x, y);
                img.data[i..i + 4].copy_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        img
    }

    #[test]
    fn crop_copies_the_rectangle() {
        let img = gradient(10, 8);
        let c = img.crop(PixelRect::new(2, 3, 4, 2));
        assert_eq!((c.width(), c.height()), (4, 2));
        assert_eq!(c.rgb(PixelPoint::new(0, 0)), Some(Rgb(2, 3, 7)));
        assert_eq!(c.rgb(PixelPoint::new(3, 1)), Some(Rgb(5, 4, 7)));
        assert_eq!(c.rgb(PixelPoint::new(4, 0)), None);
    }

    #[test]
    fn crop_past_the_edge_is_transparent_there() {
        let img = gradient(10, 8);
        let c = img.crop(PixelRect::new(-2, 6, 4, 4));
        assert_eq!(c.rgb(PixelPoint::new(2, 0)), Some(Rgb(0, 6, 7)));
        assert_eq!(c.data()[c.offset(0, 0) + 3], 0);
        assert_eq!(c.data()[c.offset(2, 3) + 3], 0);
    }

    #[test]
    fn blit_clips_to_the_destination() {
        let src = Image::filled(4, 4, Rgb(9, 9, 9));
        let mut dst = Image::new(3, 3);
        dst.blit(&src, src.extent(), 1, 1);
        assert_eq!(dst.rgb(PixelPoint::new(0, 0)), Some(Rgb(0, 0, 0)));
        assert_eq!(dst.rgb(PixelPoint::new(2, 2)), Some(Rgb(9, 9, 9)));
        dst.blit(&src, src.extent(), -10, 0); // entirely off: nothing
    }

    #[test]
    fn hex_colour() {
        assert_eq!(Rgb(255, 16, 0).hex(), "#ff1000");
    }
}
