//! Rendering operations onto pixels. Core defines the operations as data;
//! this is the only place that knows what they look like.
//!
//! Everything works on a patch: an image whose top-left sits at `origin`
//! in frame pixels. A layer never touches pixels outside its clip.

use screenie_core::geometry::{PixelMap, PixelPoint, PixelRect};
use screenie_core::model::{CensorKind, CensorOp, Layer, OpData};

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
    }
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
