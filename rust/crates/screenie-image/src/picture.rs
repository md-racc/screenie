//! What the picker shows and outputs, from the immutable capture and the
//! project: the committed picture (every visible layer applied in order)
//! and, while a layer is open, its view over the layer's clip.
//!
//! Everything here is a cache. Throwing it away changes nothing: `sync`
//! rebuilds what's needed from the capture and the layers.

use screenie_core::geometry::{LogicalRect, PixelPoint, PixelRect};
use screenie_core::model::{Layer, Op, OpData};

use crate::frame::Frame;
use crate::image::{Image, Rgb};
use crate::render::{Patch, apply_layer, apply_op, contains, intersect};

pub struct Picture {
    base: Frame,
    committed: Frame,
    /// The layers `committed` was made from.
    made_from: Vec<Layer>,
    version: u64,
    /// The open layer's clip and locked ops, applied over the committed
    /// picture in that clip.
    locked: Option<(LogicalRect, Vec<Op>, PixelRect, Image)>,
    /// The same, plus the item being edited: what's shown.
    shown: Option<(Option<OpData>, Image)>,
    /// The open layer's prefixes: the clip patch after its first n ops, so
    /// changing the top filter doesn't re-run the ones under it.
    prefixes: Vec<(Vec<Op>, Image)>,
    /// Open-layer ops applied so far (to measure the cache).
    applied: usize,
    /// The layer the Layers view shows alone: the capture with only it
    /// applied, over its clip.
    isolated: Option<(Layer, PixelRect, Image)>,
    /// Bumps whenever what's shown changes (for caches of it).
    generation: u64,
    /// A highlighter trace in progress, so its frames add only its tail.
    highlight: Option<crate::render::HighlightTrace>,
}

/// OCR upscales frames under this scale.
pub const OCR_SCALE: f64 = 1.5;

/// Open-layer prefixes kept.
pub const PREFIXES: usize = 12;

impl Picture {
    pub fn new(base: Frame) -> Self {
        Self {
            committed: base.clone(),
            base,
            made_from: Vec::new(),
            version: 0,
            locked: None,
            shown: None,
            prefixes: Vec::new(),
            applied: 0,
            isolated: None,
            generation: 0,
            highlight: None,
        }
    }

    /// Bumps whenever what's shown changes.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What's shown over a rectangle: the committed picture with the open
    /// layer's view over its clip.
    pub fn shown_crop(&self, r: PixelRect) -> Image {
        let mut img = self.committed.image().crop(r);
        if let Some((pr, patch)) = self.open_patch() {
            img.blit(patch, patch.extent(), pr.x - r.x, pr.y - r.y);
        }
        img
    }

    /// The layer the Layers view is inspecting, if any.
    pub fn inspect(&mut self, layer: Option<&Layer>) {
        let Some(layer) = layer else {
            self.isolated = None;
            return;
        };
        if self.isolated.as_ref().is_some_and(|(l, ..)| l == layer) {
            return;
        }
        let map = *self.base.map();
        let r = intersect(map.to_pixels(layer.clip), self.base.image().extent());
        let mut img = self.base.image().crop(r);
        let mut shown = layer.clone();
        shown.hidden = false; // inspecting a hidden layer still shows it
        apply_layer(
            &mut Patch {
                img: &mut img,
                origin: PixelPoint::new(r.x, r.y),
            },
            &map,
            &shown,
        );
        self.isolated = Some((layer.clone(), r, img));
    }

    /// The inspected layer alone, over its clip (frame px).
    pub fn isolated_patch(&self) -> Option<(PixelRect, &Image)> {
        self.isolated.as_ref().map(|(_, r, img)| (*r, img))
    }

    /// How many open-layer ops have been applied, all told.
    pub fn ops_applied(&self) -> usize {
        self.applied
    }

    /// The capture, untouched.
    pub fn base(&self) -> &Frame {
        &self.base
    }

    /// The committed picture.
    pub fn committed(&self) -> &Frame {
        &self.committed
    }

    /// Bumps whenever the committed picture changes.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Bring the caches up to date with the session.
    pub fn sync(&mut self, layers: &[Layer], open: Option<&Layer>, editing: Option<&OpData>) {
        if layers != self.made_from.as_slice() {
            let map = *self.base.map();
            let mut img = self.base.image().clone();
            for layer in layers.iter().filter(|l| !l.hidden) {
                apply_layer(
                    &mut Patch {
                        img: &mut img,
                        origin: PixelPoint::new(0, 0),
                    },
                    &map,
                    layer,
                );
            }
            self.committed = Frame::new(img, self.base.layout());
            self.made_from = layers.to_vec();
            self.version += 1;
            self.generation += 1;
            self.locked = None;
            self.shown = None;
            self.prefixes.clear();
        }
        let Some(open) = open else {
            if self.locked.is_some() {
                self.generation += 1;
            }
            self.locked = None;
            self.shown = None;
            self.prefixes.clear();
            return;
        };
        let fresh = self
            .locked
            .as_ref()
            .is_some_and(|(clip, ops, _, _)| *clip == open.clip && *ops == open.ops);
        if !fresh {
            let map = *self.committed.map();
            let clip = map.to_pixels(open.clip);
            let r = intersect(clip, self.committed.image().extent());
            if self
                .locked
                .as_ref()
                .is_some_and(|(clip, ..)| *clip != open.clip)
            {
                self.prefixes.clear();
            }
            // Keep the prefixes still true of the ops, and start from the
            // longest.
            self.prefixes
                .retain(|(ops, _)| open.ops.len() >= ops.len() && open.ops[..ops.len()] == ops[..]);
            let (start, mut img) = self
                .prefixes
                .iter()
                .max_by_key(|(ops, _)| ops.len())
                .map_or_else(
                    || (0, self.committed.image().crop(r)),
                    |(ops, img)| (ops.len(), img.clone()),
                );
            for i in start..open.ops.len() {
                apply_op(
                    &mut Patch {
                        img: &mut img,
                        origin: PixelPoint::new(r.x, r.y),
                    },
                    &map,
                    clip,
                    &open.ops[i].data,
                );
                self.applied += 1;
                // Only the longest prefixes are kept: copy only those.
                if open.ops.len() - i <= PREFIXES {
                    self.prefixes.push((open.ops[..=i].to_vec(), img.clone()));
                }
            }
            let extra = self.prefixes.len().saturating_sub(PREFIXES);
            self.prefixes.drain(..extra);
            self.locked = Some((open.clip, open.ops.clone(), r, img));
            self.shown = None;
        }
        let editing = editing.cloned();
        if self.shown.as_ref().is_some_and(|(e, _)| *e == editing) {
            return;
        }
        let (_, _, r, locked) = self.locked.as_ref().expect("just made");
        // A pen trace that only grew since the last frame: draw just its new
        // tail over what's shown, rather than the whole trace over a fresh
        // copy (that grows with the line). Opaque ink looks the same either
        // way; the full trace is rendered once, exactly, when it locks.
        if let (Some(OpData::Stroke(new)), Some((Some(OpData::Stroke(old)), shown))) =
            (&editing, self.shown.as_mut())
        {
            let grew = old.rgb == new.rgb
                && old.width == new.width
                && old.points.len() >= 2
                && new.points.len() > old.points.len()
                && new.points[..old.points.len()] == old.points[..];
            if grew {
                let map = *self.committed.map();
                let clip = map.to_pixels(open.clip);
                let tail = OpData::Stroke(screenie_core::model::StrokeOp {
                    points: new.points[old.points.len() - 1..].to_vec(),
                    ..old.clone()
                });
                apply_op(
                    &mut Patch {
                        img: shown,
                        origin: PixelPoint::new(r.x, r.y),
                    },
                    &map,
                    clip,
                    &tail,
                );
                if let Some((e, _)) = self.shown.as_mut() {
                    *e = editing;
                }
                self.generation += 1;
                return;
            }
        }
        let mut img = locked.clone();
        let map = *self.committed.map();
        let mut patch = Patch {
            img: &mut img,
            origin: PixelPoint::new(r.x, r.y),
        };
        match &editing {
            // Its coverage is kept, and grows by the tail (ink, above,
            // simply draws over the last frame).
            Some(OpData::Highlight(h)) => {
                crate::render::highlight_cached(&mut self.highlight, &mut patch, &map, h);
            }
            Some(op) => {
                self.highlight = None;
                apply_op(&mut patch, &map, map.to_pixels(open.clip), op);
            }
            None => self.highlight = None,
        }
        self.shown = Some((editing, img));
        self.generation += 1;
    }

    /// The open layer's view over its clip (frame px), if a layer is open.
    pub fn open_patch(&self) -> Option<(PixelRect, &Image)> {
        let (_, _, r, _) = self.locked.as_ref()?;
        let (_, img) = self.shown.as_ref()?;
        Some((*r, img))
    }

    /// What's shown at a pixel: the open layer's view inside its clip, else
    /// the committed picture.
    pub fn rgb(&self, p: PixelPoint) -> Option<Rgb> {
        if let Some((r, img)) = self.open_patch()
            && contains(r, p)
        {
            return img.rgb(PixelPoint::new(p.x - r.x, p.y - r.y));
        }
        self.committed.rgb(p)
    }

    /// What OCR reads for a box: the committed pixels in greyscale, dark
    /// backgrounds flipped (tesseract wants dark text on light), and
    /// doubled when the frame is under 1.5x (screen text is small). Also
    /// returns the crop's origin and its upscale, to map word boxes back.
    pub fn ocr_crop(&self, rect: LogicalRect) -> (Image, PixelPoint, u32) {
        self.ocr_crop_pixels(self.committed.map().to_pixels(rect))
    }

    /// `ocr_crop` of a rect of the picture's pixels.
    pub fn ocr_crop_pixels(&self, r: PixelRect) -> (Image, PixelPoint, u32) {
        let mut img = self.committed.image().crop(r);
        crate::view::apply(&mut img, screenie_core::lens::ViewFilter::Grey);
        if crate::view::is_dark(&img) {
            crate::view::apply(&mut img, screenie_core::lens::ViewFilter::Invert);
        }
        let up = if self.committed.map().scale() < OCR_SCALE {
            2
        } else {
            1
        };
        if up > 1 {
            img = crate::compose::resize(&img, img.width() * up, img.height() * up);
        }
        (img, PixelPoint::new(r.x, r.y), up)
    }

    /// The output for a box: the committed picture's pixels.
    pub fn crop(&self, rect: LogicalRect) -> Image {
        self.committed.crop(rect)
    }
}

/// The eyedropper's view of the picture: a picture of its own (the
/// capture is shared), brought up to date with the session on each read.
pub struct PictureSampler(Picture);

impl PictureSampler {
    pub fn new(base: Frame) -> Self {
        Self(Picture::new(base))
    }
}

impl screenie_core::session::Sampler for PictureSampler {
    fn sample(&mut self, s: &screenie_core::session::Session, at: PixelPoint) -> Option<[u8; 3]> {
        self.0
            .sync(s.layers(), s.open_layer(), s.editing().as_ref());
        self.0.rgb(at).map(|c| [c.0, c.1, c.2])
    }
}

/// A change in grey (0-255) this big from one pixel to the next, or from
/// two before (anti-aliased edges), is an edge.
pub const EDGE_STEP: f64 = 32.0;
pub const EDGE_STEP_SOFT: f64 = 48.0;

/// The first pixel past `from`, going `d` within `bounds`, where the grey
/// (averaged over three pixels across the ray) changes by an edge's worth.
pub fn find_edge(
    rgb: impl Fn(PixelPoint) -> Option<Rgb>,
    from: PixelPoint,
    d: screenie_core::geometry::Dir,
    bounds: PixelRect,
) -> Option<PixelPoint> {
    let (dx, dy) = d.vec();
    let (dx, dy) = (dx as i32, dy as i32);
    let inside = |q: PixelPoint| contains(bounds, q);
    let grey = |t: i32| -> Option<f64> {
        let c = PixelPoint::new(from.x + dx * t, from.y + dy * t);
        if !inside(c) {
            return None;
        }
        let (n, sum) = (-1..=1)
            .map(|k| PixelPoint::new(c.x + dy.abs() * k, c.y + dx.abs() * k))
            .filter(|&q| inside(q))
            .filter_map(&rgb)
            .fold((0.0, 0.0), |(n, s), c| {
                let l = 0.299 * f64::from(c.0) + 0.587 * f64::from(c.1) + 0.114 * f64::from(c.2);
                (n + 1.0, s + l)
            });
        (n > 0.0).then(|| sum / n)
    };
    let mut back = [grey(0)?, grey(0)?];
    for t in 1.. {
        let g = grey(t)?;
        if (g - back[1]).abs() >= EDGE_STEP || (t > 1 && (g - back[0]).abs() >= EDGE_STEP_SOFT) {
            return Some(PixelPoint::new(from.x + dx * t, from.y + dy * t));
        }
        back = [back[1], g];
    }
    None
}

/// How far (grey levels) a pixel can be from where a flood started and
/// still be part of its region.
pub const FLOOD_TOLERANCE: f64 = 40.0;

/// Measure's area by a click inside: flood out from `seed` over pixels
/// whose grey is within `FLOOD_TOLERANCE` of its own, stopped by `walls`
/// (drawn measurements, as segments) and kept to `bounds`. The region's
/// outer outline, holes and all, simplified to within half a pixel; an error
/// if it isn't closed in (it reaches `bounds`' edge).
pub fn flood_region(
    rgb: impl Fn(PixelPoint) -> Option<Rgb>,
    seed: PixelPoint,
    bounds: PixelRect,
    walls: &[(PixelPoint, PixelPoint)],
) -> Result<Vec<PixelPoint>, String> {
    let (bx, by, bw, bh) = (bounds.x, bounds.y, bounds.w, bounds.h);
    let inside = |q: PixelPoint| contains(bounds, q);
    if !inside(seed) {
        return Err("outside the monitor".into());
    }
    let ix = |q: PixelPoint| ((q.y - by) * bw + (q.x - bx)) as usize;
    const WALL: u8 = 1;
    const IN: u8 = 2;
    let mut mask = vec![0u8; (bw * bh) as usize];
    // Walls: 8-connected lines, which a 4-connected flood can't cross.
    for &(a, b) in walls {
        let (mut x, mut y) = (a.x, a.y);
        let (dx, dy) = ((b.x - a.x).abs(), -(b.y - a.y).abs());
        let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
        let mut err = dx + dy;
        loop {
            let q = PixelPoint::new(x, y);
            if inside(q) {
                mask[ix(q)] = WALL;
            }
            if x == b.x && y == b.y {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    let grey = |q: PixelPoint| {
        rgb(q).map(|c| 0.299 * f64::from(c.0) + 0.587 * f64::from(c.1) + 0.114 * f64::from(c.2))
    };
    if mask[ix(seed)] == WALL {
        return Err("that's on a line: click inside".into());
    }
    let g0 = grey(seed).ok_or("no picture there")?;
    mask[ix(seed)] = IN;
    let mut todo = vec![seed];
    while let Some(q) = todo.pop() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = PixelPoint::new(q.x + dx, q.y + dy);
            if !inside(n) {
                return Err("not closed in: draw lines to close it".into());
            }
            let k = ix(n);
            if mask[k] != 0 {
                continue;
            }
            if grey(n).is_some_and(|g| (g - g0).abs() <= FLOOD_TOLERANCE) {
                mask[k] = IN;
                todo.push(n);
            }
        }
    }
    // Drawn walls the region touches are its edge, so it measures to the
    // lines (not the pixels just inside them).
    const EDGE: u8 = 3;
    // (diagonally too: a corner of the walls is on the edge)
    const AROUND: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    for y in by..by + bh {
        for x in bx..bx + bw {
            let q = PixelPoint::new(x, y);
            if mask[ix(q)] != WALL {
                continue;
            }
            let touches = AROUND.iter().any(|&(dx, dy)| {
                let n = PixelPoint::new(x + dx, y + dy);
                inside(n) && mask[ix(n)] == IN
            });
            if touches {
                mask[ix(q)] = EDGE;
            }
        }
    }
    let filled = |q: PixelPoint| inside(q) && matches!(mask[ix(q)], IN | EDGE);
    // The top-left-most filled pixel (an edge one, maybe), to trace from.
    let start = (by..by + bh)
        .flat_map(|y| (bx..bx + bw).map(move |x| PixelPoint::new(x, y)))
        .find(|&q| filled(q))
        .expect("the seed is filled");
    // Within half a pixel: smooths a staircase, keeps a real edge.
    let outline = trace_outline(start, filled, 8 * mask.len());
    Ok(simplify_ring(&outline, 0.5))
}

/// The outer boundary of an 8-connected region, clockwise on screen, by
/// Moore-neighbour tracing from its top-left-most pixel; at most `max_steps`
/// long.
fn trace_outline(
    start: PixelPoint,
    filled: impl Fn(PixelPoint) -> bool,
    max_steps: usize,
) -> Vec<PixelPoint> {
    const RING: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let at = |p: PixelPoint, i: usize| PixelPoint::new(p.x + RING[i].0, p.y + RING[i].1);
    let index = |p: PixelPoint, q: PixelPoint| {
        RING.iter()
            .position(|&(dx, dy)| (q.x - p.x, q.y - p.y) == (dx, dy))
            .expect("neighbours")
    };
    let mut out = vec![start];
    let (mut p, mut b) = (start, at(start, 4)); // came from the west: never filled
    let first_b = b;
    // A safety net: each step is a pixel and the way it was entered, of
    // eight, and the walk is deterministic, so a longer one is going round.
    for _ in 0..max_steps {
        let ib = index(p, b);
        let mut next = None;
        for k in 1..=8 {
            let c = at(p, (ib + k) % 8);
            if filled(c) {
                next = Some((c, at(p, (ib + k - 1) % 8)));
                break;
            }
        }
        let Some((c, prev)) = next else {
            break; // a single pixel
        };
        // Jacob's stop: back at the start, entered the way it first left
        // (a region through its start twice goes on round).
        if c == start && prev == first_b {
            break;
        }
        out.push(c);
        p = c;
        b = prev;
    }
    out
}

/// A closed ring with points dropped while it stays within `eps` of the
/// original (Douglas-Peucker, split at the point farthest from the first).
fn simplify_ring(ring: &[PixelPoint], eps: f64) -> Vec<PixelPoint> {
    fn dp(pts: &[PixelPoint], eps: f64, out: &mut Vec<PixelPoint>) {
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let (ax, ay, bx, by) = (
            f64::from(a.x),
            f64::from(a.y),
            f64::from(b.x),
            f64::from(b.y),
        );
        let len = (bx - ax).hypot(by - ay);
        let d = |q: &PixelPoint| {
            let (qx, qy) = (f64::from(q.x), f64::from(q.y));
            if len == 0.0 {
                (qx - ax).hypot(qy - ay)
            } else {
                ((bx - ax) * (ay - qy) - (ax - qx) * (by - ay)).abs() / len
            }
        };
        let (k, far) = pts
            .iter()
            .enumerate()
            .skip(1)
            .take(pts.len().saturating_sub(2))
            .map(|(i, q)| (i, d(q)))
            .fold((0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
        if far > eps {
            dp(&pts[..=k], eps, out);
            dp(&pts[k..], eps, out);
        } else {
            out.push(a);
        }
    }
    if ring.len() < 4 {
        return ring.to_vec();
    }
    // Split at the point farthest from the first, so both halves are open.
    let f = ring[0];
    let far = (1..ring.len())
        .max_by_key(|&i| (ring[i].x - f.x).pow(2) + (ring[i].y - f.y).pow(2))
        .expect("points");
    let mut out = Vec::new();
    dp(&ring[..=far], eps, &mut out);
    let mut back: Vec<PixelPoint> = ring[far..].to_vec();
    back.push(ring[0]);
    dp(&back, eps, &mut out);
    out
}

/// Measure's edges, over the committed picture.
pub struct PictureEdges(Picture);

impl PictureEdges {
    pub fn new(base: Frame) -> Self {
        Self(Picture::new(base))
    }
}

impl screenie_core::measure::EdgeFinder for PictureEdges {
    fn edge(
        &mut self,
        s: &screenie_core::session::Session,
        from: PixelPoint,
        d: screenie_core::geometry::Dir,
        bounds: PixelRect,
    ) -> Option<PixelPoint> {
        self.0.sync(s.layers(), None, None);
        let pic = &self.0;
        find_edge(|q| pic.rgb(q), from, d, bounds)
    }

    fn region(
        &mut self,
        s: &screenie_core::session::Session,
        seed: PixelPoint,
        bounds: PixelRect,
        walls: &[(PixelPoint, PixelPoint)],
    ) -> Result<Vec<PixelPoint>, String> {
        self.0.sync(s.layers(), None, None);
        let pic = &self.0;
        flood_region(|q| pic.rgb(q), seed, bounds, walls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::model::{CensorKind, CensorOp, LayerId, OpId};

    const ALL: LogicalRect = LogicalRect::new(0.0, 0.0, 100.0, 50.0);

    #[test]
    fn a_growing_trace_draws_its_tail_and_looks_like_the_whole() {
        use screenie_core::geometry::LogicalPoint;
        use screenie_core::model::StrokeOp;
        let open = layer(1, vec![]);
        let pts: Vec<LogicalPoint> = (0..60)
            .map(|i| {
                let t = f64::from(i) * 0.1;
                LogicalPoint::new(50.0 + 30.0 * t.cos(), 25.0 + 15.0 * (2.0 * t).sin())
            })
            .collect();
        let op = |n: usize| {
            OpData::Stroke(StrokeOp {
                points: pts[..n].to_vec(),
                rgb: [0.9, 0.2, 0.1],
                width: 3.0,
            })
        };
        let mut live = picture();
        for n in 2..=60 {
            live.sync(&[], Some(&open), Some(&op(n)));
        }
        let mut whole = picture();
        whole.sync(&[], Some(&open), Some(&op(60)));
        let (a, b) = (live.open_patch().unwrap().1, whole.open_patch().unwrap().1);
        let worst = a
            .data()
            .iter()
            .zip(b.data())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap();
        // Only anti-aliased edge pixels differ, none by a quarter.
        assert!(worst < 64, "worst channel difference {worst}");
        let off = |k: u8| {
            a.data()
                .iter()
                .zip(b.data())
                .filter(|(x, y)| x.abs_diff(**y) > k)
                .count()
        };
        // The rest of the line's edges are a little heavier in places.
        assert!(
            off(8) * 50 < a.data().len(),
            "{} bytes off by more than 8",
            off(8)
        );
        // A change other than growth (a new ink) draws it whole again.
        live.sync(&[], Some(&open), None);
        assert_eq!(live.open_patch().unwrap().1.data(), picture_patch(&open));
    }

    #[test]
    fn a_growing_highlight_adds_its_tail_and_looks_like_the_whole() {
        use screenie_core::geometry::LogicalPoint;
        use screenie_core::model::HighlightOp;
        let open = layer(1, vec![]);
        // Over a light block, where multiplying shows.
        let mut light = Image::filled(100, 50, Rgb(230, 230, 230));
        light.data_mut()[..4].copy_from_slice(&[10, 10, 10, 255]);
        let pic = || Picture::new(Frame::new(light.clone(), ALL));
        let pts: Vec<LogicalPoint> = (0..80)
            .map(|i| {
                let t = f64::from(i) * 0.08;
                LogicalPoint::new(25.0 + 18.0 * t.cos(), 25.0 + 15.0 * (2.0 * t).sin())
            })
            .collect();
        let op = |n: usize| {
            OpData::Highlight(HighlightOp {
                points: pts[..n].to_vec(),
                rgb: [1.0, 0.85, 0.1],
                width: 10.0,
                alpha: 0.4,
            })
        };
        let mut live = pic();
        for n in 1..=80 {
            live.sync(&[], Some(&open), Some(&op(n)));
        }
        let mut whole = pic();
        whole.sync(&[], Some(&open), Some(&op(80)));
        let (a, b) = (live.open_patch().unwrap().1, whole.open_patch().unwrap().1);
        let diff = |k: u8| {
            a.data()
                .iter()
                .zip(b.data())
                .filter(|(x, y)| x.abs_diff(**y) > k)
                .count()
        };
        // It overlaps itself, and still doesn't double up where it does:
        // only anti-aliased edges differ, and a little.
        assert_eq!(diff(32), 0, "{} bytes off by more than 32", diff(32));
        assert!(
            diff(4) * 50 < a.data().len(),
            "{} bytes off by more than 4",
            diff(4)
        );
    }

    fn picture_patch(open: &Layer) -> Vec<u8> {
        let mut p = picture();
        p.sync(&[], Some(open), None);
        p.open_patch().unwrap().1.data().to_vec()
    }

    fn shoelace(pts: &[PixelPoint]) -> f64 {
        let n = pts.len();
        (0..n)
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                f64::from(a.x * b.y - b.x * a.y)
            })
            .sum::<f64>()
            .abs()
            / 2.0
    }

    /// A dark 100x60 picture with light rectangles painted in.
    fn plan(rects: &[(i32, i32, i32, i32, u8)]) -> Image {
        let mut img = Image::filled(100, 60, Rgb(20, 20, 20));
        for &(x0, y0, w, h, v) in rects {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let i = ((y * 100 + x) * 4) as usize;
                    img.data_mut()[i..i + 3].copy_from_slice(&[v, v, v]);
                }
            }
        }
        img
    }

    #[test]
    fn a_flood_finds_the_region_and_its_outline() {
        let img = plan(&[(20, 10, 40, 30, 230)]);
        let all = PixelRect::new(0, 0, 100, 60);
        let out = flood_region(|q| img.rgb(q), PixelPoint::new(40, 25), all, &[]).unwrap();
        // Pixel centres round a 40x30 block: 39 by 29, four corners.
        assert_eq!(out.len(), 4, "{out:?}");
        assert_eq!(shoelace(&out), 39.0 * 29.0);
        // An L: six corners.
        let img = plan(&[(20, 10, 40, 30, 230), (20, 40, 10, 10, 230)]);
        let out = flood_region(|q| img.rgb(q), PixelPoint::new(40, 25), all, &[]).unwrap();
        // Six corners, and a half-pixel step across the inside one (the
        // outline runs through pixel centres).
        assert_eq!(out.len(), 7, "{out:?}");
        assert_eq!(shoelace(&out), 39.0 * 29.0 + 9.0 * 10.0 + 0.5);
        // A hole (furniture in a room) is inside the outline, not cut out.
        let img = plan(&[(20, 10, 40, 30, 230), (35, 20, 5, 5, 20)]);
        let out = flood_region(|q| img.rgb(q), PixelPoint::new(25, 15), all, &[]).unwrap();
        assert_eq!(shoelace(&out), 39.0 * 29.0);
    }

    #[test]
    fn drawn_lines_close_a_region_and_open_ones_are_refused() {
        let all = PixelRect::new(0, 0, 100, 60);
        let img = plan(&[]);
        let rgb = |q| img.rgb(q);
        let p = PixelPoint::new;
        // A closed path drawn on a flat picture: its inside.
        let walls = [
            (p(10, 10), p(50, 10)),
            (p(50, 10), p(50, 40)),
            (p(50, 40), p(10, 40)),
            (p(10, 40), p(10, 10)),
        ];
        let out = flood_region(rgb, p(30, 25), all, &walls).unwrap();
        assert_eq!(shoelace(&out), 40.0 * 30.0); // to the lines drawn
        // Open: refused, and saying why.
        let err = flood_region(rgb, p(30, 25), all, &walls[..3]).unwrap_err();
        assert!(err.contains("not closed in"), "{err}");
        // A room with a doorway, closed by a drawn line across it.
        let room = plan(&[(20, 10, 40, 30, 230), (59, 20, 41, 6, 230)]);
        let rgb = |q| room.rgb(q);
        assert!(
            flood_region(rgb, p(40, 25), all, &[]).is_err(),
            "out the door"
        );
        let door = [(p(59, 18), p(59, 28))];
        let out = flood_region(rgb, p(40, 25), all, &door).unwrap();
        assert_eq!(shoelace(&out), 39.0 * 29.0); // the room's pixels, to the door's line
        // On a line: refused.
        assert!(flood_region(rgb, p(59, 22), all, &door).is_err());
    }

    #[test]
    fn edges_along_a_ray() {
        use screenie_core::geometry::Dir;
        // White, a 1px dark wall at x = 40, a soft edge at 70-72, then grey.
        let mut img = Image::filled(100, 50, Rgb(250, 250, 250));
        for y in 0..50 {
            for (x, v) in [(40, 20), (70, 225), (71, 200), (72, 175)] {
                let i = (y * 100 + x) * 4;
                img.data_mut()[i..i + 3].copy_from_slice(&[v, v, v]);
            }
            for x in 73..100 {
                let i = (y * 100 + x) * 4;
                img.data_mut()[i..i + 3].copy_from_slice(&[175, 175, 175]);
            }
        }
        let rgb = |q: PixelPoint| img.rgb(q);
        let all = PixelRect::new(0, 0, 100, 50);
        let at = |x| PixelPoint::new(x, 25);
        assert_eq!(find_edge(rgb, at(10), Dir::Right, all), Some(at(40))); // onto the wall
        assert_eq!(find_edge(rgb, at(40), Dir::Right, all), Some(at(41))); // off it
        assert_eq!(find_edge(rgb, at(41), Dir::Right, all), Some(at(71))); // the soft edge
        assert_eq!(find_edge(rgb, at(80), Dir::Right, all), None); // nothing more
        assert_eq!(find_edge(rgb, at(60), Dir::Left, all), Some(at(40))); // onto it, from either side
        let short = PixelRect::new(0, 0, 30, 50);
        assert_eq!(find_edge(rgb, at(10), Dir::Right, short), None); // bounded
        assert_eq!(find_edge(rgb, at(10), Dir::Up, all), None);
    }

    fn picture() -> Picture {
        Picture::new(Frame::new(Image::filled(100, 50, Rgb(10, 10, 10)), ALL))
    }

    fn bar(id: u64, x: f64) -> Op {
        Op {
            id: OpId(id),
            data: OpData::Censor(CensorOp {
                rect: LogicalRect::new(x, 0.0, 10.0, 10.0),
                kind: CensorKind::Bar,
                amount: 1.0,
            }),
        }
    }

    fn layer(id: u64, ops: Vec<Op>) -> Layer {
        Layer {
            id: LayerId(id),
            clip: LogicalRect::new(0.0, 0.0, 50.0, 50.0),
            hidden: false,
            ops,
        }
    }

    fn at(p: &Picture, x: i32) -> Rgb {
        p.rgb(PixelPoint::new(x, 5)).unwrap()
    }

    #[test]
    fn committed_layers_change_the_output_and_hidden_ones_do_not() {
        let mut p = picture();
        let mut l = layer(1, vec![bar(2, 0.0)]);
        p.sync(std::slice::from_ref(&l), None, None);
        assert_eq!(p.version(), 1);
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(255, 255, 255))
        );
        l.hidden = true;
        p.sync(std::slice::from_ref(&l), None, None);
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(10, 10, 10))
        );
        assert_eq!(p.base().rgb(PixelPoint::new(5, 5)), Some(Rgb(10, 10, 10))); // never touched
    }

    #[test]
    fn the_open_layer_is_shown_but_not_output() {
        let mut p = picture();
        let open = layer(1, vec![bar(2, 0.0)]);
        let editing = bar(3, 20.0).data;
        p.sync(&[], Some(&open), Some(&editing));
        assert_eq!(at(&p, 5), Rgb(255, 255, 255)); // locked
        assert_eq!(at(&p, 25), Rgb(255, 255, 255)); // being edited
        assert_eq!(at(&p, 45), Rgb(10, 10, 10));
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(10, 10, 10))
        );
        p.sync(&[], None, None); // discarded
        assert!(p.open_patch().is_none());
        assert_eq!(at(&p, 5), Rgb(10, 10, 10));
    }

    #[test]
    fn the_open_layer_is_clipped() {
        let mut p = picture();
        let open = layer(1, vec![]);
        p.sync(&[], Some(&open), Some(&bar(3, 45.0).data));
        assert_eq!(at(&p, 48), Rgb(255, 255, 255));
        assert_eq!(at(&p, 51), Rgb(10, 10, 10)); // past the 50px clip
    }

    #[test]
    fn changing_the_top_op_reuses_the_ones_under_it() {
        let mut p = picture();
        let mut open = layer(1, vec![bar(2, 0.0), bar(3, 10.0), bar(4, 20.0)]);
        p.sync(&[], Some(&open), None);
        assert_eq!(p.ops_applied(), 3);
        open.ops[2] = bar(4, 30.0); // the top one moves
        p.sync(&[], Some(&open), None);
        assert_eq!(p.ops_applied(), 4); // only it ran again
        assert_eq!(at(&p, 35), Rgb(255, 255, 255));
        assert_eq!(at(&p, 25), Rgb(10, 10, 10));
        open.ops.remove(0); // under everything: from scratch
        p.sync(&[], Some(&open), None);
        assert_eq!(p.ops_applied(), 6);
        assert_eq!(at(&p, 5), Rgb(10, 10, 10));
    }

    #[test]
    fn the_open_view_is_what_committing_outputs() {
        use screenie_core::model::{FilterKind, FilterOp};
        // Black left half, white right; the layer's clip is the white half,
        // so nothing it shows or outputs should darken.
        let mut img = Image::filled(100, 50, Rgb(0, 0, 0));
        let white = Image::filled(50, 50, Rgb(255, 255, 255));
        img.blit(&white, white.extent(), 50, 0);
        let clip = LogicalRect::new(50.0, 0.0, 50.0, 50.0);
        let blur = OpData::Filter(FilterOp {
            rect: clip,
            kind: FilterKind::Blur,
            amount: 8.0,
            muted: false,
        });
        // Its cells don't line up with the clip: one straddles x = 50.
        let pixelate = OpData::Censor(CensorOp {
            rect: LogicalRect::new(33.0, 0.0, 41.0, 50.0),
            kind: CensorKind::Pixelate,
            amount: 12.0,
        });
        for data in [blur, pixelate] {
            let l = Layer {
                id: LayerId(1),
                clip,
                hidden: false,
                ops: vec![Op { id: OpId(2), data }],
            };
            let pic = || Picture::new(Frame::new(img.clone(), ALL));
            let mut open = pic();
            open.sync(&[], Some(&l), None);
            let (r, shown) = open.open_patch().unwrap();
            assert_eq!(shown.rgb(PixelPoint::new(0, 25)), Some(Rgb(255, 255, 255)));
            let mut committed = pic();
            committed.sync(std::slice::from_ref(&l), None, None);
            assert_eq!(&committed.committed().image().crop(r), shown);
            let mut inspected = pic();
            inspected.inspect(Some(&l));
            assert_eq!(inspected.isolated_patch().unwrap().1, shown);
        }
    }

    #[test]
    fn a_big_batch_keeps_only_the_top_prefixes() {
        let mut p = picture();
        let mut open = layer(1, (0..3).map(|i| bar(i, 0.0)).collect());
        p.sync(&[], Some(&open), None);
        // Thirty more at once, as accepting auto-redact's suggestions adds.
        open.ops
            .extend((3..33).map(|i| bar(i, f64::from(i as u32))));
        p.sync(&[], Some(&open), None);
        assert_eq!(p.ops_applied(), 33);
        assert!(p.prefixes.len() <= PREFIXES, "{}", p.prefixes.len());
        let top = open.ops.len() - 1;
        open.ops[top] = bar(99, 40.0); // the top one changes: only it runs
        p.sync(&[], Some(&open), None);
        assert_eq!(p.ops_applied(), 34);
    }

    #[test]
    fn rebuilding_from_scratch_gives_the_same_pixels() {
        let layers = vec![layer(1, vec![bar(2, 0.0), bar(3, 30.0)])];
        let mut a = picture();
        a.sync(&layers, None, None);
        let mut b = picture();
        b.sync(&layers, Some(&layer(9, vec![])), None); // caches in another state
        b.sync(&layers, None, None);
        assert_eq!(a.crop(ALL), b.crop(ALL));
    }
}
