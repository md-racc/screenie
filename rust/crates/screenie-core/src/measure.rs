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
use crate::model::{Heads, LineOp, OpData, ShapeKind, ShapeOp, StrokeOp, TextOp};
use crate::session::{FinishedAction, Mode, Outcome, Session, Transition};
use crate::text::TextMode;
use crate::units::{self, Display, Length, Scale};

/// Points this close (capture pixels) to an existing one snap to it.
pub const SNAP: f64 = 4.0;
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
fn arc_sweep(centre: P, a: P, b: P, c: P) -> (f64, f64) {
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
    pub fn values(&self) -> Values {
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
                    v.degrees = Some((-(b.1 - a.1)).atan2(b.0 - a.0).to_degrees());
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

    /// Where its chip goes, capture pixels: a line's middle, an area's
    /// centre, an angle's vertex.
    pub fn anchor(&self) -> P {
        let (line, closed) = self.outline();
        if self.kind == Prim::Angle {
            return p(self.points[1]);
        }
        if closed || self.kind == Prim::Circle {
            let n = line.len() as f64;
            let (sx, sy) = line.iter().fold((0.0, 0.0), |(x, y), q| (x + q.0, y + q.1));
            return (sx / n, sy / n);
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
    pub items: Vec<Measurement>,
    /// The list's selection, an index into `items`.
    pub selected: Option<usize>,
    pub scale: Option<Scale>,
    /// The reference's name, while it's in the list.
    pub reference: Option<String>,
    pub display: Display,
    /// The measurement a calibration is being typed for.
    pub calibrating: Option<usize>,
    /// The first suitable measurement asked for its length already.
    pub asked: bool,
    /// Esc once with measurements: Esc again drops them.
    pub warned: bool,
    /// What the status says once (a bad calibration, no edge).
    pub note: Option<String>,
    /// Where the pointer was last seen: its first report (the overlay
    /// appearing under it) doesn't move the cursor, only moving it does.
    pointer: Option<LogicalPoint>,
    counts: [u32; 6],
}

impl MeasureMode {
    pub fn new(cursor: PointCursor, area: LogicalRect, scale: Option<Scale>) -> Self {
        Self {
            tool: Prim::Line,
            cursor,
            area,
            step: STEP,
            points: Vec::new(),
            items: Vec::new(),
            selected: None,
            display: scale.map_or(Display::Pixels, |s| Display::Real(s.unit)),
            scale,
            reference: None,
            calibrating: None,
            asked: scale.is_some(),
            warned: false,
            note: None,
            pointer: None,
            counts: [0; 6],
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
        let real = |px: f64, power: i32| {
            unit.zip(ppm)
                .map(|(u, ppm)| px / (ppm * u.metres()).powi(power))
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
                    if let Some(r) = real(l, 1) {
                        o["real"] = json!(r);
                        o["unit"] = json!(unit.map(|u| u.name()));
                    }
                }
                if let Some(a) = v.area {
                    o["px2"] = json!(a);
                    if let Some(r) = real(a, 2) {
                        o["real"] = json!(r);
                        o["unit"] = json!(unit.map(|u| format!("{}²", u.name())));
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

    /// Drop a point at the cursor, snapped unless `snap` is off.
    fn drop_point(&mut self, m: &mut MeasureMode, snap: bool) -> Option<Transition> {
        let mut at = m.cursor.at;
        if snap {
            let k = SNAP * self.pixels.scale();
            if let Some(q) = m
                .snap_points()
                .filter(|q| dist(p(*q), p(at)) <= k)
                .min_by(|a, b| dist(p(*a), p(at)).total_cmp(&dist(p(*b), p(at))))
            {
                at = q;
                m.cursor.put(at.x, at.y, false);
            }
        }
        if m.points.last() == Some(&at) {
            m.note = Some("that's the last point: move the cursor first".into());
            return None;
        }
        m.points.push(at);
        m.warned = false;
        if m.tool.complete_at() == Some(m.points.len()) {
            return self.finish_measurement(m);
        }
        None
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
        let name = m.name_next(m.tool);
        m.items.push(Measurement {
            kind: m.tool,
            points,
            name,
            excluded: false,
        });
        m.selected = Some(m.items.len() - 1);
        if m.tool.calibrates() && m.scale.is_none() && !m.asked {
            m.asked = true;
            return Some(self.ask_calibration(m, m.items.len() - 1));
        }
        None
    }

    fn ask_calibration(&mut self, m: &mut MeasureMode, index: usize) -> Transition {
        m.calibrating = Some(index);
        let a = m.items[index].anchor();
        Transition::Push(Mode::Text(TextMode::calibration(self.px_logical(a))))
    }

    /// The typed real length came back.
    pub(crate) fn measure_calibrated(&mut self, m: &mut MeasureMode, text: &str) {
        let Some(i) = m.calibrating.take().filter(|&i| i < m.items.len()) else {
            return;
        };
        match units::parse_length(text) {
            Ok(real) => self.set_reference(m, i, real),
            Err(e) => {
                // C asks again.
                m.note = Some(e);
                m.asked = false;
            }
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
        if s.error > units::SHORT_REFERENCE {
            m.note = Some(format!(
                "short reference (±{:.1}%): measure a longer one, or zoom the source",
                s.error * 100.0
            ));
        }
        m.reference = Some(item.name.clone());
        m.display = Display::Real(s.unit);
        m.scale = Some(s);
        self.measure_scale = Some(s);
    }

    /// `u`: the last point, else the last measurement (and the scale, if
    /// it was the reference).
    fn measure_undo(&mut self, m: &mut MeasureMode) {
        if m.points.pop().is_some() {
            return;
        }
        let Some(last) = m.items.pop() else { return };
        if m.reference.as_deref() == Some(last.name.as_str()) {
            m.reference = None;
            m.scale = None;
            m.display = Display::Pixels;
            m.asked = false;
            self.measure_scale = None;
        }
        m.selected = m.items.len().checked_sub(1);
    }

    /// `a`: the measurements as an annotation layer, then back to Select,
    /// where Enter captures with them in.
    fn bake(&mut self, m: &MeasureMode) -> Transition {
        if m.items.is_empty() {
            return Transition::Pop;
        }
        let rgb = self.ink.rgb;
        let width = 2.0;
        let mut ops = Vec::new();
        for item in &m.items {
            let (line, closed) = item.outline();
            let pts: Vec<LogicalPoint> = line.iter().map(|&q| self.px_logical(q)).collect();
            match item.kind {
                Prim::Line if pts.len() == 2 => ops.push(OpData::Line(LineOp {
                    a: pts[0],
                    b: pts[1],
                    rgb,
                    width,
                    heads: Heads::Both,
                })),
                Prim::Circle if pts.len() > 2 => {
                    let v = item.values();
                    let (c, r) = (item.anchor(), v.radius.unwrap_or(0.0));
                    let a = self.px_logical((c.0 - r, c.1 - r));
                    let b = self.px_logical((c.0 + r, c.1 + r));
                    ops.push(OpData::Shape(ShapeOp {
                        kind: ShapeKind::Circle,
                        rect: LogicalRect::from_edges(a.x, a.y, b.x, b.y),
                        rgb,
                        width,
                    }));
                }
                _ => {
                    let mut pts = pts;
                    if closed {
                        pts.push(pts[0]);
                    }
                    ops.push(OpData::Stroke(StrokeOp {
                        points: pts,
                        rgb,
                        width,
                    }));
                }
            }
            let at = self.px_logical(item.anchor());
            ops.push(OpData::Text(TextOp {
                origin: LogicalPoint::new(at.x + 4.0, at.y + 4.0),
                text: m.label(item),
                rgb,
                size: 14.0,
                chip: true,
            }));
        }
        self.add_layer(ops);
        Transition::PopTo(0)
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
                ItemView {
                    line: line.iter().map(|&q| self.px_logical(q)).collect(),
                    closed,
                    filled: item.kind == Prim::Area,
                    label: m.label(item),
                    anchor: self.px_logical(item.anchor()),
                    reference: m.reference.as_deref() == Some(item.name.as_str()),
                    selected: Some(i) == m.selected,
                    points: item.points.iter().map(|&q| self.px_logical(p(q))).collect(),
                }
            })
            .collect();
        let mut progress: Vec<LogicalPoint> =
            m.points.iter().map(|&q| self.px_logical(p(q))).collect();
        if !progress.is_empty() {
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
            let d = dist(p(last), p(c));
            s.push_str(&format!(
                "  → {}",
                units::length(d, 2, m.scale.as_ref(), m.display)
            ));
        }
        if let Some(sc) = &m.scale {
            s.push_str(&format!("  {}", sc.describe()));
            if m.display == Display::Pixels {
                s.push_str(" (showing px)");
            }
        }
        if m.warned {
            s.push_str(&format!("  Esc again drops {} measurements", m.items.len()));
        } else if let Some(n) = &m.note {
            s.push_str(&format!("  {n}"));
        }
        s
    }

    pub(crate) fn measure_intent(&mut self, m: &mut MeasureMode, intent: Intent) -> Transition {
        let shift = self.input_mods().shift;
        let alt = self.input_mods().alt;
        if !matches!(
            intent,
            Intent::Back | Intent::Motion(_) | Intent::ToggleHelp
        ) {
            m.warned = false;
        }
        if !matches!(intent, Intent::Motion(_)) {
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
            Intent::Adjust(n) => m.step = (m.step + n).clamp(STEP_MIN, STEP_MAX),
            Intent::Measure(kind) => {
                if kind != m.tool {
                    m.points.clear();
                }
                m.tool = kind;
            }
            Intent::DropPoint => {
                if let Some(t) = self.drop_point(m, !alt) {
                    return t;
                }
            }
            Intent::Motion(at) => {
                if m.pointer.is_some_and(|was| was != at) {
                    let q = self.pixels.pixel_under(at);
                    m.cursor.put(q.x, q.y, false);
                }
                m.pointer = Some(at);
            }
            Intent::Press(at) => {
                let mut q = self.pixels.pixel_under(at);
                // Shift on a click: locked to 45° from the last point.
                if let (true, Some(&last)) = (shift, m.points.last()) {
                    q = lock45(last, q);
                }
                m.cursor.put(q.x, q.y, true);
                if let Some(t) = self.drop_point(m, !alt) {
                    return t;
                }
            }
            Intent::Confirm => {
                if let Some(&last) = m.points.last() {
                    // Enter ends at the cursor, if it moved on.
                    if last != m.cursor.at {
                        if let Some(t) = self.drop_point(m, !alt) {
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
                _ => {
                    m.asked = false;
                    m.note = Some("the next line or arc sets the scale".into());
                }
            },
            Intent::UnitSystem => match (m.display, m.scale) {
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
                    if m.reference.as_deref() == Some(gone.name.as_str()) {
                        m.reference = None; // the scale stays
                    }
                    m.selected = (!m.items.is_empty()).then(|| i.min(m.items.len() - 1));
                }
            }
            Intent::ListToggle => {
                if let Some(i) = m.selected {
                    m.items[i].excluded = !m.items[i].excluded;
                }
            }
            Intent::Bake => return self.bake(m),
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

    fn deliver_measurements(&self, m: &MeasureMode) -> Transition {
        Transition::Finish(Outcome::Done(FinishedAction::Measure {
            text: m.text(),
            json: m.json(),
        }))
    }

    fn find_edge(&mut self, from: PixelPoint, d: Dir, bounds: PixelRect) -> Option<PixelPoint> {
        let mut f = self.edge_finder.take()?;
        let r = f.edge(self, from, d, bounds);
        self.edge_finder = Some(f);
        r
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
