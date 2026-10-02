//! Rendering operations onto pixels. Core defines the operations as data;
//! this is the only place that knows what they look like.
//!
//! Everything works on a patch: an image whose top-left sits at `origin`
//! in frame pixels. A layer never touches pixels outside its clip.

use screenie_core::geometry::{PixelMap, PixelPoint, PixelRect};
use screenie_core::model::{
    CensorKind, CensorOp, FilterKind, FilterOp, Layer, OpData, Rgb, ShapeKind, ShapeOp, StrokeOp,
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
    let mut buf: Vec<[f32; 4]> = Vec::with_capacity(w * h);
    for y in src.y..src.y + src.h {
        let i = patch.offset(src.x, y);
        for px in patch.img.data()[i..i + w * 4].chunks_exact(4) {
            buf.push([0, 1, 2, 3].map(|k| f32::from(px[k])));
        }
    }
    let mut tmp = buf.clone();
    let pass = |from: &[[f32; 4]], to: &mut [[f32; 4]], horizontal: bool| {
        let (n, lines) = if horizontal { (w, h) } else { (h, w) };
        let at = |line: usize, i: usize| {
            if horizontal {
                line * w + i
            } else {
                i * w + line
            }
        };
        let norm = 1.0 / (2 * r + 1) as f32;
        for line in 0..lines {
            for i in 0..n {
                let mut acc = [0.0f32; 4];
                for d in -r..=r {
                    let j = (i as i32 + d).clamp(0, n as i32 - 1) as usize;
                    let p = from[at(line, j)];
                    for k in 0..4 {
                        acc[k] += p[k];
                    }
                }
                to[at(line, i)] = acc.map(|v| v * norm);
            }
        }
    };
    for _ in 0..3 {
        pass(&buf, &mut tmp, true);
        pass(&tmp, &mut buf, false);
    }
    for y in area.y..area.y + area.h {
        let i = patch.offset(area.x, y);
        let row = (y - src.y) as usize * w + (area.x - src.x) as usize;
        for (n, px) in patch.img.data_mut()[i..i + area.w as usize * 4]
            .chunks_exact_mut(4)
            .enumerate()
        {
            let v = buf[row + n];
            for k in 0..4 {
                px[k] = v[k].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
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
}
