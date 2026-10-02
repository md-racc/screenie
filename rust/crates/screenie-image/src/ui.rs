//! Drawing the picker's overlay for one monitor: the frozen frame, dimmed
//! outside the box, the box outline and cursor marker, and the tags.
//!
//! UI is never part of the output: this draws into a display buffer only.

use std::collections::HashMap;

use screenie_core::geometry::{LogicalPoint, LogicalRect, PixelRect, rnd};
use screenie_core::session::BoxView;
use screenie_core::tags::{self, ROW};

use crate::compose::resize;
use crate::image::Image;
use crate::picture::Picture;

/// What a frame of the overlay shows. The front end fills it from the
/// session.
#[derive(Clone, Debug)]
pub struct View {
    /// The main box.
    pub rect: LogicalRect,
    pub cursor: LogicalPoint,
    /// Whether Select is the top mode (bright outline, marker).
    pub active: bool,
    /// The boxes of the modes above Select (a censor's), bottom first.
    pub boxes: Vec<BoxView>,
    pub status: String,
    /// The palette and legend lines, shown above the box.
    pub top: Vec<String>,
    /// Every monitor's rectangle (tags stay on one).
    pub monitors: Vec<LogicalRect>,
}

impl View {
    /// What a session shows now.
    pub fn of(s: &screenie_core::session::Session) -> View {
        View {
            rect: s.rect(),
            cursor: s.selection().cursor_point(),
            active: s.select_is_top(),
            boxes: s.boxes(),
            status: s.status(),
            top: if s.show_help() { s.legend() } else { vec![] },
            monitors: s.layout().monitor_rects(),
        }
    }
}

/// Tag text size, logical px.
pub const FONT_SIZE: f32 = 13.0;
/// The share of the frame's brightness left outside the box.
pub const DIM: f32 = 0.5;

/// Premultiplied RGBA, 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint(pub f32, pub f32, pub f32, pub f32);

impl Paint {
    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Paint {
        Paint(r * a, g * a, b * a, a)
    }
}

/// Composite `paint` over a rectangle, clipped to the image.
pub fn fill(img: &mut Image, r: PixelRect, paint: Paint) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let (x0, y0) = (r.x.max(0), r.y.max(0));
    let (x1, y1) = ((r.x + r.w).min(iw), (r.y + r.h).min(ih));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let keep = 1.0 - paint.3;
    let add = [
        paint.0 * 255.0,
        paint.1 * 255.0,
        paint.2 * 255.0,
        paint.3 * 255.0,
    ];
    let data = img.data_mut();
    for y in y0..y1 {
        let row = (y * iw) as usize * 4;
        for px in data[row + x0 as usize * 4..row + x1 as usize * 4].chunks_exact_mut(4) {
            for c in 0..4 {
                px[c] = (add[c] + f32::from(px[c]) * keep).round().min(255.0) as u8;
            }
        }
    }
}

struct Glyph {
    xmin: i32,
    ymin: i32,
    width: usize,
    height: usize,
    advance: f32,
    coverage: Vec<u8>,
}

/// The monitor's frame pixels at its device size, plain and dimmed.
struct Base {
    size: (u32, u32),
    version: u64,
    plain: Image,
    dimmed: Image,
}

pub struct Ui {
    font: fontdue::Font,
    glyphs: HashMap<(char, u32), Glyph>,
    bases: HashMap<usize, Base>,
}

impl Ui {
    pub fn new(font: &[u8]) -> Result<Ui, String> {
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default())
            .map_err(|e| format!("font: {e}"))?;
        Ok(Ui {
            font,
            glyphs: HashMap::new(),
            bases: HashMap::new(),
        })
    }

    /// A chip's width for `text`, logical px: its advance plus padding.
    pub fn chip_width(&self, text: &str) -> f64 {
        let adv: f32 = text
            .chars()
            .map(|c| self.font.metrics(c, FONT_SIZE).advance_width)
            .sum();
        f64::from(adv) + 8.0
    }

    /// The height of capitals and digits, logical px (chips are sized by
    /// it, not by the text's own glyphs, so descenders don't grow them).
    fn cap_height(&self) -> f64 {
        let m = self.font.metrics('H', FONT_SIZE);
        m.height as f64
    }

    fn glyph(&mut self, c: char, px: f32) -> &Glyph {
        let key = (c, (px * 64.0) as u32);
        self.glyphs.entry(key).or_insert_with(|| {
            let (m, coverage) = self.font.rasterize(c, px);
            Glyph {
                xmin: m.xmin,
                ymin: m.ymin,
                width: m.width,
                height: m.height,
                advance: m.advance_width,
                coverage,
            }
        })
    }

    fn base(
        &mut self,
        pic: &Picture,
        index: usize,
        monitor: LogicalRect,
        size: (u32, u32),
    ) -> &Base {
        let version = pic.version();
        let stale = self
            .bases
            .get(&index)
            .is_none_or(|b| b.size != size || b.version != version);
        if stale {
            let plain = resize(&pic.committed().crop(monitor), size.0, size.1);
            let mut dimmed = plain.clone();
            for px in dimmed.data_mut().chunks_exact_mut(4) {
                for c in &mut px[..3] {
                    *c = (f32::from(*c) * DIM).round() as u8;
                }
            }
            self.bases.insert(
                index,
                Base {
                    size,
                    version,
                    plain,
                    dimmed,
                },
            );
        }
        &self.bases[&index]
    }

    /// Draw monitor `index` (logical rect `monitor`) into `into`.
    pub fn draw(
        &mut self,
        pic: &Picture,
        index: usize,
        monitor: LogicalRect,
        view: &View,
        into: &mut Image,
    ) {
        let size = (into.width(), into.height());
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let s = f64::from(size.0) / monitor.w;
        let dev = |r: LogicalRect| {
            let x0 = rnd((r.x - monitor.x) * s);
            let y0 = rnd((r.y - monitor.y) * s);
            let x1 = rnd((r.right() - monitor.x) * s);
            let y1 = rnd((r.bottom() - monitor.y) * s);
            PixelRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
        };

        let base = self.base(pic, index, monitor, size);
        into.data_mut().copy_from_slice(base.dimmed.data());
        let b = dev(view.rect);
        into.blit(&base.plain, b, b.x, b.y);

        // The open layer, over its clip.
        if let Some((px, patch)) = pic.open_patch() {
            let m = pic.committed().map();
            let at = m.to_logical(screenie_core::geometry::PixelPoint::new(px.x, px.y));
            let k = m.scale();
            let logical = LogicalRect::new(
                at.x - 0.5 / k,
                at.y - 0.5 / k,
                f64::from(px.w) / k,
                f64::from(px.h) / k,
            );
            let d = dev(logical);
            if (patch.width(), patch.height()) == (d.w as u32, d.h as u32) {
                into.blit(patch, patch.extent(), d.x, d.y);
            } else if d.w > 0 && d.h > 0 {
                let fitted = resize(patch, d.w as u32, d.h as u32);
                into.blit(&fitted, fitted.extent(), d.x, d.y);
            }
        }

        let t = rnd(2.0 * s).max(1);
        let a = if view.active { 0.9 } else { 0.5 };
        outline(into, b, t, Paint::rgba(1.0, 1.0, 1.0, a));
        let marker = |into: &mut Image, c: LogicalPoint| {
            let m = dev(LogicalRect::new(c.x - 4.0, c.y - 4.0, 8.0, 8.0));
            fill(into, m, Paint::rgba(1.0, 1.0, 1.0, 1.0));
        };
        if view.active {
            marker(into, view.cursor);
        }
        for bx in &view.boxes {
            outline(into, dev(bx.rect), t, Paint::rgba(1.0, 1.0, 1.0, 0.9));
            if bx.active {
                marker(into, bx.cursor);
            }
        }

        // Tags, status first so the others keep clear of it.
        let status = vec![view.status.clone()];
        let mut placed: Vec<LogicalRect> = Vec::new();
        for (lines, spots) in [(&status, tags::STATUS), (&view.top, tags::LEGEND)] {
            if lines.is_empty() {
                continue;
            }
            let bw = lines.iter().map(|l| self.chip_width(l)).fold(0.0, f64::max);
            let bh = ROW * lines.len() as f64;
            let r = tags::place(view.rect, bw, bh, spots, &view.monitors, &placed);
            placed.push(r);
            if !r.intersects(&monitor) {
                continue;
            }
            for (i, line) in lines.iter().enumerate() {
                let bottom = r.y + ROW * (i as f64 + 1.0) - 2.0;
                self.chip(into, monitor, s, r.x, bottom, line);
            }
        }
    }

    /// A text chip whose bottom-left corner is at `(x, y)`, global logical.
    fn chip(&mut self, into: &mut Image, monitor: LogicalRect, s: f64, x: f64, y: f64, text: &str) {
        let cap = self.cap_height();
        let w = self.chip_width(text);
        let (lx, ly) = (x - monitor.x, y - monitor.y);
        let top = ly - cap - 8.0;
        let r = PixelRect::new(
            rnd(lx * s),
            rnd(top * s),
            rnd((lx + w) * s) - rnd(lx * s),
            rnd(ly * s) - rnd(top * s),
        );
        fill(into, r, Paint::rgba(0.0, 0.0, 0.0, 0.92));
        let px = FONT_SIZE * s as f32;
        let mut pen = ((lx + 4.0) * s) as f32;
        let baseline = ((ly - 4.0) * s).round() as i32;
        for c in text.chars() {
            let g = self.glyph(c, px);
            let gx = (pen.round() as i32) + g.xmin;
            let gy = baseline - g.height as i32 - g.ymin;
            let advance = g.advance;
            blend_coverage(into, gx, gy, g.width, g.height, &g.coverage);
            pen += advance;
        }
    }
}

/// A `t`-pixel outline just outside `b`.
fn outline(into: &mut Image, b: PixelRect, t: i32, paint: Paint) {
    for r in [
        PixelRect::new(b.x - t, b.y - t, b.w + 2 * t, t),
        PixelRect::new(b.x - t, b.y + b.h, b.w + 2 * t, t),
        PixelRect::new(b.x - t, b.y, t, b.h),
        PixelRect::new(b.x + b.w, b.y, t, b.h),
    ] {
        fill(into, r, paint);
    }
}

/// Composite white text coverage at `(x, y)`.
fn blend_coverage(img: &mut Image, x: i32, y: i32, w: usize, h: usize, coverage: &[u8]) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let data = img.data_mut();
    for gy in 0..h as i32 {
        let py = y + gy;
        if py < 0 || py >= ih {
            continue;
        }
        for gx in 0..w as i32 {
            let px = x + gx;
            if px < 0 || px >= iw {
                continue;
            }
            let a = u32::from(coverage[(gy as usize) * w + gx as usize]);
            if a == 0 {
                continue;
            }
            let i = ((py * iw + px) * 4) as usize;
            for c in 0..4 {
                let d = u32::from(data[i + c]);
                data[i + c] = ((255 * a + d * (255 - a) + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rgb;
    use screenie_core::geometry::PixelPoint;

    #[test]
    fn fill_composites_over() {
        let mut img = Image::filled(4, 4, Rgb(200, 100, 0));
        fill(
            &mut img,
            PixelRect::new(1, 1, 2, 2),
            Paint::rgba(0.0, 0.0, 0.0, 0.5),
        );
        assert_eq!(img.rgb(PixelPoint::new(1, 1)), Some(Rgb(100, 50, 0)));
        assert_eq!(img.rgb(PixelPoint::new(0, 0)), Some(Rgb(200, 100, 0)));
        fill(
            &mut img,
            PixelRect::new(-5, -5, 100, 1),
            Paint::rgba(1.0, 1.0, 1.0, 1.0),
        ); // clipped
    }

    #[test]
    fn white_coverage_blends_toward_white() {
        let mut img = Image::filled(2, 1, Rgb(0, 0, 0));
        blend_coverage(&mut img, 0, 0, 2, 1, &[255, 128]);
        assert_eq!(img.rgb(PixelPoint::new(0, 0)), Some(Rgb(255, 255, 255)));
        assert_eq!(img.rgb(PixelPoint::new(1, 0)), Some(Rgb(128, 128, 128)));
    }
}
