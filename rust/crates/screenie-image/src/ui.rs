//! Drawing the picker's overlay for one monitor: the frozen frame, dimmed
//! outside the box, the box outline and cursor marker, and the tags.
//!
//! UI is never part of the output: this draws into a display buffer only.

use std::collections::HashMap;

use screenie_core::geometry::{LogicalPoint, LogicalRect, PixelPoint, PixelRect, rnd};
use screenie_core::lens::{CHIP, ViewFilter, ZOOM};
use screenie_core::session::{BoxView, Mode};
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
    /// The ink, shown as a swatch after the status (Annotate, Shape).
    pub swatch: Option<[f64; 3]>,
    /// List tags (the filter stack), shown beside the box.
    pub lists: Vec<Vec<String>>,
    /// The clip of the layer shown alone (Layers), outlined.
    pub inspect: Option<LogicalRect>,
    /// A pixel cursor (Insert's or the loupe's), when its mode is on top.
    pub pixel_cursor: Option<PixelPoint>,
    /// The loupe's magnifier, when the loupe is on top.
    pub loupe: Option<LoupeView>,
    /// The enhance lens.
    pub enhance: Option<EnhanceView>,
    /// What the lenses show the picture through.
    pub filter: ViewFilter,
    /// Every monitor's rectangle (tags stay on one).
    pub monitors: Vec<LogicalRect>,
}

impl View {
    /// What a session shows now.
    pub fn of(s: &screenie_core::session::Session) -> View {
        let v = View {
            rect: s.rect(),
            cursor: s.selection().cursor_point(),
            active: s.select_is_top(),
            boxes: s.boxes(),
            status: s.status(),
            top: {
                let mut top = s.palette();
                if s.show_help() {
                    top.extend(s.legend());
                }
                top
            },
            swatch: matches!(s.mode_name(), "annotate" | "shape").then(|| s.ink().0.rgb),
            lists: s.lists(),
            inspect: s.inspected().map(|l| l.clip),
            pixel_cursor: None,
            loupe: None,
            enhance: None,
            filter: s.view().filter,
            monitors: s.layout().monitor_rects(),
        };
        let mut v = v;
        for (m, top) in s.lenses() {
            match m {
                Mode::Insert(i) if top => v.pixel_cursor = Some(i.cursor.at),
                Mode::Loupe(l) if top => {
                    v.pixel_cursor = Some(l.cursor.at);
                    v.loupe = Some(LoupeView {
                        rect: s.magnifier(l),
                        at: l.cursor.at,
                        n: s.loupe_px(),
                    });
                }
                Mode::Enhance(e) => {
                    v.enhance = Some(EnhanceView {
                        lens: e.lens.rect(),
                        zoom: e.zoom,
                        sharp: s.view().sharp,
                    });
                }
                _ => {}
            }
        }
        v
    }

    /// What tags keep clear of: the magnifier and its chip.
    pub fn occupied(&self) -> Vec<LogicalRect> {
        self.loupe
            .iter()
            .map(|l| LogicalRect::new(l.rect.x, l.rect.y, l.rect.w, l.rect.h + CHIP))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoupeView {
    /// The magnifier (its chip goes under it).
    pub rect: LogicalRect,
    pub at: PixelPoint,
    /// Frame pixels across.
    pub n: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnhanceView {
    pub lens: LogicalRect,
    pub zoom: f64,
    pub sharp: bool,
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
    /// The main box as shown, view-filtered, for the lenses: keyed by the
    /// picture's generation, the box and the filter.
    lens_source: Option<((u64, PixelRect, ViewFilter), Image)>,
}

impl Ui {
    pub fn new(font: &[u8]) -> Result<Ui, String> {
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default())
            .map_err(|e| format!("font: {e}"))?;
        Ok(Ui {
            font,
            glyphs: HashMap::new(),
            bases: HashMap::new(),
            lens_source: None,
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

        // The open layer over its clip; or the inspected layer alone.
        let patch = if view.inspect.is_some() {
            pic.isolated_patch()
        } else {
            pic.open_patch()
        };
        if let Some((px, patch)) = patch {
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
        if let Some(clip) = view.inspect {
            outline(
                into,
                dev(clip),
                rnd(s).max(1),
                Paint::rgba(1.0, 1.0, 1.0, 0.9),
            );
        }
        for bx in &view.boxes {
            outline(into, dev(bx.rect), t, Paint::rgba(1.0, 1.0, 1.0, 0.9));
            if bx.active {
                marker(into, bx.cursor);
            }
        }

        if view.enhance.is_some() || view.loupe.is_some() {
            self.lenses(pic, monitor, s, view, into);
        }
        if let Some(p) = view.pixel_cursor {
            pixel_cursor(into, pic, monitor, s, p);
        }

        // Tags, status first so the others keep clear of it.
        let status = vec![view.status.clone()];
        let mut placed: Vec<LogicalRect> = view.occupied();
        let blocks = [(&status, tags::STATUS), (&view.top, tags::LEGEND)]
            .into_iter()
            .chain(view.lists.iter().map(|l| (l, tags::LIST)));
        for (lines, spots) in blocks {
            if lines.is_empty() {
                continue;
            }
            let is_status = std::ptr::eq(lines, &status);
            // Room for the ink swatch after the status chip.
            let extra = if is_status && view.swatch.is_some() {
                SWATCH + 5.0
            } else {
                0.0
            };
            let bw = lines.iter().map(|l| self.chip_width(l)).fold(0.0, f64::max) + extra;
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
            if let (true, Some(rgb)) = (is_status, view.swatch) {
                let x = r.x + self.chip_width(&lines[0]) + 5.0 - monitor.x;
                let y = r.y + ROW - 2.0 - monitor.y;
                swatch(into, s, x, y, rgb);
            }
        }
    }

    /// The main box as shown, through the view filter, and where it sits.
    fn lens_source(
        &mut self,
        pic: &Picture,
        main: LogicalRect,
        filter: ViewFilter,
    ) -> (PixelRect, &Image) {
        let r = pic.committed().map().to_pixels(main);
        let key = (pic.generation(), r, filter);
        if self.lens_source.as_ref().is_none_or(|(k, _)| *k != key) {
            let mut img = pic.shown_crop(r);
            crate::view::apply(&mut img, filter);
            self.lens_source = Some((key, img));
        }
        (r, &self.lens_source.as_ref().expect("just made").1)
    }

    fn lenses(
        &mut self,
        pic: &Picture,
        monitor: LogicalRect,
        s: f64,
        view: &View,
        into: &mut Image,
    ) {
        let map = *pic.committed().map();
        let main = view.rect;
        let dev = |r: LogicalRect| {
            let x0 = rnd((r.x - monitor.x) * s);
            let y0 = rnd((r.y - monitor.y) * s);
            PixelRect::new(
                x0,
                y0,
                rnd((r.right() - monitor.x) * s) - x0,
                rnd((r.bottom() - monitor.y) * s) - y0,
            )
        };
        let t = rnd(2.0 * s).max(1);
        let white = Paint::rgba(1.0, 1.0, 1.0, 0.9);
        let (src_rect, src) = self.lens_source(pic, main, view.filter);
        let src = src.clone();
        // A pixel of what's shown: through the view filter inside the box.
        let sample = |p: PixelPoint| -> Option<[u8; 4]> {
            let (lx, ly) = (p.x - src_rect.x, p.y - src_rect.y);
            if lx >= 0 && ly >= 0 && lx < src_rect.w && ly < src_rect.h {
                let i = (ly as usize * src.width() as usize + lx as usize) * 4;
                return Some([src.data()[i], src.data()[i + 1], src.data()[i + 2], 255]);
            }
            pic.rgb(p).map(|c| [c.0, c.1, c.2, 255])
        };

        if let Some(e) = view.enhance {
            let d = dev(e.lens);
            fill(into, d, Paint::rgba(0.0, 0.0, 0.0, 1.0));
            let c = e.lens.centre();
            let (iw, ih) = (into.width() as i32, into.height() as i32);
            for y in d.y.max(0)..(d.y + d.h).min(ih) {
                for x in d.x.max(0)..(d.x + d.w).min(iw) {
                    // The logical point this device pixel shows, through the zoom.
                    let p = LogicalPoint::new(
                        monitor.x + (f64::from(x) + 0.5) / s,
                        monitor.y + (f64::from(y) + 0.5) / s,
                    );
                    let q =
                        LogicalPoint::new(c.x + (p.x - c.x) / e.zoom, c.y + (p.y - c.y) / e.zoom);
                    if !main.contains(q) {
                        continue; // nothing from outside the box
                    }
                    let (fx, fy) = map.to_device(q);
                    let px = if e.sharp {
                        sample(PixelPoint::new(fx.floor() as i32, fy.floor() as i32))
                    } else {
                        bilinear(&sample, fx - 0.5, fy - 0.5)
                    };
                    if let Some(v) = px {
                        let i = ((y * iw + x) * 4) as usize;
                        into.data_mut()[i..i + 4].copy_from_slice(&v);
                    }
                }
            }
            outline(into, d, t, white);
        }

        if let Some(l) = view.loupe {
            let d = dev(l.rect);
            fill(into, d, Paint::rgba(0.0, 0.0, 0.0, 1.0));
            let half = l.n / 2;
            for j in 0..l.n {
                for i in 0..l.n {
                    let p = PixelPoint::new(l.at.x - half + i, l.at.y - half + j);
                    let Some(v) = sample(p) else { continue };
                    let cell = LogicalRect::new(
                        l.rect.x + f64::from(i) * ZOOM,
                        l.rect.y + f64::from(j) * ZOOM,
                        ZOOM,
                        ZOOM,
                    );
                    fill(
                        into,
                        dev(cell),
                        Paint::rgba(
                            f32::from(v[0]) / 255.0,
                            f32::from(v[1]) / 255.0,
                            f32::from(v[2]) / 255.0,
                            1.0,
                        ),
                    );
                }
            }
            // The pixel grid, faint.
            let grid = Paint::rgba(1.0, 1.0, 1.0, 0.12);
            for i in 1..l.n {
                let off = f64::from(i) * ZOOM;
                let v = dev(LogicalRect::new(l.rect.x + off, l.rect.y, 0.0, l.rect.h));
                fill(into, PixelRect::new(v.x, v.y, 1, v.h), grid);
                let h = dev(LogicalRect::new(l.rect.x, l.rect.y + off, l.rect.w, 0.0));
                fill(into, PixelRect::new(h.x, h.y, h.w, 1), grid);
            }
            // The pixel under the cursor, outlined inverted.
            let c = f64::from(half) * ZOOM;
            let cell = dev(LogicalRect::new(l.rect.x + c, l.rect.y + c, ZOOM, ZOOM));
            invert_outline(
                into,
                PixelRect::new(cell.x + 1, cell.y + 1, cell.w - 2, cell.h - 2),
                1,
            );
            outline(into, d, t, white);
            // Its colour and position, under the magnifier.
            if let Some(v) = sample(l.at) {
                let text = format!(
                    "#{:02x}{:02x}{:02x}  ({},{})",
                    v[0], v[1], v[2], l.at.x, l.at.y
                );
                let (x, y) = (l.rect.x, l.rect.bottom() + CHIP);
                self.chip(into, monitor, s, x, y, &text);
                let sx = x + self.chip_width(&text) + 5.0 - monitor.x;
                swatch(
                    into,
                    s,
                    sx,
                    y - 2.0 - monitor.y,
                    [0, 1, 2].map(|k| f64::from(v[k]) / 255.0),
                );
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

/// A bilinear sample at a continuous pixel position.
fn bilinear(sample: &impl Fn(PixelPoint) -> Option<[u8; 4]>, x: f64, y: f64) -> Option<[u8; 4]> {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);
    let at = |dx, dy| {
        sample(PixelPoint::new(x0 + dx, y0 + dy)).or_else(|| sample(PixelPoint::new(x0, y0)))
    };
    let (a, b, c, d) = (at(0, 0)?, at(1, 0)?, at(0, 1)?, at(1, 1)?);
    Some([0, 1, 2, 3].map(|k| {
        let top = f64::from(a[k]) * (1.0 - tx) + f64::from(b[k]) * tx;
        let bottom = f64::from(c[k]) * (1.0 - tx) + f64::from(d[k]) * tx;
        (top * (1.0 - ty) + bottom * ty).round() as u8
    }))
}

/// Invert the pixels of a rectangle (DIFFERENCE with white): shows on any
/// colour.
fn invert(into: &mut Image, r: PixelRect) {
    let (iw, ih) = (into.width() as i32, into.height() as i32);
    for y in r.y.max(0)..(r.y + r.h).min(ih) {
        for x in r.x.max(0)..(r.x + r.w).min(iw) {
            let i = ((y * iw + x) * 4) as usize;
            for k in 0..3 {
                into.data_mut()[i + k] = 255 - into.data()[i + k];
            }
        }
    }
}

/// A `t`-pixel inverted outline just inside `r`.
fn invert_outline(into: &mut Image, r: PixelRect, t: i32) {
    invert(into, PixelRect::new(r.x, r.y, r.w, t));
    invert(into, PixelRect::new(r.x, r.y + r.h - t, r.w, t));
    invert(into, PixelRect::new(r.x, r.y + t, t, r.h - 2 * t));
    invert(into, PixelRect::new(r.x + r.w - t, r.y + t, t, r.h - 2 * t));
}

/// The cursor on one frame pixel (Insert, the loupe): a solid 3x3 on it
/// and a hollow 15x15 around it, in frame pixels so it sits on the pixel
/// grid, and inverted so it shows on any colour.
fn pixel_cursor(into: &mut Image, pic: &Picture, monitor: LogicalRect, s: f64, p: PixelPoint) {
    let map = pic.committed().map();
    let dev = |x: i32, y: i32, w: i32, h: i32| {
        let a = map.from_device(f64::from(x), f64::from(y));
        let b = map.from_device(f64::from(x + w), f64::from(y + h));
        let (x0, y0) = (rnd((a.x - monitor.x) * s), rnd((a.y - monitor.y) * s));
        PixelRect::new(
            x0,
            y0,
            rnd((b.x - monitor.x) * s) - x0,
            rnd((b.y - monitor.y) * s) - y0,
        )
    };
    invert(into, dev(p.x - 1, p.y - 1, 3, 3));
    let t = dev(0, 0, 1, 1).w.max(1);
    invert_outline(into, dev(p.x - 7, p.y - 7, 15, 15), t);
}

/// The ink swatch's side, logical px.
const SWATCH: f64 = 13.0;

/// A colour square whose bottom-left is at `(x, y)` (monitor-relative
/// logical px), with a thin two-tone border so it shows on anything.
fn swatch(into: &mut Image, s: f64, x: f64, y: f64, rgb: [f64; 3]) {
    let px = |v: f64| rnd(v * s);
    let r = PixelRect::new(
        px(x),
        px(y - SWATCH - 2.0),
        px(x + SWATCH) - px(x),
        px(y - 2.0) - px(y - SWATCH - 2.0),
    );
    let t = px(1.0).max(1);
    outline(into, r, t, Paint::rgba(1.0, 1.0, 1.0, 0.8));
    let inner = PixelRect::new(r.x + t, r.y + t, r.w - 2 * t, r.h - 2 * t);
    outline(into, inner, t, Paint::rgba(0.0, 0.0, 0.0, 0.8));
    fill(
        into,
        inner,
        Paint::rgba(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, 1.0),
    );
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
