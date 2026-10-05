//! Rendering operations onto pixels. Core defines the operations as data;
//! this is the only place that knows what they look like.
//!
//! Everything works on a patch: an image whose top-left sits at `origin`
//! in frame pixels. A layer never touches pixels outside its clip.

use screenie_core::geometry::{PixelMap, PixelPoint, PixelRect};
use screenie_core::model::{
    CalloutOp, CensorKind, CensorOp, FilterKind, FilterOp, Heads, HighlightOp, Layer, LineOp,
    OpData, Rgb, ShapeKind, ShapeOp, StrokeOp, TextOp,
};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, PixmapMut, Rect, Stroke,
    Transform,
};

use crate::image::Image;

fn intersect(a: PixelRect, b: PixelRect) -> PixelRect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    PixelRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

/// A patch: pixels plus where they sit in the frame.
pub struct Patch<'a> {
    pub img: &'a mut Image,
    pub origin: PixelPoint,
}

impl Patch<'_> {
    /// The patch's extent in frame pixels.
    fn extent(&self) -> PixelRect {
        PixelRect::new(
            self.origin.x,
            self.origin.y,
            self.img.width() as i32,
            self.img.height() as i32,
        )
    }

    fn offset(&self, x: i32, y: i32) -> usize {
        let (lx, ly) = (x - self.origin.x, y - self.origin.y);
        (ly as usize * self.img.width() as usize + lx as usize) * 4
    }
}

/// Apply a layer's operations, in order, within its clip.
pub fn apply_layer(patch: &mut Patch, map: &PixelMap, layer: &Layer) {
    let clip = map.to_pixels(layer.clip);
    for op in &layer.ops {
        apply_op(patch, map, clip, &op.data);
    }
}

/// Apply one operation, touching nothing outside `clip` (frame px).
pub fn apply_op(patch: &mut Patch, map: &PixelMap, clip: PixelRect, op: &OpData) {
    match op {
        OpData::Censor(c) => censor(patch, map, clip, c),
        OpData::Stroke(st) => stroke(patch, map, clip, st),
        OpData::Shape(sh) => shape(patch, map, clip, sh),
        OpData::Line(l) => line(patch, map, clip, l),
        OpData::Highlight(h) => highlight(patch, map, clip, h),
        OpData::Text(t) => text(patch, map, clip, t),
        OpData::Callout(c) => callout(patch, map, clip, c),
        OpData::Filter(f) => filter(patch, map, clip, f),
    }
}

/// Apply `f` to every pixel's RGB in `area`.
fn per_pixel(patch: &mut Patch, area: PixelRect, mut f: impl FnMut([f32; 3]) -> [f32; 3]) {
    for y in area.y..area.y + area.h {
        let i = patch.offset(area.x, y);
        for px in patch.img.data_mut()[i..i + area.w as usize * 4].chunks_exact_mut(4) {
            let c = [f32::from(px[0]), f32::from(px[1]), f32::from(px[2])];
            let out = f(c);
            for k in 0..3 {
                px[k] = out[k].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Luminance, as the HSL blend modes define it.
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn lerp(c: [f32; 3], to: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|k| c[k] + (to[k] - c[k]) * t)
}

/// One S-curve pass around mid-grey (overlaying a picture on itself).
fn s_curve(v: f32) -> f32 {
    let x = v / 255.0;
    let y = if x < 0.5 {
        2.0 * x * x
    } else {
        1.0 - 2.0 * (1.0 - x) * (1.0 - x)
    };
    y * 255.0
}

fn filter(patch: &mut Patch, map: &PixelMap, clip: PixelRect, f: &FilterOp) {
    if f.muted {
        return;
    }
    let area = intersect(intersect(map.to_pixels(f.rect), clip), patch.extent());
    if area.is_empty() {
        return;
    }
    let a = f.amount as f32;
    match f.kind {
        FilterKind::Grey => per_pixel(patch, area, |c| lerp(c, [lum(c); 3], a.clamp(0.0, 1.0))),
        FilterKind::Invert => per_pixel(patch, area, |c| c.map(|v| 255.0 - v)),
        FilterKind::Brightness if a > 0.0 => per_pixel(patch, area, |c| c.map(|v| v + 25.5 * a)),
        FilterKind::Brightness => {
            per_pixel(patch, area, |c| c.map(|v| v * (1.0 + 0.1 * a).max(0.0)))
        }
        FilterKind::Contrast if a > 0.0 => {
            let passes = a.round() as u32;
            per_pixel(patch, area, |c| {
                c.map(|mut v| {
                    for _ in 0..passes {
                        v = s_curve(v);
                    }
                    v
                })
            })
        }
        FilterKind::Contrast => {
            per_pixel(patch, area, |c| lerp(c, [127.5; 3], (-0.2 * a).min(1.0)))
        }
        FilterKind::Blur => blur(patch, area, (f.amount * map.scale()).max(1.0)),
    }
}

/// Three box-blur passes (close to a Gaussian), reading past `area` where
/// the patch has pixels (edges are clamped), writing only inside it.
/// Channels are 8.8 fixed point, and each pass a running sum, so a pixel
/// costs the same whatever the radius, and the passes move half the bytes
/// floats would.
fn blur(patch: &mut Patch, area: PixelRect, radius: f64) {
    let r = ((radius / 2.0).round() as i32).max(1);
    let margin = 3 * r;
    let src = intersect(
        PixelRect::new(
            area.x - margin,
            area.y - margin,
            area.w + 2 * margin,
            area.h + 2 * margin,
        ),
        patch.extent(),
    );
    let (w, h) = (src.w as usize, src.h as usize);
    let mut buf = vec![[0u16; 4]; w * h];
    for (y, out) in (src.y..src.y + src.h).zip(buf.chunks_exact_mut(w)) {
        let i = patch.offset(src.x, y);
        for (o, px) in out
            .iter_mut()
            .zip(patch.img.data()[i..i + w * 4].chunks_exact(4))
        {
            *o = [0, 1, 2, 3].map(|k| u16::from(px[k]) << 8);
        }
    }
    let mut tmp = vec![[0u16; 4]; w * h];
    // A sum of 2r+1 values, divided: by a multiply and a shift.
    let n = (2 * r + 1) as u64;
    let mul = ((1u64 << 32) + n / 2) / n;
    let div = |acc: u32| ((u64::from(acc) * mul) >> 32) as u16;
    let clamp = |i: i32, n: usize| i.clamp(0, n as i32 - 1) as usize;
    let ru = r as usize;
    // Across each row (clamped only near its ends)...
    let across = |from: &[[u16; 4]], to: &mut [[u16; 4]]| {
        for (f, t) in from.chunks_exact(w).zip(to.chunks_exact_mut(w)) {
            let mut acc = [0u32; 4];
            for d in -r..=r {
                let p = f[clamp(d, w)];
                for k in 0..4 {
                    acc[k] += u32::from(p[k]);
                }
            }
            for i in 0..w {
                t[i] = acc.map(div);
                let (inn, out) = if i >= ru && i + ru + 1 < w {
                    (f[i + ru + 1], f[i - ru])
                } else {
                    (f[clamp(i as i32 + r + 1, w)], f[clamp(i as i32 - r, w)])
                };
                for k in 0..4 {
                    acc[k] = acc[k] + u32::from(inn[k]) - u32::from(out[k]);
                }
            }
        }
    };
    // ...and down the columns, a row at a time (in memory order).
    let down = |from: &[[u16; 4]], to: &mut [[u16; 4]]| {
        let row = |y: usize| &from[y * w..(y + 1) * w];
        let mut acc = vec![[0u32; 4]; w];
        for d in -r..=r {
            for (a, p) in acc.iter_mut().zip(row(clamp(d, h))) {
                for k in 0..4 {
                    a[k] += u32::from(p[k]);
                }
            }
        }
        for y in 0..h {
            for (t, a) in to[y * w..(y + 1) * w].iter_mut().zip(&acc) {
                *t = a.map(div);
            }
            let (inn, out) = (row(clamp(y as i32 + r + 1, h)), row(clamp(y as i32 - r, h)));
            for ((a, i), o) in acc.iter_mut().zip(inn).zip(out) {
                for k in 0..4 {
                    a[k] = a[k] + u32::from(i[k]) - u32::from(o[k]);
                }
            }
        }
    };
    for _ in 0..3 {
        across(&buf, &mut tmp);
        down(&tmp, &mut buf);
    }
    for y in area.y..area.y + area.h {
        let i = patch.offset(area.x, y);
        let row = (y - src.y) as usize * w + (area.x - src.x) as usize;
        for (px, v) in patch.img.data_mut()[i..i + area.w as usize * 4]
            .chunks_exact_mut(4)
            .zip(&buf[row..])
        {
            for k in 0..4 {
                // Rounded to the nearest level.
                px[k] = ((v[k] + 0x80) >> 8).min(255) as u8;
            }
        }
    }
}

static FONT: std::sync::OnceLock<fontdue::Font> = std::sync::OnceLock::new();

/// The font text and callouts are drawn in (the tags' monospace). Set once
/// by the host; until then text ops draw nothing.
pub fn set_font(bytes: &[u8]) -> Result<(), String> {
    if FONT.get().is_some() {
        return Ok(());
    }
    let f = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|e| format!("font: {e}"))?;
    let _ = FONT.set(f);
    Ok(())
}

fn ink_lum(rgb: Rgb) -> f64 {
    0.3 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2]
}

/// The advance of a line of text, device px.
fn advance(font: &fontdue::Font, line: &str, px: f32) -> f32 {
    line.chars()
        .map(|c| font.metrics(c, px).advance_width)
        .sum()
}

/// Draw a line of text with its baseline at (x, y), device px, in `rgb`.
fn glyphs(
    patch: &mut Patch,
    clip: PixelRect,
    font: &fontdue::Font,
    line: &str,
    (x, y): (f32, f32),
    px: f32,
    rgb: Rgb,
) {
    let area = intersect(clip, patch.extent());
    let col = rgb.map(|v| (v * 255.0) as f32);
    let mut pen = x;
    for c in line.chars() {
        let (m, cov) = font.rasterize(c, px);
        let gx = pen.round() as i32 + m.xmin;
        let gy = y.round() as i32 - m.height as i32 - m.ymin;
        for j in 0..m.height as i32 {
            for i in 0..m.width as i32 {
                let (fx, fy) = (gx + i, gy + j);
                if fx < area.x || fy < area.y || fx >= area.x + area.w || fy >= area.y + area.h {
                    continue;
                }
                let a = f32::from(cov[(j * m.width as i32 + i) as usize]) / 255.0;
                if a == 0.0 {
                    continue;
                }
                let k = patch.offset(fx, fy);
                let px_ = &mut patch.img.data_mut()[k..k + 4];
                for ch in 0..3 {
                    px_[ch] = (col[ch] * a + f32::from(px_[ch]) * (1.0 - a)).round() as u8;
                }
                px_[3] = (255.0 * a + f32::from(px_[3]) * (1.0 - a)).round() as u8;
            }
        }
        pen += m.advance_width;
    }
}

fn text(patch: &mut Patch, map: &PixelMap, clip: PixelRect, t: &TextOp) {
    let Some(font) = FONT.get() else { return };
    let k = map.scale();
    let px = (t.size * k) as f32;
    let (ox, oy) = map.to_device(t.origin);
    let lh = t.line_height() * k;
    let ascent = font
        .horizontal_line_metrics(px)
        .map_or(px * 0.8, |m| m.ascent);
    let lines: Vec<&str> = t.text.split('\n').collect();
    if t.chip {
        let w = lines
            .iter()
            .map(|l| advance(font, l, px))
            .fold(0.0, f32::max) as f64;
        let pad = t.pad() * k;
        let h = lh * lines.len() as f64;
        // Black behind light ink, white behind dark.
        let v = if ink_lum(t.rgb) > 0.5 { 0.0 } else { 1.0 };
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba(v, v, v, 0.85).unwrap_or(Color::BLACK));
        if let Some(r) = Rect::from_xywh(
            (ox - pad) as f32,
            (oy - pad) as f32,
            (w + 2.0 * pad) as f32,
            (h + 2.0 * pad) as f32,
        ) {
            with_pixmap(patch, clip, |pm, tr, mask| {
                pm.fill_rect(r, &paint, tr, mask);
            });
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let y = oy + lh * i as f64 + f64::from(ascent);
        glyphs(patch, clip, font, line, (ox as f32, y as f32), px, t.rgb);
    }
}

fn callout(patch: &mut Patch, map: &PixelMap, clip: PixelRect, c: &CalloutOp) {
    let (cx, cy) = map.to_device(c.centre);
    let r = (c.size / 2.0 * map.scale()) as f32;
    let paint = ink(c.rgb);
    with_pixmap(patch, clip, |pm, t, mask| {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx as f32, cy as f32, r);
        if let Some(path) = pb.finish() {
            pm.fill_path(&path, &paint, FillRule::Winding, t, mask);
        }
    });
    let Some(font) = FONT.get() else { return };
    let label = c.n.to_string();
    let px = r * 1.2;
    let w = advance(font, &label, px);
    let m = font.horizontal_line_metrics(px);
    let (asc, desc) = m.map_or((px * 0.8, -px * 0.2), |m| (m.ascent, m.descent));
    let num = if ink_lum(c.rgb) > 0.55 {
        [0.0; 3]
    } else {
        [1.0; 3]
    };
    let baseline = cy as f32 + (asc + desc) / 2.0;
    glyphs(
        patch,
        clip,
        font,
        &label,
        (cx as f32 - w / 2.0, baseline),
        px,
        num,
    );
}

/// Paint for ink: opaque, anti-aliased.
fn ink(rgb: Rgb) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(
        Color::from_rgba(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, 1.0).unwrap_or(Color::BLACK),
    );
    paint.anti_alias = true;
    paint
}

/// The clip as a mask over the patch, or `None` if it covers it all.
/// `Err` if nothing of the patch is inside the clip.
fn clip_mask(patch: &Patch, clip: PixelRect) -> Result<Option<Mask>, ()> {
    let ext = patch.extent();
    let inside = intersect(clip, ext);
    if inside.is_empty() {
        return Err(());
    }
    if inside == ext {
        return Ok(None);
    }
    let (w, h) = (patch.img.width(), patch.img.height());
    let mut mask = Mask::new(w, h).ok_or(())?;
    let r = Rect::from_xywh(
        (inside.x - patch.origin.x) as f32,
        (inside.y - patch.origin.y) as f32,
        inside.w as f32,
        inside.h as f32,
    )
    .ok_or(())?;
    mask.fill_path(
        &PathBuilder::from_rect(r),
        FillRule::Winding,
        false,
        Transform::identity(),
    );
    Ok(Some(mask))
}

/// Run `draw` on the patch as a tiny-skia pixmap, in frame pixels (the
/// transform takes them to the patch), clipped.
fn with_pixmap(
    patch: &mut Patch,
    clip: PixelRect,
    draw: impl FnOnce(&mut PixmapMut, Transform, Option<&Mask>),
) {
    let Ok(mask) = clip_mask(patch, clip) else {
        return;
    };
    let (w, h) = (patch.img.width(), patch.img.height());
    let t = Transform::from_translate(-patch.origin.x as f32, -patch.origin.y as f32);
    if let Some(mut pm) = PixmapMut::from_bytes(patch.img.data_mut(), w, h) {
        draw(&mut pm, t, mask.as_ref());
    }
}

fn round_stroke(width: f32) -> Stroke {
    Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    }
}

fn stroke(patch: &mut Patch, map: &PixelMap, clip: PixelRect, st: &StrokeOp) {
    let width = (st.width * map.scale()) as f32;
    let pts: Vec<(f32, f32)> = st
        .points
        .iter()
        .map(|&p| {
            let (x, y) = map.to_device(p);
            (x as f32, y as f32)
        })
        .collect();
    let Some(&(x0, y0)) = pts.first() else { return };
    let paint = ink(st.rgb);
    with_pixmap(patch, clip, |pm, t, mask| {
        let mut pb = PathBuilder::new();
        if pts.iter().all(|&p| p == (x0, y0)) {
            // A click: a dot, as round caps on a zero-length line would be.
            pb.push_circle(x0, y0, width / 2.0);
            if let Some(path) = pb.finish() {
                pm.fill_path(&path, &paint, FillRule::Winding, t, mask);
            }
            return;
        }
        pb.move_to(x0, y0);
        for &(x, y) in &pts[1..] {
            pb.line_to(x, y);
        }
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint, &round_stroke(width), t, mask);
        }
    });
}

/// A highlighter trace: one path, so overlaps don't double up, multiplied
/// at its opacity so dark text under it stays dark.
/// The highlighter's paint: its ink at its opacity, multiplied.
fn highlight_paint(h: &HighlightOp) -> Paint<'static> {
    let mut paint = ink(h.rgb);
    paint.shader = tiny_skia::Shader::SolidColor(
        Color::from_rgba(
            h.rgb[0] as f32,
            h.rgb[1] as f32,
            h.rgb[2] as f32,
            h.alpha as f32,
        )
        .unwrap_or(Color::BLACK),
    );
    paint.blend_mode = tiny_skia::BlendMode::Multiply;
    paint
}

/// A highlighter trace in progress, kept between frames: its coverage
/// over one patch, so a trace that grew adds only its new tail. Drawing
/// it whole each frame gets dearer the longer the line.
pub struct HighlightTrace {
    op: HighlightOp,
    origin: PixelPoint,
    coverage: Mask,
    /// What the coverage reaches, in patch pixels: only that is multiplied.
    reach: Option<Rect>,
}

impl HighlightTrace {
    /// Cover `points[from..]` (and the segment into them) of `h`.
    fn cover(&mut self, map: &PixelMap, h: &HighlightOp, from: usize) {
        let pts: Vec<(f32, f32)> = h.points[from.saturating_sub(1)..]
            .iter()
            .map(|&p| {
                let (x, y) = map.to_device(p);
                (x as f32, y as f32)
            })
            .collect();
        let Some(&(x0, y0)) = pts.first() else { return };
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        for &(x, y) in &pts[1..] {
            pb.line_to(x, y);
        }
        if pts.len() == 1 {
            pb.line_to(x0 + 0.01, y0);
        }
        let width = (h.width * map.scale()) as f32;
        let outline = pb
            .finish()
            .and_then(|p| p.stroke(&round_stroke(width), 1.0));
        if let Some(outline) = outline {
            let t = Transform::from_translate(-self.origin.x as f32, -self.origin.y as f32);
            self.coverage
                .fill_path(&outline, FillRule::Winding, true, t);
            let b = outline.bounds();
            let (ox, oy) = (self.origin.x as f32, self.origin.y as f32);
            let mut e = [
                b.left() - ox - 1.0,
                b.top() - oy - 1.0,
                b.right() - ox + 1.0,
                b.bottom() - oy + 1.0,
            ];
            if let Some(o) = self.reach {
                e = [
                    e[0].min(o.left()),
                    e[1].min(o.top()),
                    e[2].max(o.right()),
                    e[3].max(o.bottom()),
                ];
            }
            self.reach = Rect::from_ltrb(e[0], e[1], e[2], e[3]);
        }
    }
}

/// Highlight `h` over `patch`, through `cache`: when `h` only grew since
/// the cached trace, just its tail is added. Anti-aliased edges can come
/// out a little heavier than drawing it whole; `apply_op` draws it exactly.
pub fn highlight_cached(
    cache: &mut Option<HighlightTrace>,
    patch: &mut Patch,
    map: &PixelMap,
    h: &HighlightOp,
) {
    let (w, h_px) = (patch.img.width(), patch.img.height());
    let grew = cache.as_ref().is_some_and(|c| {
        c.origin == patch.origin
            && (c.coverage.width(), c.coverage.height()) == (w, h_px)
            && (c.op.rgb, c.op.width, c.op.alpha) == (h.rgb, h.width, h.alpha)
            && h.points.len() >= c.op.points.len()
            && h.points[..c.op.points.len()] == c.op.points[..]
    });
    let from = if grew {
        cache.as_ref().map_or(0, |c| c.op.points.len())
    } else {
        let Some(coverage) = Mask::new(w, h_px) else {
            return;
        };
        *cache = Some(HighlightTrace {
            op: h.clone(),
            origin: patch.origin,
            coverage,
            reach: None,
        });
        0
    };
    let Some(c) = cache.as_mut() else { return };
    if from < h.points.len() || from == 0 {
        c.cover(map, h, from);
    }
    c.op = h.clone();
    let whole = Rect::from_xywh(0.0, 0.0, w as f32, h_px as f32);
    let Some(r) = c.reach.zip(whole).and_then(|(r, w)| r.intersect(&w)) else {
        return;
    };
    if let Some(mut pm) = PixmapMut::from_bytes(patch.img.data_mut(), w, h_px) {
        pm.fill_rect(
            r,
            &highlight_paint(h),
            Transform::identity(),
            Some(&c.coverage),
        );
    }
}

fn highlight(patch: &mut Patch, map: &PixelMap, clip: PixelRect, h: &HighlightOp) {
    let pts: Vec<(f32, f32)> = h
        .points
        .iter()
        .map(|&p| {
            let (x, y) = map.to_device(p);
            (x as f32, y as f32)
        })
        .collect();
    let Some(&(x0, y0)) = pts.first() else { return };
    let paint = highlight_paint(h);
    let width = (h.width * map.scale()) as f32;
    with_pixmap(patch, clip, |pm, t, mask| {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        for &(x, y) in &pts[1..] {
            pb.line_to(x, y);
        }
        if pts.len() == 1 {
            pb.line_to(x0 + 0.01, y0);
        }
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint, &round_stroke(width), t, mask);
        }
    });
}

/// A straight line, with arrowheads (filled triangles, scaling with the
/// width) at the ends that have them. The line stops at a head's base.
fn line(patch: &mut Patch, map: &PixelMap, clip: PixelRect, l: &LineOp) {
    let (ax, ay) = map.to_device(l.a);
    let (bx, by) = map.to_device(l.b);
    let (dx, dy) = (bx - ax, by - ay);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let k = map.scale();
    let head = l.head_len() * k;
    let half = head * 0.45;
    let width = (l.width * k) as f32;
    let at_a = l.heads == Heads::Both;
    let at_b = l.heads != Heads::None;
    // Keep the shaft out of the heads; a head longer than the line shrinks.
    let head = head.min(len / if at_a && at_b { 2.0 } else { 1.0 });
    let (sa, sb) = (if at_a { head } else { 0.0 }, if at_b { head } else { 0.0 });
    let paint = ink(l.rgb);
    with_pixmap(patch, clip, |pm, t, mask| {
        let mut pb = PathBuilder::new();
        pb.move_to((ax + ux * sa) as f32, (ay + uy * sa) as f32);
        pb.line_to((bx - ux * sb) as f32, (by - uy * sb) as f32);
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint, &round_stroke(width), t, mask);
        }
        let mut heads = PathBuilder::new();
        let mut tri = |tx: f64, ty: f64, ux: f64, uy: f64| {
            let (bx, by) = (tx - ux * head, ty - uy * head);
            heads.move_to(tx as f32, ty as f32);
            heads.line_to((bx - uy * half) as f32, (by + ux * half) as f32);
            heads.line_to((bx + uy * half) as f32, (by - ux * half) as f32);
            heads.close();
        };
        if at_b {
            tri(bx, by, ux, uy);
        }
        if at_a {
            tri(ax, ay, -ux, -uy);
        }
        if let Some(path) = heads.finish() {
            pm.fill_path(&path, &paint, FillRule::Winding, t, mask);
        }
    });
}

fn shape(patch: &mut Patch, map: &PixelMap, clip: PixelRect, sh: &ShapeOp) {
    let (x0, y0) = map.to_device(screenie_core::geometry::LogicalPoint::new(
        sh.rect.x, sh.rect.y,
    ));
    let (x1, y1) = map.to_device(screenie_core::geometry::LogicalPoint::new(
        sh.rect.right(),
        sh.rect.bottom(),
    ));
    let (w, h) = ((x1 - x0).max(1e-3), (y1 - y0).max(1e-3));
    let Some(r) = Rect::from_xywh(x0 as f32, y0 as f32, w as f32, h as f32) else {
        return;
    };
    let path = match sh.kind {
        ShapeKind::Rect => Some(PathBuilder::from_rect(r)),
        ShapeKind::Circle => PathBuilder::from_oval(r),
    };
    let Some(path) = path else { return };
    let paint = ink(sh.rgb);
    let width = (sh.width * map.scale()) as f32;
    with_pixmap(patch, clip, |pm, t, mask| {
        pm.stroke_path(&path, &paint, &round_stroke(width), t, mask);
    });
}

fn censor(patch: &mut Patch, map: &PixelMap, clip: PixelRect, c: &CensorOp) {
    let rect = map.to_pixels(c.rect);
    let area = intersect(intersect(rect, clip), patch.extent());
    if area.is_empty() || rect.is_empty() {
        return;
    }
    match c.kind {
        CensorKind::Bar => {
            let v = (c.amount.clamp(0.0, 1.0) * 255.0).round() as u8;
            for y in area.y..area.y + area.h {
                let i = patch.offset(area.x, y);
                for px in patch.img.data_mut()[i..i + area.w as usize * 4].chunks_exact_mut(4) {
                    px.copy_from_slice(&[v, v, v, 255]);
                }
            }
        }
        CensorKind::Pixelate => {
            // Blocks of the op's size, aligned to its rect: each the mean
            // of what's under it so far.
            let block = (c.amount * map.scale()).max(2.0);
            let cols = (f64::from(rect.w) / block).ceil().max(1.0) as i32;
            let rows = (f64::from(rect.h) / block).ceil().max(1.0) as i32;
            let edge = |start: i32, len: i32, n: i32, i: i32| start + (len * i + n / 2) / n;
            for j in 0..rows {
                let (y0, y1) = (
                    edge(rect.y, rect.h, rows, j),
                    edge(rect.y, rect.h, rows, j + 1),
                );
                for i in 0..cols {
                    let (x0, x1) = (
                        edge(rect.x, rect.w, cols, i),
                        edge(rect.x, rect.w, cols, i + 1),
                    );
                    let cell = PixelRect::new(x0, y0, x1 - x0, y1 - y0);
                    let src = intersect(cell, patch.extent());
                    if src.is_empty() {
                        continue;
                    }
                    let mut sum = [0u64; 4];
                    for y in src.y..src.y + src.h {
                        let k = patch.offset(src.x, y);
                        for px in patch.img.data()[k..k + src.w as usize * 4].chunks_exact(4) {
                            for c in 0..4 {
                                sum[c] += u64::from(px[c]);
                            }
                        }
                    }
                    let n = (src.w * src.h) as u64;
                    let mean = sum.map(|s| ((s + n / 2) / n) as u8);
                    let dst = intersect(cell, area);
                    for y in dst.y..dst.y + dst.h {
                        let k = patch.offset(dst.x, y);
                        for px in
                            patch.img.data_mut()[k..k + dst.w as usize * 4].chunks_exact_mut(4)
                        {
                            px.copy_from_slice(&mean);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rgb;
    use screenie_core::geometry::LogicalRect;
    use screenie_core::model::{LayerId, Op, OpId};

    fn map() -> PixelMap {
        PixelMap::new(LogicalRect::new(0.0, 0.0, 100.0, 100.0), 100, 100)
    }

    fn censor_op(kind: CensorKind, amount: f64, r: LogicalRect) -> OpData {
        OpData::Censor(CensorOp {
            rect: r,
            kind,
            amount,
        })
    }

    #[test]
    fn a_bar_fills_its_rect_inside_the_clip_only() {
        let mut img = Image::filled(100, 100, Rgb(10, 20, 30));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let clip = PixelRect::new(0, 0, 50, 100);
        apply_op(
            &mut p,
            &map(),
            clip,
            &censor_op(
                CensorKind::Bar,
                1.0,
                LogicalRect::new(40.0, 40.0, 20.0, 20.0),
            ),
        );
        assert_eq!(img.rgb(PixelPoint::new(45, 45)), Some(Rgb(255, 255, 255)));
        assert_eq!(img.rgb(PixelPoint::new(55, 45)), Some(Rgb(10, 20, 30))); // clipped
        assert_eq!(img.rgb(PixelPoint::new(39, 45)), Some(Rgb(10, 20, 30)));
    }

    #[test]
    fn pixelate_averages_blocks() {
        let mut img = Image::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let v = if x % 2 == 0 { 0 } else { 200 };
                let i = (y * 100 + x) * 4;
                img.data_mut()[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let clip = PixelRect::new(0, 0, 100, 100);
        apply_op(
            &mut p,
            &map(),
            clip,
            &censor_op(
                CensorKind::Pixelate,
                10.0,
                LogicalRect::new(0.0, 0.0, 40.0, 40.0),
            ),
        );
        assert_eq!(img.rgb(PixelPoint::new(3, 3)), Some(Rgb(100, 100, 100)));
        assert_eq!(img.rgb(PixelPoint::new(39, 39)), Some(Rgb(100, 100, 100)));
        assert_eq!(img.rgb(PixelPoint::new(41, 3)), Some(Rgb(200, 200, 200))); // outside: untouched
    }

    #[test]
    fn layers_apply_in_order_and_ops_see_what_came_before() {
        let mut img = Image::filled(100, 100, Rgb(0, 0, 0));
        let layer = Layer {
            id: LayerId(1),
            clip: LogicalRect::new(0.0, 0.0, 100.0, 100.0),
            hidden: false,
            ops: vec![
                Op {
                    id: OpId(2),
                    data: censor_op(CensorKind::Bar, 1.0, LogicalRect::new(0.0, 0.0, 10.0, 20.0)),
                },
                // A pixelate over half white, half black: grey.
                Op {
                    id: OpId(3),
                    data: censor_op(
                        CensorKind::Pixelate,
                        20.0,
                        LogicalRect::new(0.0, 0.0, 20.0, 20.0),
                    ),
                },
            ],
        };
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        apply_layer(&mut p, &map(), &layer);
        assert_eq!(img.rgb(PixelPoint::new(15, 5)), Some(Rgb(128, 128, 128)));
    }

    fn red() -> screenie_core::model::Rgb {
        [1.0, 0.0, 0.0]
    }

    #[test]
    fn a_stroke_is_drawn_along_its_points_and_clipped() {
        use screenie_core::geometry::LogicalPoint;
        let mut img = Image::filled(100, 100, Rgb(0, 0, 0));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Stroke(StrokeOp {
            points: vec![LogicalPoint::new(10.0, 50.0), LogicalPoint::new(90.0, 50.0)],
            rgb: red(),
            width: 4.0,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 60, 100), &op);
        assert_eq!(img.rgb(PixelPoint::new(30, 50)), Some(Rgb(255, 0, 0)));
        assert_eq!(img.rgb(PixelPoint::new(30, 40)), Some(Rgb(0, 0, 0)));
        assert_eq!(img.rgb(PixelPoint::new(70, 50)), Some(Rgb(0, 0, 0))); // clipped
    }

    #[test]
    fn a_click_is_a_dot() {
        use screenie_core::geometry::LogicalPoint;
        let mut img = Image::filled(100, 100, Rgb(0, 0, 0));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Stroke(StrokeOp {
            points: vec![LogicalPoint::new(50.0, 50.0)],
            rgb: red(),
            width: 6.0,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        assert_eq!(img.rgb(PixelPoint::new(50, 50)), Some(Rgb(255, 0, 0)));
        assert_eq!(img.rgb(PixelPoint::new(50, 56)), Some(Rgb(0, 0, 0)));
    }

    #[test]
    fn shapes_outline_their_rect() {
        let mut img = Image::filled(100, 100, Rgb(0, 0, 0));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let rect = LogicalRect::new(20.0, 20.0, 60.0, 60.0);
        for kind in [ShapeKind::Rect, ShapeKind::Circle] {
            let op = OpData::Shape(ShapeOp {
                kind,
                rect,
                rgb: red(),
                width: 4.0,
            });
            apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        }
        assert_eq!(img.rgb(PixelPoint::new(50, 20)), Some(Rgb(255, 0, 0))); // both pass here
        assert_eq!(img.rgb(PixelPoint::new(21, 21)), Some(Rgb(255, 0, 0))); // the rect's corner
        assert_eq!(img.rgb(PixelPoint::new(36, 36)), Some(Rgb(0, 0, 0))); // inside both: untouched
        assert_eq!(img.rgb(PixelPoint::new(50, 50)), Some(Rgb(0, 0, 0)));
    }

    use crate::image::Rgb as Rgb3;

    fn filtered(kind: FilterKind, amount: f64, c: Rgb3) -> Rgb3 {
        let mut img = Image::filled(10, 10, c);
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Filter(FilterOp {
            rect: LogicalRect::new(0.0, 0.0, 10.0, 10.0),
            kind,
            amount,
            muted: false,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        img.rgb(PixelPoint::new(5, 5)).unwrap()
    }

    #[test]
    fn filters_per_pixel() {
        let c = Rgb3(200, 100, 50);
        let l = (0.3 * 200.0 + 0.59 * 100.0 + 0.11 * 50.0_f64).round() as u8;
        assert_eq!(filtered(FilterKind::Grey, 1.0, c), Rgb3(l, l, l));
        assert_eq!(filtered(FilterKind::Invert, 1.0, c), Rgb3(55, 155, 205));
        assert_eq!(
            filtered(FilterKind::Brightness, 2.0, c),
            Rgb3(251, 151, 101)
        );
        assert_eq!(filtered(FilterKind::Brightness, -5.0, c), Rgb3(100, 50, 25));
        // Contrast: the S-curve pushes away from mid-grey; less pulls toward it.
        let more = filtered(FilterKind::Contrast, 1.0, c);
        assert!(more.0 > 200 && more.2 < 50, "{more:?}");
        assert_eq!(filtered(FilterKind::Contrast, -5.0, c), Rgb3(128, 128, 128));
        // A flat colour stays flat under blur.
        assert_eq!(filtered(FilterKind::Blur, 4.0, c), c);
    }

    #[test]
    fn blur_spreads_an_edge_and_stays_in_its_rect() {
        let mut img = Image::filled(40, 10, Rgb3(0, 0, 0));
        for y in 0..10 {
            for x in 20..40 {
                let i = (y * 40 + x) * 4;
                img.data_mut()[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Filter(FilterOp {
            rect: LogicalRect::new(10.0, 0.0, 20.0, 10.0),
            kind: FilterKind::Blur,
            amount: 6.0,
            muted: false,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        let v = |x| img.rgb(PixelPoint::new(x, 5)).unwrap().0;
        assert!(v(19) > 40 && v(20) < 215, "{} {}", v(19), v(20)); // the edge softened
        assert_eq!(v(9), 0); // outside the rect: untouched
        assert_eq!(v(30), 255);
    }

    #[test]
    fn a_muted_filter_does_nothing() {
        let mut img = Image::filled(10, 10, Rgb3(10, 20, 30));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Filter(FilterOp {
            rect: LogicalRect::new(0.0, 0.0, 10.0, 10.0),
            kind: FilterKind::Invert,
            amount: 1.0,
            muted: true,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        assert_eq!(img.rgb(PixelPoint::new(5, 5)), Some(Rgb3(10, 20, 30)));
    }

    #[test]
    fn an_arrow_has_a_head_at_its_end_only() {
        use screenie_core::geometry::LogicalPoint;
        let mut img = Image::filled(100, 100, Rgb3(0, 0, 0));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Line(LineOp {
            a: LogicalPoint::new(10.0, 50.0),
            b: LogicalPoint::new(90.0, 50.0),
            rgb: red(),
            width: 2.0,
            heads: Heads::End,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        let v = |x, y| img.rgb(PixelPoint::new(x, y)).unwrap().0;
        assert_eq!(v(50, 50), 255); // the shaft
        assert_eq!(v(50, 54), 0); // thin
        assert_eq!(v(81, 52), 255); // the head is wide near its base
        assert_eq!(v(14, 53), 0); // no head at the tail
        assert_eq!(v(95, 50), 0); // nothing past the tip
    }

    #[test]
    fn a_highlight_tints_light_and_keeps_dark_dark() {
        use screenie_core::geometry::LogicalPoint;
        let mut img = Image::filled(100, 100, Rgb3(255, 255, 255));
        for x in 0..100 {
            let i = (50 * 100 + x) * 4;
            img.data_mut()[i..i + 3].copy_from_slice(&[0, 0, 0]); // a dark line of "text"
        }
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        let op = OpData::Highlight(HighlightOp {
            points: vec![LogicalPoint::new(10.0, 50.0), LogicalPoint::new(90.0, 50.0)],
            rgb: [1.0, 1.0, 0.0],
            width: 10.0,
            alpha: 0.4,
        });
        apply_op(&mut p, &map(), PixelRect::new(0, 0, 100, 100), &op);
        let px = |x, y| img.rgb(PixelPoint::new(x, y)).unwrap();
        let tinted = px(50, 48);
        assert!(
            tinted.0 == 255 && tinted.2 < 200 && tinted.2 > 100,
            "{tinted:?}"
        ); // yellowish, not opaque
        assert_eq!(px(50, 50), Rgb3(0, 0, 0)); // the text stays black
        assert_eq!(px(50, 40), Rgb3(255, 255, 255)); // outside the trace
    }

    #[test]
    fn a_patch_offset_in_the_frame() {
        let mut img = Image::filled(20, 20, Rgb(0, 0, 0));
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(50, 50),
        };
        let clip = PixelRect::new(0, 0, 100, 100);
        apply_op(
            &mut p,
            &map(),
            clip,
            &censor_op(CensorKind::Bar, 1.0, LogicalRect::new(55.0, 55.0, 5.0, 5.0)),
        );
        assert_eq!(img.rgb(PixelPoint::new(5, 5)), Some(Rgb(255, 255, 255)));
        assert_eq!(img.rgb(PixelPoint::new(4, 5)), Some(Rgb(0, 0, 0)));
    }

    #[test]
    fn blur_matches_a_direct_box_blur_to_a_level() {
        let (w, h, r) = (37usize, 23usize, 3i32);
        let mut img = Image::filled(w as u32, h as u32, Rgb3(0, 0, 0));
        let mut seed = 7u32;
        for v in img.data_mut().iter_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (seed >> 24) as u8;
        }
        // The reference: three direct box passes each way, in f64.
        let mut want: Vec<f64> = img.data().iter().map(|&v| f64::from(v)).collect();
        let at = |x: i32, y: i32| {
            (y.clamp(0, h as i32 - 1) as usize * w + x.clamp(0, w as i32 - 1) as usize) * 4
        };
        for _ in 0..3 {
            for horizontal in [true, false] {
                let from = want.clone();
                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        for k in 0..4 {
                            let sum: f64 = (-r..=r)
                                .map(|d| {
                                    from[if horizontal {
                                        at(x + d, y)
                                    } else {
                                        at(x, y + d)
                                    } + k]
                                })
                                .sum();
                            want[at(x, y) + k] = sum / f64::from(2 * r + 1);
                        }
                    }
                }
            }
        }
        let mut p = Patch {
            img: &mut img,
            origin: PixelPoint::new(0, 0),
        };
        blur(
            &mut p,
            PixelRect::new(0, 0, w as i32, h as i32),
            f64::from(2 * r),
        );
        let worst = img
            .data()
            .iter()
            .zip(&want)
            .map(|(&got, &want)| (f64::from(got) - want).abs())
            .fold(0.0, f64::max);
        assert!(worst <= 1.0, "off by {worst}");
    }
}
