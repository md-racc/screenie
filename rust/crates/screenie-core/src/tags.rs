//! Where tags go: the status line, the legend and lists, placed around the
//! main box (README, Tags).
//!
//! Each kind has its own side, so they never compete for one spot: the
//! status tag on the bottom, the palette and legend on top, lists on the
//! sides. Each block takes the first spot that works, always wholly on one
//! monitor: outside the box; else inside on the same side, while it's small
//! next to the box; else another side. Never on a block already placed.

use crate::geometry::LogicalRect;

/// One chip row, logical px.
pub const ROW: f64 = 20.0;
/// Between a block and the box edge.
pub const MARGIN: f64 = 4.0;
/// An inside block may cover at most this share of the box.
pub const INSIDE_MAX: f64 = 0.35;

/// A candidate spot for a block, relative to the box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    InsideBottomLeft,
    InsideTopLeft,
    BelowLeft,
    /// Right-aligned under the box: near a monitor's right edge.
    BelowRight,
    AboveLeft,
    AboveRight,
    RightBottom,
    RightTop,
    LeftBottom,
    LeftTop,
}

use Spot::*;

pub const STATUS: &[Spot] = &[
    BelowLeft,
    BelowRight,
    InsideBottomLeft,
    AboveLeft,
    AboveRight,
    RightBottom,
    LeftBottom,
];
pub const LEGEND: &[Spot] = &[
    AboveLeft,
    AboveRight,
    InsideTopLeft,
    BelowLeft,
    BelowRight,
    RightTop,
    LeftTop,
];
pub const LIST: &[Spot] = &[RightTop, LeftTop, InsideTopLeft, BelowLeft, AboveLeft];

impl Spot {
    fn inside(self) -> bool {
        matches!(self, InsideBottomLeft | InsideTopLeft)
    }

    /// Which way an outside spot slides past a block already there.
    fn away(self) -> (f64, f64) {
        match self {
            AboveLeft | AboveRight => (0.0, -1.0),
            BelowLeft | BelowRight => (0.0, 1.0),
            RightBottom | RightTop => (1.0, 0.0),
            _ => (-1.0, 0.0),
        }
    }

    /// The block's top-left at this spot.
    fn at(self, b: LogicalRect, bw: f64, bh: f64) -> (f64, f64) {
        let m = MARGIN;
        match self {
            InsideBottomLeft => (b.x + m, b.bottom() - m - bh),
            InsideTopLeft => (b.x + m, b.y + m),
            BelowLeft => (b.x, b.bottom() + m),
            BelowRight => (b.right() - bw, b.bottom() + m),
            AboveLeft => (b.x, b.y - m - bh),
            AboveRight => (b.right() - bw, b.y - m - bh),
            RightBottom => (b.right() + m, b.bottom() - bh),
            RightTop => (b.right() + m, b.y),
            LeftBottom => (b.x - m - bw, b.bottom() - bh),
            LeftTop => (b.x - m - bw, b.y),
        }
    }
}

fn on_a_monitor(r: &LogicalRect, monitors: &[LogicalRect]) -> bool {
    monitors
        .iter()
        .any(|m| m.x <= r.x && m.y <= r.y && r.right() <= m.right() && r.bottom() <= m.bottom())
}

/// Where a `bw` x `bh` block goes (its rectangle): the first of `spots`
/// that fits. If none does, the first spot's position, clamped onto the
/// monitor under the box's centre.
pub fn place(
    b: LogicalRect,
    bw: f64,
    bh: f64,
    spots: &[Spot],
    monitors: &[LogicalRect],
    avoid: &[LogicalRect],
) -> LogicalRect {
    for &spot in spots {
        let (mut x, mut y) = spot.at(b, bw, bh);
        let fits = if spot.inside() {
            let r = LogicalRect::new(x, y, bw, bh);
            bw <= b.w - 2.0 * MARGIN
                && bh <= b.h - 2.0 * MARGIN
                && bw * bh <= INSIDE_MAX * b.w * b.h
                && on_a_monitor(&r, monitors)
                && !avoid.iter().any(|o| o.intersects(&r))
        } else {
            // Outside, a block already there doesn't rule the side out:
            // slide on past it, away from the box.
            let (dx, dy) = spot.away();
            for _ in 0..avoid.len() {
                let r = LogicalRect::new(x, y, bw, bh);
                let Some(hit) = avoid.iter().find(|o| o.intersects(&r)) else {
                    break;
                };
                if dy < 0.0 {
                    y = hit.y - MARGIN - bh;
                } else if dy > 0.0 {
                    y = hit.bottom() + MARGIN;
                } else if dx < 0.0 {
                    x = hit.x - MARGIN - bw;
                } else {
                    x = hit.right() + MARGIN;
                }
            }
            let r = LogicalRect::new(x, y, bw, bh);
            on_a_monitor(&r, monitors) && !avoid.iter().any(|o| o.intersects(&r))
        };
        if fits {
            return LogicalRect::new(x, y, bw, bh);
        }
    }
    let first = spots.first().copied().unwrap_or(InsideTopLeft);
    let (x, y) = first.at(b, bw, bh);
    let c = b.centre();
    let mon = monitors
        .iter()
        .find(|m| m.contains(c))
        .or(monitors.first())
        .copied()
        .unwrap_or(b);
    let x = x.min(mon.right() - bw).max(mon.x);
    let y = y.min(mon.bottom() - bh).max(mon.y);
    LogicalRect::new(x, y, bw, bh)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONS: [LogicalRect; 2] = [
        LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
        LogicalRect::new(1920.0, 0.0, 1920.0, 1080.0),
    ];
    const TAG: (f64, f64) = (250.0, 20.0); // a typical status tag

    fn r(x: f64, y: f64, w: f64, h: f64) -> LogicalRect {
        LogicalRect::new(x, y, w, h)
    }

    fn at(b: LogicalRect, size: (f64, f64), spots: &[Spot], avoid: &[LogicalRect]) -> (f64, f64) {
        let p = place(b, size.0, size.1, spots, &MONS, avoid);
        (p.x, p.y)
    }

    #[test]
    fn the_ideal_is_outside_bottom_left() {
        assert_eq!(
            at(r(100.0, 100.0, 800.0, 400.0), TAG, STATUS, &[]),
            (100.0, 504.0)
        );
    }

    #[test]
    fn a_small_box_pushes_it_outside_below() {
        assert_eq!(
            at(r(100.0, 100.0, 120.0, 40.0), TAG, STATUS, &[]),
            (100.0, 144.0)
        );
    }

    #[test]
    fn at_the_bottom_of_the_monitor_it_goes_above() {
        assert_eq!(
            at(r(100.0, 1050.0, 120.0, 30.0), TAG, STATUS, &[]),
            (100.0, 1026.0)
        );
    }

    #[test]
    fn a_box_filling_the_monitor_puts_it_inside() {
        assert_eq!(at(MONS[0], TAG, STATUS, &[]), (4.0, 1056.0));
    }

    #[test]
    fn nowhere_fits_falls_back_inside_on_the_monitor() {
        let p = place(r(0.0, 0.0, 3840.0, 1080.0), 900.0, 700.0, LIST, &MONS, &[]);
        assert!(on_a_monitor(&p, &MONS));
    }

    #[test]
    fn a_box_filling_one_monitor_puts_its_list_on_the_other() {
        assert_eq!(at(MONS[0], (900.0, 700.0), LIST, &[]), (1924.0, 0.0));
    }

    #[test]
    fn outside_spots_stay_on_one_monitor() {
        let p = place(
            r(1925.0, 1060.0, 60.0, 18.0),
            TAG.0,
            TAG.1,
            STATUS,
            &MONS,
            &[],
        );
        assert!(on_a_monitor(&p, &MONS));
    }

    #[test]
    fn inside_spots_stay_on_one_monitor() {
        // Across the seam, both spots below off-screen: inside would cross it.
        let b = r(1800.0, 0.0, 400.0, 1080.0);
        let status = place(b, TAG.0, TAG.1, STATUS, &MONS, &[]);
        assert!(on_a_monitor(&status, &MONS));
        assert_eq!((status.x, status.y), (2204.0, 1060.0)); // right, bottom
        let b = r(1800.0, 700.0, 300.0, 380.0);
        for (size, spots) in [
            (TAG, STATUS),
            ((300.0, 20.0), LEGEND),
            ((200.0, 60.0), LIST),
        ] {
            let p = place(b, size.0, size.1, spots, &MONS, &[]);
            assert!(on_a_monitor(&p, &MONS), "{p:?}");
        }
    }

    #[test]
    fn on_uneven_monitors_the_status_tag_stays_visible() {
        let mons = [r(0.0, 0.0, 1920.0, 1080.0), r(1920.0, 0.0, 2560.0, 1440.0)];
        let p = place(
            r(0.0, 0.0, 4480.0, 1440.0),
            TAG.0,
            TAG.1,
            STATUS,
            &mons,
            &[],
        );
        assert!(on_a_monitor(&p, &mons), "{p:?}");
    }

    #[test]
    fn each_kind_of_tag_has_its_own_side() {
        let b = r(300.0, 300.0, 400.0, 200.0);
        let status = place(b, TAG.0, TAG.1, STATUS, &MONS, &[]);
        let legend = place(b, 300.0, 20.0, LEGEND, &MONS, &[status]);
        let list = place(b, 200.0, 60.0, LIST, &MONS, &[status, legend]);
        assert_eq!((status.x, status.y), (300.0, 504.0)); // bottom
        assert_eq!((legend.x, legend.y), (300.0, 276.0)); // top
        assert_eq!((list.x, list.y), (704.0, 300.0)); // right side
    }

    #[test]
    fn a_list_goes_left_when_the_right_is_the_screen_edge() {
        assert_eq!(
            at(r(1700.0, 300.0, 200.0, 200.0), (200.0, 60.0), LIST, &[]),
            (1496.0, 300.0)
        );
    }

    #[test]
    fn at_the_bottom_edge_the_status_tag_stays_on_the_bottom_inside() {
        assert_eq!(
            at(r(100.0, 700.0, 800.0, 380.0), TAG, STATUS, &[]),
            (104.0, 1056.0)
        );
    }

    #[test]
    fn near_a_monitor_edge_the_tag_right_aligns_under_the_box() {
        assert_eq!(
            at(r(1700.0, 300.0, 200.0, 200.0), TAG, STATUS, &[]),
            (1650.0, 504.0)
        );
    }

    #[test]
    fn tags_keep_clear_of_what_a_mode_occupies() {
        let loupe = r(300.0, 504.0, 400.0, 240.0);
        let p = place(
            r(300.0, 300.0, 400.0, 200.0),
            TAG.0,
            TAG.1,
            STATUS,
            &MONS,
            &[loupe],
        );
        assert!(!p.intersects(&loupe));
    }

    #[test]
    fn a_block_slides_past_one_already_on_its_side() {
        let b = r(300.0, 300.0, 400.0, 200.0);
        let first = place(b, 300.0, 20.0, LEGEND, &MONS, &[]);
        let second = place(b, 300.0, 20.0, LEGEND, &MONS, &[first]);
        assert_eq!((second.x, second.y), (300.0, 276.0 - 4.0 - 20.0)); // stacked above
    }
}
