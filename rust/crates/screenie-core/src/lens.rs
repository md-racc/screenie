//! The lenses: Loupe (`z`), a precision pixel lens, and Enhance (`e`), a
//! movable magnified view inside the box. Lenses change how the frozen
//! picture is seen, never the picture; they may hand a position back.

use crate::anchor_cursor::AnchorCursor;
use crate::boxes::{DRAG_SLOP, STEP};
use crate::geometry::{LogicalPoint, LogicalRect, PixelMap, PixelPoint};
use crate::input::Rates;
use crate::insert::PointCursor;
use crate::intent::{Intent, Scale};
use crate::session::{ChildResult, FinishedAction, Mode, Outcome, Place, Session, Transition};

/// Display-only transforms, for the lenses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewFilter {
    #[default]
    Off,
    Grey,
    Invert,
    /// Greyscale, an S-curve, and dark backgrounds flipped: for reading.
    Contrast,
}

impl ViewFilter {
    const ORDER: [ViewFilter; 4] = [Self::Off, Self::Grey, Self::Invert, Self::Contrast];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Grey => "grey",
            Self::Invert => "invert",
            Self::Contrast => "contrast",
        }
    }

    /// `v` (+1) and `V` (-1) cycle off, grey, invert, contrast.
    pub fn cycled(self, d: i32) -> Self {
        let i = Self::ORDER.iter().position(|&f| f == self).unwrap_or(0) as i32;
        Self::ORDER[(i + d).rem_euclid(4) as usize]
    }
}

/// What the lenses show the picture through: shared by both, kept for the
/// session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub filter: ViewFilter,
    pub sharp: bool,
}

impl View {
    /// The status line's mention of a view filter, if one is on.
    fn tag(self) -> String {
        if self.filter == ViewFilter::Off {
            String::new()
        } else {
            format!("  {}", self.filter.name())
        }
    }
}

/// Loupe: screen px per frame pixel.
pub const ZOOM: f64 = 10.0;
/// The loupe window, frame pixels across: odd, in steps of 4.
pub const SIZE: i32 = 21;
pub const SIZE_MIN: i32 = 9;
pub const SIZE_MAX: i32 = 61;
/// Room under the magnifier for its colour chip, logical px.
pub const CHIP: f64 = 28.0;
/// Loupe repeat: pixels a second; with Shift, loupe-widths a second.
pub const RATES: Rates = Rates {
    start: 15.0,
    top: 200.0,
};
pub const PAGE_RATES: Rates = Rates {
    start: 4.0,
    top: 12.0,
};

#[derive(Clone, Debug)]
pub struct LoupeMode {
    pub cursor: PointCursor,
    pub(crate) dragging: bool,
}

impl LoupeMode {
    /// A loupe in `main`, at `at` (default: the centre).
    pub fn new(map: &PixelMap, main: LogicalRect, at: Option<PixelPoint>) -> Self {
        let mut cursor = PointCursor::new(map, main);
        if let Some(p) = at {
            cursor.put(p.x, p.y, false);
        }
        Self {
            cursor,
            dragging: false,
        }
    }
}

/// Enhance: the starting zoom; a tap moves to the next multiple of
/// `ZOOM_TAP`.
pub const ZOOM_START: f64 = 2.0;
pub const ZOOM_TAP: f64 = 0.5;
pub const ZOOM_MIN: f64 = 1.0;
pub const ZOOM_MAX: f64 = 64.0;
/// Shift: this many steps at a time.
pub const BIG: f64 = 4.0;

#[derive(Clone, Debug)]
pub struct EnhanceMode {
    pub lens: AnchorCursor,
    pub zoom: f64,
    pub(crate) drag: Option<(LogicalPoint, bool)>,
}

impl EnhanceMode {
    /// A lens half the box's size, centred on `at`, inside `main`.
    pub fn new(main: LogicalRect, at: LogicalPoint) -> Self {
        let (w, h) = (main.w, main.h);
        Self {
            lens: AnchorCursor::new(
                LogicalRect::new(at.x - w / 4.0, at.y - h / 4.0, w / 2.0, h / 2.0),
                main,
            ),
            zoom: ZOOM_START,
            drag: None,
        }
    }

    /// To the next `ZOOM_TAP` mark that way: 2 -> 2.5 -> 3, and 2.73 -> 3.
    pub fn zoom_tap(&mut self, up: bool) {
        let marks = self.zoom / ZOOM_TAP;
        let mark = if up {
            (marks + 1e-6).floor() + 1.0
        } else {
            (marks - 1e-6).ceil() - 1.0
        };
        self.zoom = (mark * ZOOM_TAP).clamp(ZOOM_MIN, ZOOM_MAX);
    }
}

impl Session {
    pub(crate) fn view_tag(&self) -> String {
        self.view.tag()
    }

    /// The view keys, shared by the lenses.
    fn view_intent(&mut self, intent: Intent) -> bool {
        match intent {
            Intent::ViewFilter(d) => self.view.filter = self.view.filter.cycled(d),
            Intent::ToggleSharp => self.view.sharp = !self.view.sharp,
            _ => return false,
        }
        true
    }

    /// Open a lens from a mode with a box (or the pen), at a point.
    pub(crate) fn lens_at(&self, intent: Intent, at: LogicalPoint) -> Option<Transition> {
        let main = self.sel.rect();
        Some(Transition::Push(match intent {
            Intent::Loupe => Mode::Loupe(LoupeMode::new(
                &self.pixels,
                main,
                Some(self.pixels.pixel_under(at)),
            )),
            Intent::Enhance => Mode::Enhance(EnhanceMode::new(main, at)),
            _ => return None,
        }))
    }

    pub(crate) fn loupe_rates(&self) -> Rates {
        if self.input_mods().shift {
            PAGE_RATES
        } else {
            RATES
        }
    }

    pub(crate) fn loupe_intent(&mut self, m: &mut LoupeMode, intent: Intent) -> Transition {
        if self.view_intent(intent) {
            return Transition::Stay;
        }
        let page = self.loupe_px;
        let c = &mut m.cursor;
        let v = |d: crate::geometry::Dir| {
            let (x, y) = d.vec();
            (x as i32, y as i32)
        };
        match intent {
            Intent::Steer(d) => {
                let (dx, dy) = v(d);
                c.shift(dx, dy, false);
            }
            Intent::Translate(d) => {
                let (dx, dy) = v(d);
                c.shift(dx * page, dy * page, false);
            }
            Intent::HeldSteer(dirs, n) | Intent::HeldTranslate(dirs, n) => {
                let k = n as i32
                    * if matches!(intent, Intent::HeldTranslate(..)) {
                        page
                    } else {
                        1
                    };
                let (x, y) = dirs.vector();
                c.shift(x as i32 * k, y as i32 * k, false);
            }
            Intent::Adjust(n) => self.loupe_px = (self.loupe_px + 4 * n).clamp(SIZE_MIN, SIZE_MAX),
            Intent::CopyColour => {
                let pixel = c.at;
                return Transition::Finish(Outcome::Done(FinishedAction::Colour { pixel }));
            }
            Intent::Tool(t) => {
                // A build tool right here: a new layer, or, from a lens over
                // a tool, that layer's next tool.
                let at = self.pixels.to_logical(c.at);
                let mode = self.tool_mode_at(t, Place::Centre(at));
                return if self.open.is_some() {
                    // Switching tools locks the item the tool under the
                    // lenses was editing, as it would from the tool itself.
                    let editing = self.editing_under_lenses();
                    self.lock(editing);
                    Transition::SwitchTool(mode)
                } else {
                    Transition::OpenTool(mode)
                };
            }
            Intent::Enhance => {
                let at = self.pixels.to_logical(c.at);
                return Transition::Push(Mode::Enhance(EnhanceMode::new(self.sel.rect(), at)));
            }
            Intent::Press(at) => {
                m.dragging = true;
                let p = self.pixels.pixel_under(at);
                m.cursor.put(p.x, p.y, false);
            }
            Intent::Motion(at) if m.dragging => {
                let p = self.pixels.pixel_under(at);
                m.cursor.put(p.x, p.y, false);
            }
            Intent::Release(_) => m.dragging = false,
            Intent::Loupe | Intent::Confirm | Intent::Back => {
                return Transition::PopWith(ChildResult::Position(c.at));
            }
            _ => {}
        }
        Transition::Stay
    }

    pub(crate) fn enhance_intent(&mut self, m: &mut EnhanceMode, intent: Intent) -> Transition {
        if self.view_intent(intent) {
            return Transition::Stay;
        }
        let shift = self.input_mods().shift;
        let lens = &mut m.lens;
        match intent {
            Intent::Steer(d) | Intent::Translate(d) => {
                let k = STEP
                    * if matches!(intent, Intent::Translate(_)) {
                        BIG
                    } else {
                        1.0
                    };
                let (dx, dy) = d.vec();
                lens.translate(dx * k, dy * k);
            }
            Intent::Scale(d, Scale::Half) => lens.halve(d),
            Intent::Scale(d, Scale::Double) => lens.double(d),
            Intent::HeldSteer(dirs, n) | Intent::HeldTranslate(dirs, n) => {
                let k = f64::from(n) * STEP * if shift { BIG } else { 1.0 };
                let (dx, dy) = dirs.vector();
                lens.translate(dx * k, dy * k);
            }
            Intent::Adjust(n) => m.zoom_tap(n > 0),
            Intent::Loupe => {
                let at = self.pixels.pixel_under(lens.rect().centre());
                return Transition::Push(Mode::Loupe(LoupeMode::new(
                    &self.pixels,
                    self.sel.rect(),
                    Some(at),
                )));
            }
            Intent::Press(at) => m.drag = Some((at, false)),
            Intent::Motion(at) => {
                if let Some((from, moved)) = &mut m.drag {
                    if *moved || (at.x - from.x).abs() + (at.y - from.y).abs() > DRAG_SLOP {
                        *moved = true;
                        let from = *from;
                        lens.drag(from, at);
                    }
                }
            }
            Intent::Release(at) => {
                if let Some((_, false)) = m.drag.take() {
                    // A click centres the lens there, as near as it fits.
                    let c = lens.rect().centre();
                    lens.translate(at.x - c.x, at.y - c.y);
                }
            }
            Intent::Confirm | Intent::Back => return Transition::Pop,
            _ => {}
        }
        Transition::Stay
    }

    /// The loupe's magnifier: beside its cursor, off the box if it can be,
    /// wholly on one monitor (placed like a tag).
    pub fn magnifier(&self, m: &LoupeMode) -> LogicalRect {
        use crate::tags::Spot::*;
        let spots = [
            BelowLeft,
            RightTop,
            AboveLeft,
            LeftTop,
            RightBottom,
            LeftBottom,
        ];
        let c = self.pixels.to_logical(m.cursor.at);
        let around = LogicalRect::new(c.x - 12.0, c.y - 12.0, 24.0, 24.0);
        let size = f64::from(self.loupe_px) * ZOOM;
        // Only the cursor's monitor (docs/picker.md, Loupe).
        let monitors: Vec<LogicalRect> = self
            .layout
            .monitor_at(c)
            .map(|m| vec![m.rect])
            .unwrap_or_else(|| self.layout.monitor_rects());
        let (bw, bh) = (size, size + CHIP);
        let main = self.sel.rect();
        let mut r = crate::tags::place(around, bw, bh, &spots, &monitors, &[main]);
        if r.intersects(&main) {
            // No spot off the box: anywhere it fits.
            r = crate::tags::place(around, bw, bh, &spots, &monitors, &[]);
        }
        // The magnifier itself; its chip goes in the CHIP under it.
        LogicalRect::new(r.x, r.y, size, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_filters_cycle_both_ways() {
        let f = ViewFilter::Off;
        assert_eq!(f.cycled(1), ViewFilter::Grey);
        assert_eq!(f.cycled(-1), ViewFilter::Contrast);
        assert_eq!(ViewFilter::Contrast.cycled(1), ViewFilter::Off);
    }

    #[test]
    fn zoom_taps_land_on_half_marks() {
        let mut e = EnhanceMode::new(
            LogicalRect::new(0.0, 0.0, 100.0, 100.0),
            LogicalPoint::new(50.0, 50.0),
        );
        let mut seen = vec![];
        for up in [true, true, false] {
            e.zoom_tap(up);
            seen.push(e.zoom);
        }
        assert_eq!(seen, [2.5, 3.0, 2.5]);
        e.zoom = 2.73;
        e.zoom_tap(true);
        assert_eq!(e.zoom, 3.0);
        e.zoom = 1.0;
        e.zoom_tap(false);
        assert_eq!(e.zoom, 1.0);
    }
}
