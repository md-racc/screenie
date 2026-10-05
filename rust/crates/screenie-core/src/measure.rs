//! Measure (`m` in Select): lengths, angles and areas over the frozen
//! picture, in capture pixels or, once calibrated, in real units
//! (docs/measure.md). It opens no layer: measurements are overlay only
//! until baked (`a`) into an annotation layer.
//!
//! Points sit on capture pixels and are placed with Insert's pixel cursor;
//! Ctrl+hjkl jumps to the next edge in the picture, which the host finds
//! (`EdgeFinder`), since core never sees pixels.

use crate::geometry::{Dir, LogicalPoint, LogicalRect, PixelPoint, PixelRect, rnd};
use crate::input::Rates;
use crate::insert::{BIG, PointCursor, STEP, STEP_MAX, STEP_MIN};
use crate::intent::{DirSet, Intent, Scale as Halve};
use crate::lens::{EnhanceMode, LoupeMode};
use crate::session::{FinishedAction, Mode, Outcome, Session, Transition};
use crate::text::TextMode;
use crate::units::{self, Display, Length, Scale};

/// Points this close (capture pixels) to an existing one snap to it.
pub const SNAP: f64 = 4.0;
/// A Shift'd point, or `^`'s first, reaches this far (logical px) for an
/// existing point to land on exactly.
pub const VERTEX_REACH: f64 = 12.0;
/// `+`/`-` or the wheel on an open line bend it this much (capture px).
pub const BEND_STEP: f64 = 2.0;
/// Segments an arc or circle is drawn with.
const SEGMENTS: usize = 48;

/// Finds the next contrast edge in the picture along a ray: the first
/// pixel past `from` that way where the picture changes, within `bounds`.
pub trait EdgeFinder {
    fn edge(
        &mut self,
        s: &Session,
        from: PixelPoint,
        d: Dir,
        bounds: PixelRect,
    ) -> Option<PixelPoint>;

    /// The region round `seed` (an area by a click inside): flooded over
    /// the picture, stopped by its edges and by `walls` (the drawn
    /// measurements), within `bounds`. Its outline, or why there's none.
    fn region(
        &mut self,
        _s: &Session,
        _seed: PixelPoint,
        _bounds: PixelRect,
        _walls: &[(PixelPoint, PixelPoint)],
    ) -> Result<Vec<PixelPoint>, String> {
        Err("no picture to flood here".into())
    }
}

/// What a measurement is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    /// `s` (as Annotate's line): a line, or a path as points are added;
    /// Enter finishes.
    Line,
    /// `(`: start, a point on it, end.
    Arc,
    /// `c`: three points on it.
    Circle,
    /// `A`: a polygon; Enter finishes.
    Area,
    /// `r`: two corners (Enter with none: the box).
    Rect,
    /// `^`: arm, vertex, arm.
    Angle,
}

impl Prim {
    pub fn name(self) -> &'static str {
        match self {
            Prim::Line => "line",
            Prim::Arc => "arc",
            Prim::Circle => "circle",
            Prim::Area => "area",
            Prim::Rect => "rect",
            Prim::Angle => "angle",
        }
    }

    /// The list's prefix: `L1`, `A1`, `∠1`.
    fn prefix(self) -> &'static str {
        match self {
            Prim::Line => "L",
            Prim::Arc => "∩",
            Prim::Circle => "○",
            Prim::Area => "A",
            Prim::Rect => "R",
            Prim::Angle => "∠",
        }
    }

    /// Points that finish it by themselves (others finish on Enter).
    fn complete_at(self) -> Option<usize> {
        match self {
            Prim::Arc | Prim::Circle | Prim::Angle => Some(3),
            Prim::Rect => Some(2),
            Prim::Line | Prim::Area => None,
        }
    }

    /// Fewest points Enter accepts.
    fn min_points(self) -> usize {
        match self {
            Prim::Line | Prim::Rect => 2,
            _ => 3,
        }
    }

    /// Whether it can be the reference (it has a length).
    pub fn calibrates(self) -> bool {
        matches!(self, Prim::Line | Prim::Arc)
    }
}

type P = (f64, f64);

fn p(q: PixelPoint) -> P {
    (f64::from(q.x), f64::from(q.y))
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The circle through three points: centre and radius (None if they're
/// in a line).
pub fn circumcircle(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |q: P| q.0 * q.0 + q.1 * q.1;
    let ux = (sq(a) * (b.1 - c.1) + sq(b) * (c.1 - a.1) + sq(c) * (a.1 - b.1)) / d;
    let uy = (sq(a) * (c.0 - b.0) + sq(b) * (a.0 - c.0) + sq(c) * (b.0 - a.0)) / d;
    Some(((ux, uy), dist((ux, uy), a)))
}

/// The arc from `a` through `b` to `c`: start angle and (signed) sweep.
pub(crate) fn arc_sweep(centre: P, a: P, b: P, c: P) -> (f64, f64) {
    let ang = |q: P| (q.1 - centre.1).atan2(q.0 - centre.0);
    let tau = std::f64::consts::TAU;
    let start = ang(a);
    let to = |q: P| (ang(q) - start).rem_euclid(tau);
    let (sb, sc) = (to(b), to(c));
    let sweep = if sb < sc { sc } else { sc - tau };
    (start, sweep)
}

/// A measurement's numbers, in capture pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values {
    /// Lines, paths and arcs.
    pub length: Option<f64>,
    /// Areas, circles and rects.
    pub area: Option<f64>,
    pub perimeter: Option<f64>,
    pub radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// An angle's, or a line's direction (counter-clockwise from right).
    pub degrees: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub kind: Prim,
    pub points: Vec<PixelPoint>,
    /// `L1`, `A2`…
    pub name: String,
    /// Left out of the totals (`x`).
    pub excluded: bool,
}

impl Measurement {
    /// Whether it has the points its kind needs (values and outlines of
    /// one that doesn't are empty, never a panic).
    fn complete(&self) -> bool {
        self.points.len() >= self.kind.min_points()
    }

    pub fn values(&self) -> Values {
        if !self.complete() {
            return Values::default();
        }
        let pts: Vec<P> = self.points.iter().map(|&q| p(q)).collect();
        let path = |closed: bool| {
            let mut l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
            if closed && pts.len() > 2 {
                l += dist(pts[pts.len() - 1], pts[0]);
            }
            l
        };
        let mut v = Values::default();
        match self.kind {
            Prim::Line => {
                v.length = Some(path(false));
                if let [a, b] = pts[..] {
                    // + 0.0: a flat line is 0°, not -0°.
                    v.degrees = Some((-(b.1 - a.1)).atan2(b.0 - a.0).to_degrees() + 0.0);
                }
            }
            Prim::Arc => match circumcircle(pts[0], pts[1], pts[2]) {
                Some((c, r)) => {
                    let (_, sweep) = arc_sweep(c, pts[0], pts[1], pts[2]);
                    v.length = Some(r * sweep.abs());
                    v.radius = Some(r);
                    v.degrees = Some(sweep.abs().to_degrees());
                }
                None => v.length = Some(dist(pts[0], pts[1]) + dist(pts[1], pts[2])),
            },
            Prim::Circle => {
                if let Some((_, r)) = circumcircle(pts[0], pts[1], pts[2]) {
                    v.radius = Some(r);
                    v.area = Some(std::f64::consts::PI * r * r);
                    v.perimeter = Some(std::f64::consts::TAU * r);
                }
            }
            Prim::Area => {
                let n = pts.len();
                let twice: f64 = (0..n)
                    .map(|i| {
                        let (a, b) = (pts[i], pts[(i + 1) % n]);
                        a.0 * b.1 - b.0 * a.1
                    })
                    .sum();
                v.area = Some(twice.abs() / 2.0);
                v.perimeter = Some(path(true));
            }
            Prim::Rect => {
                let (w, h) = ((pts[1].0 - pts[0].0).abs(), (pts[1].1 - pts[0].1).abs());
                v.width = Some(w);
                v.height = Some(h);
                v.area = Some(w * h);
                v.perimeter = Some(2.0 * (w + h));
            }
            Prim::Angle => {
                let (a, o, c) = (pts[0], pts[1], pts[2]);
                let (u, w) = ((a.0 - o.0, a.1 - o.1), (c.0 - o.0, c.1 - o.1));
                let cos = (u.0 * w.0 + u.1 * w.1) / (u.0.hypot(u.1) * w.0.hypot(w.1));
                v.degrees = Some(cos.clamp(-1.0, 1.0).acos().to_degrees());
            }
        }
        v
    }

    /// What's drawn, in capture pixels: the polyline (arcs and circles
    /// sampled), and whether it closes.
    pub fn outline(&self) -> (Vec<P>, bool) {
        let pts: Vec<P> = self.points.iter().map(|&q| p(q)).collect();
        if !self.complete() {
            return (pts, false);
        }
        match self.kind {
            Prim::Arc | Prim::Circle => {
                let Some((c, r)) = circumcircle(pts[0], pts[1], pts[2]) else {
                    return (pts, false);
                };
                let (start, sweep) = if self.kind == Prim::Arc {
                    arc_sweep(c, pts[0], pts[1], pts[2])
                } else {
                    (0.0, std::f64::consts::TAU)
                };
                let line = (0..=SEGMENTS)
                    .map(|i| {
                        let t = start + sweep * i as f64 / SEGMENTS as f64;
                        (c.0 + r * t.cos(), c.1 + r * t.sin())
                    })
                    .collect();
                (line, false)
            }
            Prim::Rect => {
                let (a, b) = (pts[0], pts[1]);
                (vec![a, (b.0, a.1), b, (a.0, b.1)], true)
            }
            Prim::Area => (pts, true),
            Prim::Line | Prim::Angle => (pts, false),
        }
    }

    /// An angle's arc, capture pixels: between its arms, a little way out
    /// from the vertex (`k` px a logical px); its middle, and the way out
    /// from the vertex there (a unit vector, for the label).
    pub fn angle_arc(&self, k: f64) -> Option<(Vec<P>, P, (f64, f64))> {
        if self.kind != Prim::Angle || !self.complete() {
            return None;
        }
        let (a, v, c) = (p(self.points[0]), p(self.points[1]), p(self.points[2]));
        let arm = dist(a, v).min(dist(c, v));
        let r = (0.4 * arm).clamp(8.0 * k, 30.0 * k);
        let dir = |q: P| (-(q.1 - v.1)).atan2(q.0 - v.0);
        let from = dir(a);
        let pi = std::f64::consts::PI;
        // The short way round, from one arm to the other.
        let sweep = (dir(c) - from + pi).rem_euclid(2.0 * pi) - pi;
        let at = |t: f64, r: f64| (v.0 + r * t.cos(), v.1 - r * t.sin());
        let arc = (0..=24)
            .map(|i| at(from + sweep * f64::from(i) / 24.0, r))
            .collect();
        let mid = from + sweep / 2.0;
        Some((arc, at(mid, r), (mid.cos(), -mid.sin())))
    }

    /// How far `q` is from its drawing (capture pixels).
    pub fn distance_to(&self, q: P) -> f64 {
        let (line, closed) = self.outline();
        let seg = |a: P, b: P| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 {
                0.0
            } else {
                (((q.0 - a.0) * dx + (q.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
            };
            dist((a.0 + t * dx, a.1 + t * dy), q)
        };
        let mut d = line.first().map_or(f64::INFINITY, |&a| dist(a, q));
        for w in line.windows(2) {
            d = d.min(seg(w[0], w[1]));
        }
        if let (true, Some(&a), Some(&b)) = (closed, line.first(), line.last()) {
            d = d.min(seg(b, a));
        }
        d
    }

    /// Where its chip goes, capture pixels: a line's middle, an area's
    /// centre, an angle's vertex.
    pub fn anchor(&self) -> P {
        let (line, closed) = self.outline();
        if self.kind == Prim::Angle {
            return p(self.points[1]);
        }
        if closed || self.kind == Prim::Circle {
            // The polygon's centroid: its area's middle, however its points
            // bunch (a flooded outline has many round a doorway).
            let n = line.len();
            let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
            for i in 0..n {
                let (p0, p1) = (line[i], line[(i + 1) % n]);
                let cross = p0.0 * p1.1 - p1.0 * p0.1;
                a += cross;
                cx += (p0.0 + p1.0) * cross;
                cy += (p0.1 + p1.1) * cross;
            }
            if a.abs() > 1e-9 {
                return (cx / (3.0 * a), cy / (3.0 * a));
            }
            let (sx, sy) = line.iter().fold((0.0, 0.0), |(x, y), q| (x + q.0, y + q.1));
            return (sx / n as f64, sy / n as f64);
        }
        // Halfway along.
        let total: f64 = line.windows(2).map(|w| dist(w[0], w[1])).sum();
        let mut left = total / 2.0;
        for w in line.windows(2) {
            let d = dist(w[0], w[1]);
            if d >= left && d > 0.0 {
                let t = left / d;
                return (
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                );
            }
            left -= d;
        }
        line[0]
    }
}

#[derive(Clone, Debug)]
pub struct MeasureMode {
    pub tool: Prim,
    pub cursor: PointCursor,
    /// The monitor it measures on, logical: the cursor's and lenses' bounds.
    pub area: LogicalRect,
    pub step: i32,
    /// The measurement in progress.
    pub points: Vec<PixelPoint>,
    /// An open line's bend: its bulge off the chord, capture px (positive
    /// to the left of first point → cursor). Not zero, it ends as an arc.
    pub bend: f64,
    pub items: Vec<Measurement>,
    /// The list's selection, an index into `items`.
    pub selected: Option<usize>,
    pub scale: Option<Scale>,
    /// The reference's name, while it's in the list.
    pub reference: Option<String>,
    pub display: Display,
    /// The measurement a calibration is being typed for.
    pub calibrating: Option<usize>,
    /// Esc once with measurements: Esc again drops them.
    pub warned: bool,
    /// What the status says once (a bad calibration, no edge).
    pub note: Option<String>,
    /// Where the pointer was last seen: its first report (the overlay
    /// appearing under it) doesn't move the cursor, only moving it does.
    pointer: Option<LogicalPoint>,
    counts: [u32; 6],
    /// What `u` takes back, latest last (points in progress come first).
    log: Vec<MeasureUndo>,
    /// A point of a placed measurement being moved (`o`, or an Alt+drag).
    pub edit: Option<PointEdit>,
}

impl MeasureMode {
    pub fn new(cursor: PointCursor, area: LogicalRect, scale: Option<Scale>) -> Self {
        Self {
            tool: Prim::Line,
            cursor,
            area,
            step: STEP,
            points: Vec::new(),
            bend: 0.0,
            items: Vec::new(),
            selected: None,
            display: scale.map_or(Display::Pixels, |s| Display::Real(s.unit)),
            scale,
            reference: None,
            calibrating: None,
            warned: false,
            note: None,
            pointer: None,
            counts: [0; 6],
            log: Vec::new(),
            edit: None,
        }
    }

    /// Held-key rates, as Insert's over the cursor's bounds.
    pub fn rates(&self, held: DirSet, shift: bool) -> Rates {
        crate::insert::InsertMode {
            cursor: self.cursor.clone(),
            step: self.step,
        }
        .rates(held, shift)
    }

    fn label(&self, m: &Measurement) -> String {
        let v = m.values();
        let s = self.scale.as_ref();
        let d = self.display;
        let n = m.points.len();
        let text = match m.kind {
            Prim::Line | Prim::Arc => units::length(v.length.unwrap_or(0.0), n, s, d),
            Prim::Circle => match v.radius {
                Some(r) => format!("Ø {}", units::length(2.0 * r, 3, s, d)),
                None => "no circle".into(),
            },
            Prim::Area | Prim::Rect => {
                units::area(v.area.unwrap_or(0.0), v.perimeter.unwrap_or(0.0), s, d)
            }
            Prim::Angle => units::degrees(v.degrees.unwrap_or(0.0)),
        };
        if self.reference.as_deref() == Some(m.name.as_str()) {
            format!("ref {text}")
        } else {
            text
        }
    }

    /// The text `y` delivers: one line a measurement.
    pub fn text(&self) -> String {
        let s = self.scale.as_ref();
        let d = self.display;
        let mut out = Vec::new();
        if let Some(sc) = s {
            out.push(format!(
                "ref   {} {}  ({} px)",
                units::sig3(sc.real),
                sc.unit.name(),
                format_args!("{:.1}", sc.px)
            ));
        }
        for m in &self.items {
            if self.reference.as_deref() == Some(m.name.as_str()) {
                continue;
            }
            let v = m.values();
            let n = m.points.len();
            let len = |px: f64, _k: usize| units::length_bare(px, s, d);
            let detail = match m.kind {
                Prim::Line if n > 2 => format!("{} segments", n - 1),
                Prim::Line => format!("at {}", units::degrees(v.degrees.unwrap_or(0.0))),
                Prim::Arc => match v.radius {
                    Some(r) => format!(
                        "radius {}, {}",
                        len(r, 3),
                        units::degrees(v.degrees.unwrap_or(0.0))
                    ),
                    None => "straight".into(),
                },
                Prim::Circle => match v.radius {
                    Some(r) => format!(
                        "circumference {}, area {}",
                        len(std::f64::consts::TAU * r, 3),
                        units::area(v.area.unwrap_or(0.0), v.perimeter.unwrap_or(0.0), s, d)
                    ),
                    None => String::new(),
                },
                Prim::Area => format!("perimeter {}", len(v.perimeter.unwrap_or(0.0), n)),
                Prim::Rect => format!(
                    "{} × {}, diagonal {}",
                    len(v.width.unwrap_or(0.0), 2),
                    len(v.height.unwrap_or(0.0), 2),
                    len(v.width.unwrap_or(0.0).hypot(v.height.unwrap_or(0.0)), 2)
                ),
                Prim::Angle => String::new(),
            };
            let label = self.label(m);
            out.push(
                format!("{:<5} {label:<16} {detail}", m.name)
                    .trim_end()
                    .to_string(),
            );
        }
        out.join("\n")
    }

    /// `--json`'s `scale` and `measurements`.
    pub fn json(&self) -> serde_json::Value {
        use serde_json::json;
        let ppm = self.scale.map(|s| s.px_per_metre());
        let unit = match self.display {
            Display::Real(u) if ppm.is_some() => Some(u),
            _ => None,
        };
        // In the unit the value is shown in: fitted by its length (an
        // area by its side), in the display's system.
        let real = |px: f64, power: i32| {
            unit.zip(ppm).map(|(system, ppm)| {
                let side = px.abs().powf(1.0 / f64::from(power)) / ppm;
                let u = units::fit(system, side);
                (px / (ppm * u.metres()).powi(power), u)
            })
        };
        let items: Vec<serde_json::Value> = self
            .items
            .iter()
            .map(|m| {
                let v = m.values();
                let points: Vec<[i32; 2]> = m.points.iter().map(|q| [q.x, q.y]).collect();
                let mut o = json!({"kind": m.kind.name(), "name": m.name, "points": points});
                if let Some(l) = v.length {
                    o["px"] = json!(l);
                    if let Some((r, u)) = real(l, 1) {
                        o["real"] = json!(r);
                        o["unit"] = json!(u.name());
                    }
                }
                if let Some(a) = v.area {
                    o["px2"] = json!(a);
                    if let Some((r, u)) = real(a, 2) {
                        o["real"] = json!(r);
                        o["unit"] = json!(format!("{}²", u.name()));
                    }
                }
                if let (Some(d), Prim::Angle) = (v.degrees, m.kind) {
                    o["degrees"] = json!(d);
                }
                if self.reference.as_deref() == Some(m.name.as_str()) {
                    o["reference"] = json!(true);
                }
                o
            })
            .collect();
        let scale = self
            .scale
            .map(|s| json!({"px": s.px, "real": s.real, "unit": s.unit.name(), "error": s.error}));
        json!({"scale": scale, "measurements": items})
    }

    /// The side list: every measurement, then the totals.
    pub fn list(&self) -> Vec<String> {
        let mut rows: Vec<String> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mark = if Some(i) == self.selected { ">" } else { " " };
                let out = if m.excluded { " (not in total)" } else { "" };
                format!("{mark} {:<4} {}{out}", m.name, self.label(m))
            })
            .collect();
        let s = self.scale.as_ref();
        let d = self.display;
        let counted = || self.items.iter().filter(|m| !m.excluded);
        let lengths: Vec<f64> = counted()
            .filter(|m| m.kind.calibrates())
            .filter_map(|m| m.values().length)
            .collect();
        let areas: Vec<(f64, f64)> = counted()
            .filter_map(|m| {
                let v = m.values();
                v.area.map(|a| (a, v.perimeter.unwrap_or(0.0)))
            })
            .collect();
        let mut total = Vec::new();
        if lengths.len() > 1 {
            total.push(units::length(lengths.iter().sum(), 2 * lengths.len(), s, d));
        }
        if areas.len() > 1 {
            let (a, per) = areas
                .iter()
                .fold((0.0, 0.0), |(x, y), (a, b)| (x + a, y + b));
            total.push(units::area(a, per, s, d));
        }
        if !total.is_empty() {
            rows.push(format!("  total {}", total.join("  ")));
        }
        rows
    }

    /// Every measurement's drawing as segments, to the pixel: what an
    /// area's flood can't cross.
    fn walls(&self) -> Vec<(PixelPoint, PixelPoint)> {
        let px = |q: P| PixelPoint::new(rnd(q.0), rnd(q.1));
        let mut out = Vec::new();
        for it in &self.items {
            let (line, closed) = it.outline();
            out.extend(line.windows(2).map(|w| (px(w[0]), px(w[1]))));
            if let (true, Some(&a), Some(&b)) = (closed, line.first(), line.last()) {
                out.push((px(b), px(a)));
            }
        }
        out
    }

    /// Every straight segment of the lines and paths (arcs aren't straight).
    fn segments(&self) -> impl Iterator<Item = (PixelPoint, PixelPoint)> + '_ {
        self.items
            .iter()
            .filter(|m| m.kind == Prim::Line)
            .flat_map(|m| m.points.windows(2).map(|w| (w[0], w[1])))
    }

    /// Where two segments cross (inside both, not at their ends).
    fn crossings(&self) -> Vec<PixelPoint> {
        let segs: Vec<(P, P)> = self.segments().map(|(a, b)| (p(a), p(b))).collect();
        let mut out = Vec::new();
        for (i, &(a, b)) in segs.iter().enumerate() {
            for &(c, d) in &segs[i + 1..] {
                let (r, q) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
                let den = r.0 * q.1 - r.1 * q.0;
                if den.abs() < 1e-9 {
                    continue; // parallel
                }
                let (ex, ey) = (c.0 - a.0, c.1 - a.1);
                let t = (ex * q.1 - ey * q.0) / den;
                let u = (ex * r.1 - ey * r.0) / den;
                if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                    out.push(PixelPoint::new(rnd(a.0 + t * r.0), rnd(a.1 + t * r.1)));
                }
            }
        }
        out
    }

    /// The rays from `v` along the segments through it: one where a
    /// segment ends there, two where it passes through (within a pixel).
    /// Each ray is the far end it heads for.
    fn rays_at(&self, v: PixelPoint) -> Vec<PixelPoint> {
        let pv = p(v);
        let mut rays = Vec::new();
        for (a, b) in self.segments() {
            if a == v {
                rays.push(b);
            } else if b == v {
                rays.push(a);
            } else {
                let (pa, pb) = (p(a), p(b));
                let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
                let len2 = dx * dx + dy * dy;
                let t = ((pv.0 - pa.0) * dx + (pv.1 - pa.1) * dy) / len2;
                let on = (pa.0 + t * dx, pa.1 + t * dy);
                if t > 0.0 && t < 1.0 && dist(on, pv) <= 1.0 {
                    rays.push(a);
                    rays.push(b);
                }
            }
        }
        rays
    }

    /// Every angle round `v` between neighbouring rays, under 180°: the
    /// back of a straight line and the outside of a corner aren't angles
    /// anyone wants. As [arm, vertex, arm], counter-clockwise on screen.
    fn angles_at(&self, v: PixelPoint) -> Vec<[PixelPoint; 3]> {
        let dir = |q: PixelPoint| {
            let (dx, dy) = (f64::from(q.x - v.x), f64::from(q.y - v.y));
            (-dy).atan2(dx).to_degrees().rem_euclid(360.0)
        };
        let mut rays: Vec<(f64, PixelPoint)> =
            self.rays_at(v).into_iter().map(|q| (dir(q), q)).collect();
        rays.sort_by(|a, b| a.0.total_cmp(&b.0));
        rays.dedup_by(|b, a| (b.0 - a.0).abs() < 0.5); // the same way twice
        if rays.len() < 2 {
            return Vec::new();
        }
        let n = rays.len();
        (0..n)
            .filter_map(|i| {
                let (a, b) = (rays[i], rays[(i + 1) % n]);
                let gap = (b.0 - a.0).rem_euclid(360.0);
                (gap > 0.5 && gap < 179.5).then_some([a.1, v, b.1])
            })
            .collect()
    }

    /// The nearest point on a straight line or path segment to `q`, within
    /// `reach` capture pixels (arcs aren't straight).
    fn on_a_line(&self, q: PixelPoint, reach: f64) -> Option<PixelPoint> {
        let q = p(q);
        self.items
            .iter()
            .filter(|m| m.kind == Prim::Line)
            .flat_map(|m| m.points.windows(2).map(|w| (p(w[0]), p(w[1]))))
            .filter_map(|(a, b)| {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len2 = dx * dx + dy * dy;
                if len2 == 0.0 {
                    return None;
                }
                let t = (((q.0 - a.0) * dx + (q.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
                let on = (a.0 + t * dx, a.1 + t * dy);
                let d = dist(on, q);
                (d <= reach).then_some((d, on))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, on)| PixelPoint::new(rnd(on.0), rnd(on.1)))
    }

    /// A line with its first point placed: `+`/`-` bend it.
    pub fn line_open(&self) -> bool {
        self.tool == Prim::Line && self.points.len() == 1
    }

    fn name_next(&mut self, kind: Prim) -> String {
        let i = Prim::ALL.iter().position(|&k| k == kind).unwrap_or(0);
        self.counts[i] += 1;
        format!("{}{}", kind.prefix(), self.counts[i])
    }

    /// Every point already placed, to snap to.
    fn snap_points(&self) -> impl Iterator<Item = PixelPoint> + '_ {
        self.items
            .iter()
            .flat_map(|m| m.points.iter().copied())
            .chain(self.points.iter().copied())
    }
}

impl Prim {
    const ALL: [Prim; 6] = [
        Prim::Line,
        Prim::Arc,
        Prim::Circle,
        Prim::Area,
        Prim::Rect,
        Prim::Angle,
    ];
}

/// A placed measurement's point being moved: it follows the cursor until
/// kept (Space, Enter, letting go of the drag) or put back (Esc, `u`).
#[derive(Clone, Debug)]
pub struct PointEdit {
    pub item: usize,
    pub point: usize,
    /// Its points, and the scale if it's the reference, as they were.
    original: Vec<PixelPoint>,
    scale: Option<Scale>,
    /// By the mouse (an Alt+drag): letting go keeps it.
    dragging: bool,
}

/// One action `u` can take back. Measurements are found by name: names
/// are never reused, and the log is undone latest first, so they're there.
#[derive(Clone, Debug)]
enum MeasureUndo {
    /// The last `n` measurements were added (an angle's vertex adds several).
    Added(usize),
    /// One was deleted from `at` (and was the reference, or not).
    Deleted {
        at: usize,
        item: Measurement,
        reference: bool,
    },
    /// `x`: in or out of the total.
    Excluded(String),
    /// `<`/`>`: two swapped places.
    Swapped(String, String),
    /// A measurement's points (and the scale, if it was the reference)
    /// before an edit.
    Edited {
        name: String,
        points: Vec<PixelPoint>,
        scale: Option<Scale>,
    },
    /// `C`: the calibration before it.
    Calibrated {
        scale: Option<Scale>,
        reference: Option<String>,
        display: Display,
    },
}

/// What the overlay draws for Measure, in logical coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureView {
    pub items: Vec<ItemView>,
    /// The measurement in progress, to the cursor.
    pub progress: Vec<LogicalPoint>,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemView {
    pub line: Vec<LogicalPoint>,
    pub closed: bool,
    /// Area polygons get a light fill.
    pub filled: bool,
    pub label: String,
    pub anchor: LogicalPoint,
    pub reference: bool,
    pub selected: bool,
    /// Points, for their ticks.
    pub points: Vec<LogicalPoint>,
    /// An angle's arc between its arms (empty for the rest).
    pub arc: Vec<LogicalPoint>,
    /// An angle's label goes out from `anchor` (its arc's middle) this way,
    /// a unit vector, until it clears the arc; others go beside `anchor`.
    pub label_out: Option<(f64, f64)>,
}

impl Session {
    /// A capture-pixel position (fractional) as logical: pixel centres.
    fn px_logical(&self, q: P) -> LogicalPoint {
        self.pixels.from_device(q.0 + 0.5, q.1 + 0.5)
    }

    /// `m` in Select: Measure, its cursor at the box's cursor corner and
    /// bounded by that corner's monitor.
    pub(crate) fn open_measure(&mut self) -> Transition {
        let corner = self.sel.cursor_point();
        let monitor = self
            .layout
            .monitor_at(corner)
            .map_or(self.layout.bounds, |m| m.rect);
        let mut cursor = PointCursor::new(&self.pixels, monitor);
        let at = crate::session::corner_pixel(&self.pixels, &self.sel);
        cursor.put(at.x, at.y, false);
        Transition::Push(Mode::Measure(Box::new(MeasureMode::new(
            cursor,
            monitor,
            self.measure_scale,
        ))))
    }

    /// Drop a point at the cursor, snapped unless `snap` is off. A line
    /// ends at its second point, unless `keep` (Shift) makes it a path.
    /// A `click` reaches further for a vertex when Shift'd or `^`'s first:
    /// the mouse isn't as exact as the keys.
    fn drop_point(
        &mut self,
        m: &mut MeasureMode,
        snap: bool,
        keep: bool,
        click: bool,
    ) -> Option<Transition> {
        if m.tool == Prim::Area {
            // A point inside: the region round it.
            match self.find_region(m.cursor.at, m.cursor.bounds, &m.walls()) {
                Ok(points) if points.len() >= 3 => {
                    m.points = points;
                    return self.finish_measurement(m);
                }
                Ok(_) => m.note = Some("too small to measure".into()),
                Err(e) => m.note = Some(e),
            }
            return None;
        }
        let mut at = m.cursor.at;
        let picking_vertex = m.tool == Prim::Angle && m.points.is_empty();
        if snap {
            let near = |pts: Vec<PixelPoint>, reach: f64| {
                let k = reach * self.pixels.scale();
                pts.into_iter()
                    .filter(|q| dist(p(*q), p(at)) <= k)
                    .min_by(|a, b| dist(p(*a), p(at)).total_cmp(&dist(p(*b), p(at))))
            };
            // A Shift+click (or `^`'s first click) reaches further for
            // another measurement's vertex; every point snaps to any point
            // close by.
            let mut others: Vec<PixelPoint> = m
                .items
                .iter()
                .flat_map(|i| i.points.iter().copied())
                .collect();
            // `^` can also take a point where two lines cross.
            let crossings = if picking_vertex {
                m.crossings()
            } else {
                Vec::new()
            };
            others.extend(&crossings);
            let far = (click && (keep || picking_vertex))
                .then(|| near(others, VERTEX_REACH))
                .flatten()
                // Else, for a Shift+click, onto a line or path near it.
                .or_else(|| {
                    (click && keep)
                        .then(|| m.on_a_line(at, VERTEX_REACH * self.pixels.scale()))
                        .flatten()
                });
            let close: Vec<PixelPoint> = m.snap_points().chain(crossings).collect();
            if let Some(q) = far.or_else(|| near(close, SNAP)) {
                at = q;
                m.cursor.put(at.x, at.y, false);
            }
        }
        if m.points.last() == Some(&at) {
            m.note = Some("that's the last point: move the cursor first".into());
            return None;
        }
        // `^` on a vertex of straight lines: every angle round it, now.
        if picking_vertex {
            let angles = m.angles_at(at);
            if !angles.is_empty() {
                m.log.push(MeasureUndo::Added(angles.len()));
                for points in angles {
                    let name = m.name_next(Prim::Angle);
                    m.items.push(Measurement {
                        kind: Prim::Angle,
                        points: points.to_vec(),
                        name,
                        excluded: false,
                    });
                }
                m.selected = Some(m.items.len() - 1);
                m.warned = false;
                return None;
            }
        }
        m.points.push(at);
        m.warned = false;
        let done = match m.tool {
            Prim::Line => !keep && m.points.len() >= 2,
            tool => tool.complete_at() == Some(m.points.len()),
        };
        if done {
            return self.finish_measurement(m);
        }
        None
    }

    /// The measurement a two-point line becomes: an arc when bent.
    fn bent(m: &MeasureMode, points: Vec<PixelPoint>) -> (Prim, Vec<PixelPoint>) {
        let [a, b] = points[..] else {
            return (m.tool, points);
        };
        if m.tool != Prim::Line || m.bend.abs() < 0.5 {
            return (m.tool, points);
        }
        (Prim::Arc, vec![a, peak(a, b, m.bend), b])
    }

    /// The measurement in progress becomes one; the first that has a
    /// length asks for its real length.
    fn finish_measurement(&mut self, m: &mut MeasureMode) -> Option<Transition> {
        let points = std::mem::take(&mut m.points);
        if points.len() < m.tool.min_points() {
            m.points = points;
            m.note = Some(format!(
                "a {} needs {} points",
                m.tool.name(),
                m.tool.min_points()
            ));
            return None;
        }
        let (kind, points) = Self::bent(m, points);
        m.bend = 0.0;
        let name = m.name_next(kind);
        m.items.push(Measurement {
            kind,
            points,
            name,
            excluded: false,
        });
        m.log.push(MeasureUndo::Added(1));
        m.selected = Some(m.items.len() - 1);
        // Pixels until asked: C on a line or arc sets the scale.
        None
    }

    fn ask_calibration(&mut self, m: &mut MeasureMode, index: usize) -> Transition {
        m.calibrating = Some(index);
        let item = &m.items[index];
        let mut t = TextMode::calibration(self.px_logical(item.anchor()));
        // Read the scale bar it may run along: OCR round the line, which
        // fills the prompt if it reads one before anything's typed.
        let (a, b) = (item.points[0], *item.points.last().expect("a line"));
        let k = self.pixels.scale();
        let pad = rnd(48.0 * k);
        let (x0, y0) = (a.x.min(b.x) - pad, a.y.min(b.y) - pad);
        let (x1, y1) = (a.x.max(b.x) + pad, a.y.max(b.y) + pad);
        let r = crate::geometry::PixelRect::new(x0, y0, x1 - x0, y1 - y0);
        let c = m.cursor.bounds;
        let rect = crate::geometry::PixelRect::new(
            r.x.max(c.x),
            r.y.max(c.y),
            (r.x + r.w).min(c.x + c.w) - r.x.max(c.x),
            (r.y + r.h).min(c.y + c.h) - r.y.max(c.y),
        );
        match self.start_job(crate::session::JobKind::Ocr { rect, codes: false }) {
            (_, Some(result)) => fill_reading(&mut t, result, a, b),
            (id, None) => t.reading = Some((id, a, b)),
        }
        Transition::Push(Mode::Text(t))
    }

    /// The typed real length came back.
    pub(crate) fn measure_calibrated(&mut self, m: &mut MeasureMode, text: &str) {
        let Some(i) = m.calibrating.take().filter(|&i| i < m.items.len()) else {
            return;
        };
        match units::parse_length(text) {
            Ok(real) => self.set_reference(m, i, real),
            Err(e) => m.note = Some(e), // C asks again
        }
    }

    fn set_reference(&mut self, m: &mut MeasureMode, i: usize, real: Length) {
        let item = &m.items[i];
        let px = item.values().length.unwrap_or(0.0);
        if px <= 0.0 {
            m.note = Some("the reference has no length".into());
            return;
        }
        let s = Scale::new(px, real, item.points.len());
        m.log.push(MeasureUndo::Calibrated {
            scale: m.scale,
            reference: m.reference.clone(),
            display: m.display,
        });
        m.reference = Some(item.name.clone());
        m.display = Display::Real(s.unit);
        m.scale = Some(s);
        self.measure_scale = Some(s);
    }

    /// `u`: the last point of the one in progress, else the last action.
    fn measure_undo(&mut self, m: &mut MeasureMode) {
        if m.points.pop().is_some() {
            return;
        }
        let index = |m: &MeasureMode, name: &str| m.items.iter().position(|i| i.name == name);
        match m.log.pop() {
            None => m.note = Some("nothing to undo".into()),
            Some(MeasureUndo::Added(n)) => {
                let keep = m.items.len().saturating_sub(n);
                m.items.truncate(keep);
                m.selected = m.items.len().checked_sub(1);
            }
            Some(MeasureUndo::Deleted {
                at,
                item,
                reference,
            }) => {
                if reference {
                    m.reference = Some(item.name.clone());
                }
                let at = at.min(m.items.len());
                m.items.insert(at, item);
                m.selected = Some(at);
            }
            Some(MeasureUndo::Excluded(name)) => {
                if let Some(i) = index(m, &name) {
                    m.items[i].excluded = !m.items[i].excluded;
                    m.selected = Some(i);
                }
            }
            Some(MeasureUndo::Swapped(a, b)) => {
                if let (Some(i), Some(j)) = (index(m, &a), index(m, &b)) {
                    m.items.swap(i, j);
                    m.selected = Some(j);
                }
            }
            Some(MeasureUndo::Edited {
                name,
                points,
                scale,
            }) => {
                if let Some(i) = index(m, &name) {
                    m.items[i].points = points;
                    m.selected = Some(i);
                }
                if scale.is_some() {
                    m.scale = scale;
                    self.measure_scale = scale;
                }
            }
            Some(MeasureUndo::Calibrated {
                scale,
                reference,
                display,
            }) => {
                m.scale = scale;
                m.reference = reference;
                m.display = display;
                self.measure_scale = scale;
            }
        }
    }

    pub(crate) fn measure_view(&self) -> Option<MeasureView> {
        let Some(Mode::Measure(m)) = self
            .modes()
            .iter()
            .rev()
            .find(|m| matches!(m, Mode::Measure(_)))
        else {
            return None;
        };
        let items = m
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let (line, closed) = item.outline();
                let arc = item.angle_arc(self.pixels.scale());
                ItemView {
                    line: line.iter().map(|&q| self.px_logical(q)).collect(),
                    closed,
                    filled: item.kind == Prim::Area,
                    label: m.label(item),
                    anchor: self.px_logical(arc.as_ref().map_or(item.anchor(), |a| a.1)),
                    label_out: arc.as_ref().map(|a| a.2),
                    reference: m.reference.as_deref() == Some(item.name.as_str()),
                    selected: Some(i) == m.selected,
                    points: item.points.iter().map(|&q| self.px_logical(p(q))).collect(),
                    arc: arc.map_or_else(Vec::new, |(arc, ..)| {
                        arc.iter().map(|&q| self.px_logical(q)).collect()
                    }),
                }
            })
            .collect();
        let mut progress: Vec<LogicalPoint> =
            m.points.iter().map(|&q| self.px_logical(p(q))).collect();
        if let (Some(&a), true) = (progress.first(), m.line_open() && m.bend != 0.0) {
            // An open line bent into an arc, to the cursor.
            let c = self.px_logical(p(m.cursor.at));
            progress.extend(crate::annotate::arc_points(
                a,
                c,
                m.bend / self.pixels.scale(),
            ));
        } else if !progress.is_empty() {
            progress.push(self.px_logical(p(m.cursor.at)));
        }
        Some(MeasureView {
            items,
            progress,
            closed: m.tool == Prim::Area,
        })
    }

    pub(crate) fn measure_status(&self, m: &MeasureMode) -> String {
        let c = m.cursor.at;
        let mut s = format!("{}  ({},{})", m.tool.name(), c.x, c.y);
        if let Some(&last) = m.points.last() {
            // The live length: along the arc when an open line is bent,
            // else straight (other tools' kinds need more points).
            let straight = dist(p(last), p(c));
            let d = if m.line_open() && m.bend != 0.0 {
                let (kind, points) = Self::bent(m, vec![last, c]);
                Measurement {
                    kind,
                    points,
                    name: String::new(),
                    excluded: false,
                }
                .values()
                .length
                .unwrap_or(straight)
            } else {
                straight
            };
            s.push_str(&format!(
                "  → {}",
                units::length(d, 2, m.scale.as_ref(), m.display)
            ));
            if m.line_open() && m.bend != 0.0 {
                s.push_str(&format!("  arc {}px", m.bend));
            }
            if self.input_mods().ctrl {
                s.push_str("  45° lock");
            }
        }
        if let Some(sc) = &m.scale {
            s.push_str(&format!("  {}", sc.describe()));
            // As long as it lasts: a measurement is only as good as this.
            if sc.error > units::SHORT_REFERENCE {
                s.push_str("  short reference: measure a longer one, or zoom the source");
            }
            if m.display == Display::Pixels {
                s.push_str(" (showing px)");
            }
        }
        if let Some(e) = &m.edit {
            let it = &m.items[e.item];
            s.push_str(&format!(
                "  editing {} point {}/{}",
                it.name,
                e.point + 1,
                it.points.len()
            ));
        }
        if m.warned {
            s.push_str(&format!("  Esc again drops {} measurements", m.items.len()));
        } else if let Some(n) = &m.note {
            s.push_str(&format!("  {n}"));
        }
        s
    }

    /// Start moving point `point` of measurement `item`.
    fn edit_start(&mut self, m: &mut MeasureMode, item: usize, point: usize, dragging: bool) {
        let it = &m.items[item];
        let reference = m.reference.as_deref() == Some(it.name.as_str());
        m.edit = Some(PointEdit {
            item,
            point,
            original: it.points.clone(),
            scale: if reference { m.scale } else { None },
            dragging,
        });
        m.selected = Some(item);
        let q = it.points[point];
        m.cursor.put(q.x, q.y, true);
    }

    /// The edited point follows the cursor; the reference's scale keeps
    /// its real length over the new pixel length.
    fn edit_follow(&mut self, m: &mut MeasureMode) {
        let Some(e) = &m.edit else { return };
        let (i, k) = (e.item, e.point);
        m.items[i].points[k] = m.cursor.at;
        if m.reference.as_deref() == Some(m.items[i].name.as_str()) {
            let item = &m.items[i];
            if let (Some(s), Some(px)) = (m.scale, item.values().length) {
                if px > 0.0 {
                    let n = item.points.len() as f64;
                    let s = Scale {
                        px,
                        error: n.sqrt() * units::POINT_ERROR / px,
                        ..s
                    };
                    m.scale = Some(s);
                    self.measure_scale = Some(s);
                }
            }
        }
    }

    /// Keep the edit (one undo step, if anything moved), or put it back.
    fn edit_end(&mut self, m: &mut MeasureMode, keep: bool) {
        let Some(e) = m.edit.take() else { return };
        let item = &mut m.items[e.item];
        if !keep {
            item.points = e.original;
            if e.scale.is_some() {
                m.scale = e.scale;
                self.measure_scale = e.scale;
            }
        } else if item.points != e.original {
            m.log.push(MeasureUndo::Edited {
                name: item.name.clone(),
                points: e.original,
                scale: e.scale,
            });
        }
    }

    /// Alt+click: select the measurement under it; on one of its points,
    /// move that point while the button's down.
    fn alt_press(&mut self, m: &mut MeasureMode, at: LogicalPoint) {
        let q = p(self.pixels.pixel_under(at));
        let reach = VERTEX_REACH * self.pixels.scale();
        let on_point = m
            .items
            .iter()
            .enumerate()
            .flat_map(|(i, it)| it.points.iter().enumerate().map(move |(k, &v)| (i, k, v)))
            .map(|(i, k, v)| (dist(p(v), q), i, k))
            .filter(|&(d, ..)| d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, i, k)) = on_point {
            self.edit_start(m, i, k, true);
            return;
        }
        let on_line = m
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| (it.distance_to(q), i))
            .filter(|&(d, _)| d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match on_line {
            Some((_, i)) => m.selected = Some(i),
            None => m.note = Some("no measurement there".into()),
        }
    }

    pub(crate) fn measure_intent(&mut self, m: &mut MeasureMode, intent: Intent) -> Transition {
        let shift = self.input_mods().shift;
        let alt = self.input_mods().alt;
        let ctrl = self.input_mods().ctrl;
        // Moving a placed point: the cursor's keys and the mouse move it;
        // Space or Enter keep it, Esc or u put it back, o goes to the next.
        if let Some(e) = &m.edit {
            let dragging = e.dragging;
            match intent {
                Intent::SwapOpposite => {
                    let (i, k) = (e.item, e.point);
                    let next = (k + 1) % m.items[i].points.len();
                    m.edit.as_mut().expect("editing").point = next;
                    let q = m.items[i].points[next];
                    m.cursor.put(q.x, q.y, true);
                    return Transition::Stay;
                }
                Intent::DropPoint | Intent::AddPoint | Intent::Confirm => {
                    self.edit_end(m, true);
                    return Transition::Stay;
                }
                Intent::Release(_) if dragging => {
                    self.edit_end(m, true);
                    return Transition::Stay;
                }
                Intent::Back | Intent::Undo => {
                    self.edit_end(m, false);
                    return Transition::Stay;
                }
                Intent::Motion(at) if dragging => {
                    let q = self.pixels.pixel_under(at);
                    m.cursor.put(q.x, q.y, false);
                    self.edit_follow(m);
                    return Transition::Stay;
                }
                Intent::Steer(_)
                | Intent::Translate(_)
                | Intent::HeldSteer(..)
                | Intent::HeldTranslate(..)
                | Intent::Scale(..)
                | Intent::Jump(_)
                | Intent::FarEdge(_)
                | Intent::Motion(_)
                | Intent::Adjust(_)
                | Intent::ToggleHelp => {} // below, then the point follows
                _ => self.edit_end(m, true), // anything else keeps it first
            }
        } else if intent == Intent::SwapOpposite {
            // `o`: edit the selected measurement's points, from the first.
            match m.selected {
                Some(i) if m.points.is_empty() => self.edit_start(m, i, 0, false),
                _ => m.note = Some("select a measurement ([ ], Alt+click), then o".into()),
            }
            return Transition::Stay;
        }
        let t = self.measure_intent_inner(m, intent, shift, alt, ctrl);
        if m.edit.is_some() {
            self.edit_follow(m);
        }
        t
    }

    fn measure_intent_inner(
        &mut self,
        m: &mut MeasureMode,
        intent: Intent,
        shift: bool,
        alt: bool,
        ctrl: bool,
    ) -> Transition {
        if !matches!(
            intent,
            Intent::Back | Intent::Motion(_) | Intent::ToggleHelp
        ) {
            m.warned = false;
        }
        // (letting go of a click isn't fresh input: its note stays)
        if !matches!(intent, Intent::Motion(_) | Intent::Release(_)) {
            m.note = None;
        }
        let step = m.step;
        match intent {
            Intent::Steer(d) => {
                let (dx, dy) = d.vec();
                m.cursor.shift(dx as i32 * step, dy as i32 * step, true);
            }
            Intent::Translate(d) => {
                let (dx, dy) = d.vec();
                m.cursor
                    .shift(dx as i32 * step * BIG, dy as i32 * step * BIG, true);
            }
            Intent::HeldSteer(dirs, n) | Intent::HeldTranslate(dirs, n) => {
                let k = step * n as i32 * if shift { BIG } else { 1 };
                let (dx, dy) = dirs.vector();
                m.cursor.shift(dx as i32 * k, dy as i32 * k, false);
            }
            Intent::Scale(d, Halve::Half) => {
                let e = m.cursor.box_edge(d);
                let c = m.cursor.at;
                let half = |v: i32| rnd(f64::from(v + e) / 2.0);
                match d.axis() {
                    crate::geometry::Axis::X => m.cursor.put(half(c.x), c.y, true),
                    crate::geometry::Axis::Y => m.cursor.put(c.x, half(c.y), true),
                }
            }
            Intent::Jump(d) => {
                let bounds = m.cursor.bounds;
                let found = self.find_edge(m.cursor.at, d, bounds);
                match found {
                    Some(q) => m.cursor.put(q.x, q.y, true),
                    None => m.note = Some(format!("no edge {d:?}").to_lowercase()),
                }
            }
            Intent::FarEdge(d) => {
                let e = m.cursor.box_edge(d);
                let c = m.cursor.at;
                match d.axis() {
                    crate::geometry::Axis::X => m.cursor.put(e, c.y, true),
                    crate::geometry::Axis::Y => m.cursor.put(c.x, e, true),
                }
            }
            // On an open line, +/- and the wheel bend it; else the step.
            Intent::Adjust(n) if m.line_open() => m.bend += BEND_STEP * f64::from(n),
            Intent::Adjust(n) => m.step = (m.step + n).clamp(STEP_MIN, STEP_MAX),
            Intent::Measure(kind) => {
                if kind != m.tool {
                    m.points.clear();
                    m.bend = 0.0;
                }
                m.tool = kind;
            }
            Intent::DropPoint | Intent::AddPoint => {
                if let Some(t) = self.drop_point(m, !alt, intent == Intent::AddPoint, false) {
                    return t;
                }
            }
            Intent::Motion(at) => {
                if m.pointer.is_some_and(|was| was != at) {
                    let mut q = self.pixels.pixel_under(at);
                    // Ctrl held: locked to 0/45/90° from the last point.
                    if let (true, Some(&last)) = (ctrl, m.points.last()) {
                        q = lock45(last, q);
                    }
                    m.cursor.put(q.x, q.y, false);
                }
                m.pointer = Some(at);
            }
            Intent::Press(at) if alt => self.alt_press(m, at),
            Intent::Press(at) => {
                let mut q = self.pixels.pixel_under(at);
                // Ctrl on a click: locked to 45° from the last point.
                if let (true, Some(&last)) = (ctrl, m.points.last()) {
                    q = lock45(last, q);
                }
                m.cursor.put(q.x, q.y, true);
                // Shift on a click keeps a line going, as a path.
                if let Some(t) = self.drop_point(m, true, shift, true) {
                    return t;
                }
            }
            Intent::Confirm => {
                if let Some(&last) = m.points.last() {
                    // Enter ends at the cursor, if it moved on.
                    if last != m.cursor.at {
                        if let Some(t) = self.drop_point(m, !alt, false, false) {
                            return t;
                        }
                    }
                    if m.points.is_empty() {
                        return Transition::Stay; // that point finished it
                    }
                    if let Some(t) = self.finish_measurement(m) {
                        return t;
                    }
                } else if m.tool == Prim::Rect {
                    // Enter with no corners: the box.
                    let r = self.pixels.to_pixels(self.sel.rect());
                    m.points = vec![
                        PixelPoint::new(r.x, r.y),
                        PixelPoint::new(r.x + r.w, r.y + r.h),
                    ];
                    if let Some(t) = self.finish_measurement(m) {
                        return t;
                    }
                } else if !m.items.is_empty() {
                    return self.deliver_measurements(m);
                }
            }
            Intent::CopyMeasurements => {
                if m.items.is_empty() {
                    m.note = Some("nothing measured yet".into());
                } else {
                    return self.deliver_measurements(m);
                }
            }
            Intent::Back => {
                if !m.points.is_empty() {
                    m.points.clear();
                } else if !m.items.is_empty() && !m.warned {
                    m.warned = true;
                } else {
                    return Transition::Pop;
                }
            }
            Intent::Undo => self.measure_undo(m),
            Intent::Calibrate => match m.selected {
                Some(i) if m.items[i].kind.calibrates() => {
                    return self.ask_calibration(m, i);
                }
                _ => m.note = Some("select a line or arc ([ ]), then C sets the scale".into()),
            },
            Intent::UnitSystem => match (m.display, m.scale) {
                (_, Some(s)) if s.unit.is_tag() => {
                    m.note = Some(format!("{}: nothing to convert to", s.unit.name()));
                }
                (Display::Real(u), Some(_)) => m.display = Display::Real(u.other()),
                (Display::Pixels, Some(s)) => m.display = Display::Real(s.unit),
                _ => m.note = Some("not calibrated".into()),
            },
            Intent::ShowPixels => {
                m.display = match (m.display, m.scale) {
                    (Display::Pixels, Some(s)) => Display::Real(s.unit),
                    _ => Display::Pixels,
                }
            }
            Intent::ListStep(d) => {
                let n = m.items.len();
                if n > 0 {
                    let i = m.selected.unwrap_or(n - 1) as i64;
                    m.selected = Some((i - i64::from(d)).clamp(0, n as i64 - 1) as usize);
                }
            }
            Intent::ListDelete => {
                if let Some(i) = m.selected {
                    let gone = m.items.remove(i);
                    let reference = m.reference.as_deref() == Some(gone.name.as_str());
                    if reference {
                        m.reference = None; // the scale stays
                    }
                    m.log.push(MeasureUndo::Deleted {
                        at: i,
                        item: gone,
                        reference,
                    });
                    m.selected = (!m.items.is_empty()).then(|| i.min(m.items.len() - 1));
                }
            }
            Intent::ListToggle => {
                if let Some(i) = m.selected {
                    m.items[i].excluded = !m.items[i].excluded;
                    m.log.push(MeasureUndo::Excluded(m.items[i].name.clone()));
                }
            }
            Intent::ListMove(d) => {
                // Shown first-made at the top: up (`>`) is towards index 0.
                if let Some(i) = m.selected {
                    let j = i as i64 - i64::from(d);
                    if (0..m.items.len() as i64).contains(&j) {
                        let j = j as usize;
                        m.items.swap(i, j);
                        m.log.push(MeasureUndo::Swapped(
                            m.items[i].name.clone(),
                            m.items[j].name.clone(),
                        ));
                        m.selected = Some(j);
                    }
                }
            }
            Intent::Loupe => {
                return Transition::Push(Mode::Loupe(LoupeMode::new(
                    &self.pixels,
                    m.area,
                    Some(m.cursor.at),
                )));
            }
            Intent::Enhance => {
                // Over a region around the cursor, the size of a small box.
                let at = self.pixels.to_logical(m.cursor.at);
                let a = m.area;
                let (w, h) = (480.0f64.min(a.w), 270.0f64.min(a.h));
                let x = (at.x - w / 2.0).clamp(a.x, a.right() - w);
                let y = (at.y - h / 2.0).clamp(a.y, a.bottom() - h);
                let region = LogicalRect::new(x, y, w, h);
                return Transition::Push(Mode::Enhance(EnhanceMode::new(region, at)));
            }
            _ => {}
        }
        Transition::Stay
    }

    /// Space (or Shift+Space) in a loupe over Measure: a point at the
    /// loupe's pixel exactly (unsnapped), and the loupe stays up. Only the length prompt (the
    /// first line done) takes it down, to ask.
    pub(crate) fn drop_through_loupe(&mut self, intent: Intent) {
        let (Some(Mode::Loupe(mut l)), Some(Mode::Measure(mut m))) =
            (self.stack_mut().pop(), self.stack_mut().pop())
        else {
            unreachable!("only called over Measure")
        };
        m.cursor.put(l.cursor.at.x, l.cursor.at.y, false);
        // Exactly where the loupe is: no snapping, it's there for precision.
        m.note = None;
        let t = self
            .drop_point(&mut m, false, intent == Intent::AddPoint, false)
            .unwrap_or(Transition::Stay);
        l.cursor.put(m.cursor.at.x, m.cursor.at.y, false);
        self.loupe_at = Some(l.cursor.at);
        self.stack_mut().push(Mode::Measure(m));
        match t {
            Transition::Stay => self.stack_mut().push(Mode::Loupe(l)),
            other => self.apply(other),
        }
    }

    fn deliver_measurements(&self, m: &MeasureMode) -> Transition {
        Transition::Finish(Outcome::Done(FinishedAction::Measure {
            text: m.text(),
            json: m.json(),
        }))
    }

    fn find_region(
        &mut self,
        seed: PixelPoint,
        bounds: PixelRect,
        walls: &[(PixelPoint, PixelPoint)],
    ) -> Result<Vec<PixelPoint>, String> {
        let Some(mut f) = self.edge_finder.take() else {
            return Err("no picture to flood here".into());
        };
        let r = f.region(self, seed, bounds, walls);
        self.edge_finder = Some(f);
        r
    }

    fn find_edge(&mut self, from: PixelPoint, d: Dir, bounds: PixelRect) -> Option<PixelPoint> {
        let mut f = self.edge_finder.take()?;
        let r = f.edge(self, from, d, bounds);
        self.edge_finder = Some(f);
        r
    }
}

/// The point `bend` off the middle of the chord a→b, to its left on
/// screen (up when it runs rightwards): an arc's middle point.
fn peak(a: PixelPoint, b: PixelPoint, bend: f64) -> PixelPoint {
    let (dx, dy) = (f64::from(b.x - a.x), f64::from(b.y - a.y));
    let len = dx.hypot(dy).max(1e-9);
    let (mx, my) = (f64::from(a.x + b.x) / 2.0, f64::from(a.y + b.y) / 2.0);
    PixelPoint::new(rnd(mx + dy / len * bend), rnd(my - dx / len * bend))
}

/// A length read off a scale bar next to a line from `a` to `b`: the
/// numbers nearest each end (`0 … 100 m` reads `100 m`), or a lone number
/// (a `5 km` label), with the unit glued to one or the nearest unit word.
/// None when there's no number to read.
pub fn scale_bar(ocr: &crate::ocr::OcrResult, a: PixelPoint, b: PixelPoint) -> Option<String> {
    let word = regex::Regex::new(r"^([0-9]+(?:\.[0-9]+)?)([A-Za-z]+)?$").expect("a pattern");
    let centre = |r: PixelRect| {
        (
            f64::from(r.x) + f64::from(r.w) / 2.0,
            f64::from(r.y) + f64::from(r.h) / 2.0,
        )
    };
    let mut numbers: Vec<(f64, P, Option<units::Unit>)> = Vec::new();
    let mut unit_words: Vec<(units::Unit, P)> = Vec::new();
    for w in &ocr.words {
        let t = w.text.trim_matches(|c: char| ",;:()".contains(c));
        if let Some(c) = word.captures(t) {
            let value: f64 = c[1].parse().ok()?;
            let unit = c
                .get(2)
                .and_then(|u| units::Unit::standard_name(u.as_str()));
            if c.get(2).is_none() || unit.is_some() {
                numbers.push((value, centre(w.bbox), unit));
            }
        } else if let Some(u) = units::Unit::standard_name(t) {
            unit_words.push((u, centre(w.bbox)));
        }
    }
    // A number belongs to an end if it's near it; a bar's start may be an
    // unread (or unlabelled) 0, and a lone number is a label of its own.
    let len = dist(p(a), p(b));
    let near = 24f64.max(0.15 * len);
    let nearest = |q: P| {
        numbers
            .iter()
            .enumerate()
            .map(|(i, n)| (dist(n.1, q), i))
            .filter(|&(d, _)| d <= near)
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, i)| i)
    };
    let (start, end) = (nearest(p(a)), nearest(p(b)));
    let (value, i, j) = match (start, end) {
        (Some(i), Some(j)) if i != j => ((numbers[j].0 - numbers[i].0).abs(), i, j),
        (Some(i), _) | (None, Some(i)) => (numbers[i].0, i, i),
        (None, None) if numbers.len() == 1 => (numbers[0].0, 0, 0),
        (None, None) => return None,
    };
    if value <= 0.0 {
        return None;
    }
    let mid = ((f64::from(a.x + b.x)) / 2.0, (f64::from(a.y + b.y)) / 2.0);
    let unit = numbers[j].2.or(numbers[i].2).or_else(|| {
        unit_words
            .iter()
            .min_by(|x, y| dist(x.1, mid).total_cmp(&dist(y.1, mid)))
            .map(|u| u.0)
    });
    let v = if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    };
    Some(match unit {
        Some(u) => format!("{v} {}", u.name()),
        None => v,
    })
}

/// A scale-bar reading into the calibration prompt, unless it's been
/// typed in already.
pub(crate) fn fill_reading(
    t: &mut TextMode,
    result: crate::session::JobResult,
    a: PixelPoint,
    b: PixelPoint,
) {
    t.reading = None;
    let crate::session::JobResult::Ocr(Ok(ocr)) = result else {
        return; // no OCR, or it failed: just type it
    };
    if let (Some(text), true) = (scale_bar(&ocr, a, b), t.text.is_empty()) {
        t.text = text;
        t.suggested = true;
    }
}

/// `q` moved onto the nearest 45° ray from `from`.
fn lock45(from: PixelPoint, q: PixelPoint) -> PixelPoint {
    let (dx, dy) = (f64::from(q.x - from.x), f64::from(q.y - from.y));
    let r = dx.hypot(dy);
    let step = std::f64::consts::FRAC_PI_4;
    let a = (dy.atan2(dx) / step).round() * step;
    PixelPoint::new(from.x + rnd(r * a.cos()), from.y + rnd(r * a.sin()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(kind: Prim, pts: &[(i32, i32)]) -> Measurement {
        Measurement {
            kind,
            points: pts.iter().map(|&(x, y)| PixelPoint::new(x, y)).collect(),
            name: "X1".into(),
            excluded: false,
        }
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn lines_paths_and_their_direction() {
        let v = m(Prim::Line, &[(0, 0), (30, -40)]).values();
        assert!(close(v.length, 50.0));
        assert!(close(v.degrees, 53.13010235415598)); // up and right
        let v = m(Prim::Line, &[(0, 0), (3, 4), (3, 14)]).values();
        assert!(close(v.length, 15.0));
        assert_eq!(v.degrees, None);
    }

    #[test]
    fn arcs_and_circles_from_three_points() {
        // A half circle of radius 10, over the top.
        let v = m(Prim::Arc, &[(-10, 0), (0, -10), (10, 0)]).values();
        assert!(close(v.radius, 10.0));
        assert!(close(v.length, 10.0 * std::f64::consts::PI));
        assert!(close(v.degrees, 180.0));
        // The other way round: the short way, through its middle.
        let r = (50.0f64).sqrt();
        let v = m(Prim::Arc, &[(-5, -5), (0, -7), (5, -5)]).values();
        assert!(v.length.unwrap() < r * std::f64::consts::PI);
        let v = m(Prim::Circle, &[(-10, 0), (0, 10), (10, 0)]).values();
        assert!(close(v.area, std::f64::consts::PI * 100.0));
        let v = m(Prim::Arc, &[(0, 0), (5, 0), (10, 0)]).values();
        assert!(close(v.length, 10.0)); // in a line: straight
    }

    #[test]
    fn areas_rects_and_angles() {
        let v = m(Prim::Area, &[(0, 0), (10, 0), (10, 10), (0, 10)]).values();
        assert!(close(v.area, 100.0) && close(v.perimeter, 40.0));
        let v = m(Prim::Rect, &[(10, 10), (0, 30)]).values();
        assert!(close(v.width, 10.0) && close(v.height, 20.0) && close(v.area, 200.0));
        let v = m(Prim::Angle, &[(10, 0), (0, 0), (0, -10)]).values();
        assert!(close(v.degrees, 90.0));
    }

    #[test]
    fn too_few_points_give_nothing_rather_than_a_panic() {
        for kind in [
            Prim::Line,
            Prim::Arc,
            Prim::Circle,
            Prim::Area,
            Prim::Rect,
            Prim::Angle,
        ] {
            let short = m(kind, &[(0, 0), (10, 5)][..kind.min_points() - 1]);
            assert_eq!(short.values(), Values::default(), "{kind:?}");
            let _ = (short.outline(), short.anchor());
        }
    }

    #[test]
    fn an_angle_draws_its_arc_between_the_arms_and_labels_off_its_middle() {
        // Right and up from (100, 100): a quarter circle, the short way.
        let a = m(Prim::Angle, &[(200, 100), (100, 100), (100, 0)]);
        let (arc, mid, out) = a.angle_arc(1.0).unwrap();
        let r = 30.0; // 0.4 of a 100px arm, at most 30
        for q in &arc {
            assert!((dist(*q, (100.0, 100.0)) - r).abs() < 1e-9);
            assert!(
                q.0 >= 100.0 - 1e-9 && q.1 <= 100.0 + 1e-9,
                "in the quadrant: {q:?}"
            );
        }
        assert!((arc[0].0 - 130.0).abs() < 1e-9 && (arc[24].1 - 70.0).abs() < 1e-9);
        // Its middle is on the bisector, and out is up and to the right.
        let d = r / 2f64.sqrt();
        assert!((mid.0 - (100.0 + d)).abs() < 1e-9 && (mid.1 - (100.0 - d)).abs() < 1e-9);
        let h = 1.0 / 2f64.sqrt();
        assert!((out.0 - h).abs() < 1e-9 && (out.1 + h).abs() < 1e-9);
        // Short arms keep it small, but not below 8px.
        let small = m(Prim::Angle, &[(110, 100), (100, 100), (100, 90)]);
        let (arc, ..) = small.angle_arc(1.0).unwrap();
        assert!((dist(arc[0], (100.0, 100.0)) - 8.0).abs() < 1e-9);
        assert!(m(Prim::Line, &[(0, 0), (1, 1)]).angle_arc(1.0).is_none());
    }

    #[test]
    fn an_areas_label_goes_at_its_centroid_however_its_points_bunch() {
        // A 100x50 rectangle with many points bunched on its right side.
        let mut pts = vec![(0, 0), (100, 0)];
        pts.extend((1..20).map(|i| (100, i)));
        pts.extend([(100, 50), (0, 50)]);
        let a = m(Prim::Area, &pts).anchor();
        assert!(
            (a.0 - 50.0).abs() < 1e-6 && (a.1 - 25.0).abs() < 1e-6,
            "{a:?}"
        );
    }

    #[test]
    fn a_flat_line_points_at_0_degrees_not_minus_0() {
        let v = m(Prim::Line, &[(0, 0), (30, 0)]).values();
        assert_eq!(units::degrees(v.degrees.unwrap()), "0.0°");
    }

    #[test]
    fn a_shift_click_locks_to_45_degrees() {
        let o = PixelPoint::new(0, 0);
        assert_eq!(lock45(o, PixelPoint::new(100, 9)), PixelPoint::new(100, 0));
        assert_eq!(lock45(o, PixelPoint::new(50, 47)), PixelPoint::new(49, 49));
    }

    #[test]
    fn a_line_labels_at_its_middle() {
        let a = m(Prim::Line, &[(0, 0), (10, 0), (10, 10)]).anchor();
        assert!((a.0 - 10.0).abs() < 1e-9 && a.1.abs() < 1e-9);
    }
}

#[cfg(test)]
mod scale_bar_tests {
    use super::*;
    use crate::ocr::{OcrResult, OcrWord};

    /// Words at (x, y), 10px a character, 12 high.
    fn ocr(words: &[(&str, i32, i32)]) -> OcrResult {
        OcrResult {
            text: String::new(),
            words: words
                .iter()
                .map(|&(t, x, y)| OcrWord {
                    text: t.into(),
                    bbox: PixelRect::new(x, y, 10 * t.chars().count() as i32, 12),
                    confidence: Some(90.0),
                    line: 0,
                })
                .collect(),
            codes: vec![],
        }
    }

    fn read(words: &[(&str, i32, i32)]) -> Option<String> {
        scale_bar(
            &ocr(words),
            PixelPoint::new(100, 100),
            PixelPoint::new(300, 100),
        )
    }

    #[test]
    fn a_bar_reads_as_the_numbers_at_its_ends() {
        // "0     50    100 m" under a bar from x 100 to 300.
        let bar = [
            ("0", 95, 106),
            ("50", 190, 106),
            ("100", 285, 106),
            ("m", 330, 106),
        ];
        assert_eq!(read(&bar).as_deref(), Some("100 m"));
        // Glued units, and a bar that doesn't start at 0.
        let bar = [("10km", 85, 106), ("20km", 280, 106)];
        assert_eq!(read(&bar).as_deref(), Some("10 km"));
        // Only part of a bar: from 0 to 50 of it.
        let bar = [
            ("0", 95, 106),
            ("50", 290, 106),
            ("100", 485, 106),
            ("ft", 530, 106),
        ];
        assert_eq!(read(&bar).as_deref(), Some("50 ft"));
    }

    #[test]
    fn a_bar_whose_0_wasnt_read_starts_at_0() {
        // OCR often drops a lone "0": "50 100 m", the line from 0 to 100.
        let bar = [("50", 190, 106), ("100", 285, 106), ("m", 330, 106)];
        assert_eq!(read(&bar).as_deref(), Some("100 m"));
    }

    #[test]
    fn a_single_label_reads_as_itself_and_nothing_reads_as_nothing() {
        assert_eq!(
            read(&[("5", 180, 80), ("km", 200, 80)]).as_deref(),
            Some("5 km")
        );
        assert_eq!(read(&[("1:500", 180, 80)]), None);
        assert_eq!(read(&[("Kitchen", 150, 60)]), None);
        assert_eq!(read(&[]), None);
        // A number with no unit near: plain.
        assert_eq!(
            read(&[("0", 95, 106), ("40", 290, 106)]).as_deref(),
            Some("40")
        );
    }
}
