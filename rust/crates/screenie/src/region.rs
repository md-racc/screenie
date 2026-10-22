//! REGION (docs/cli.md, SOURCES AND REGIONS), resolved on a layout.

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;

/// Why a region didn't resolve.
#[derive(Debug, PartialEq)]
pub enum Unresolved {
    /// Malformed, or meaningless here: a usage error.
    Bad(String),
    /// Well formed, but nothing is there (no such window, monitor or
    /// remembered box): nothing to capture.
    Missing(String),
}

/// A region as written, resolved to a logical rectangle. `ring` is the
/// recall ring on this layout, newest first.
pub fn resolve(
    spec: &str,
    layout: &Layout,
    ring: &[LogicalRect],
) -> Result<LogicalRect, Unresolved> {
    use Unresolved::{Bad, Missing};
    let spec = spec.trim();
    match spec {
        "all" => return Ok(layout.bounds),
        "monitor" => {
            return layout
                .monitor(layout.focused)
                .map(|m| m.rect)
                .ok_or_else(|| Missing("no focused monitor".into()));
        }
        "active" => {
            // In the picker, the active window's box (shot captures the
            // window's own pixels instead).
            return layout
                .active
                .and_then(|id| layout.window(id))
                .map(|w| w.rect)
                .ok_or_else(|| Missing("no active window".into()));
        }
        _ => {}
    }
    if let Some(name) = spec.strip_prefix("monitor:") {
        return layout
            .monitors
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.rect)
            .ok_or_else(|| Missing(format!("no monitor named {name:?}")));
    }
    if spec.starts_with("window:") {
        return Err(Bad(
            "window regions are for the live screen (shot, or pick from the screen)".into(),
        ));
    }
    if let Some(rest) = spec.strip_prefix("last-region") {
        let n: usize = match rest.strip_prefix(':') {
            Some(n) => n
                .parse()
                .ok()
                .filter(|&n| n >= 1)
                .ok_or_else(|| Bad(format!("bad region {spec:?}")))?,
            None if rest.is_empty() => 1,
            None => return Err(Bad(format!("bad region {spec:?}"))),
        };
        return ring
            .get(n - 1)
            .copied()
            .ok_or_else(|| Missing(format!("no {spec} (the ring has {})", ring.len())));
    }
    geometry(spec).ok_or_else(|| {
        Bad(format!(
            "bad region {spec:?}: expected all, monitor, X,Y WxH..."
        ))
    })
}

/// Whether a box has something on screen to capture: an area, on some
/// monitor (a box partly off them keeps its size, the rest black).
pub fn on_screen(r: LogicalRect, layout: &Layout) -> bool {
    r.w > 0.0 && r.h > 0.0 && layout.monitors.iter().any(|m| m.rect.intersects(&r))
}

/// slurp's `X,Y WxH`.
pub(crate) fn geometry(s: &str) -> Option<LogicalRect> {
    let (xy, wh) = s.split_once(' ')?;
    let (x, y) = xy.split_once(',')?;
    let (w, h) = wh.trim().split_once('x')?;
    let n = |v: &str| v.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    let (w, h) = (n(w)?, n(h)?);
    (w >= 0.0 && h >= 0.0).then_some(LogicalRect::new(n(x)?, n(y)?, w, h))
}

/// A box as `--select-only` prints it. The integer formats round outward,
/// so pasting one back never loses a pixel.
pub fn format(r: LogicalRect, f: crate::cli::RegionFormat) -> String {
    use crate::cli::RegionFormat;
    let (x0, y0) = (r.x.floor(), r.y.floor());
    let (x1, y1) = (r.right().ceil(), r.bottom().ceil());
    let (w, h) = (x1 - x0, y1 - y0);
    match f {
        RegionFormat::Slurp => format!("{x0},{y0} {w}x{h}"),
        RegionFormat::Hypr => format!("{x0} {y0} {w} {h}"),
        RegionFormat::Json => crate::report::rect_json(r).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_only_formats_round_outward_but_json_is_exact() {
        use crate::cli::RegionFormat;
        let r = LogicalRect::new(10.5, 20.0, 100.25, 50.0);
        assert_eq!(format(r, RegionFormat::Slurp), "10,20 101x50");
        assert_eq!(format(r, RegionFormat::Hypr), "10 20 101 50");
        assert_eq!(
            format(r, RegionFormat::Json),
            r#"{"h":50.0,"w":100.25,"x":10.5,"y":20.0}"#
        );
        // Pasted back, it covers the box.
        assert!(geometry("10,20 101x50").is_some_and(|b| b.x <= r.x && b.right() >= r.right()));
    }
    use screenie_core::layout::{Monitor, MonitorId, Window, WindowId};

    #[test]
    fn active_resolves_from_the_layout_it_was_taken_with() {
        // Over --from last, the window focused then; window: regions need
        // the live screen.
        let mut l = layout();
        let rect = LogicalRect::new(50.0, 60.0, 300.0, 200.0);
        l.windows.push(Window {
            id: WindowId(7),
            rect,
            monitor: Some(MonitorId(0)),
            class: "foot".into(),
            title: "~".into(),
        });
        l.active = Some(WindowId(7));
        assert_eq!(resolve("active", &l, &[]), Ok(rect));
        assert!(matches!(
            resolve("window:class:foot", &l, &[]),
            Err(Unresolved::Bad(_))
        ));
    }

    fn layout() -> Layout {
        let m = |id, name: &str, x| Monitor {
            id: MonitorId(id),
            name: name.into(),
            rect: LogicalRect::new(x, 0.0, 1920.0, 1080.0),
            scale: 1.0,
        };
        Layout::new(
            vec![m(0, "HDMI-A-1", 0.0), m(1, "DP-3", 1920.0)],
            vec![],
            MonitorId(1),
        )
    }

    #[test]
    fn regions() {
        let l = layout();
        let ring = [
            LogicalRect::new(1.0, 2.0, 3.0, 4.0),
            LogicalRect::new(5.0, 6.0, 7.0, 8.0),
        ];
        assert_eq!(
            resolve("all", &l, &[]),
            Ok(LogicalRect::new(0.0, 0.0, 3840.0, 1080.0))
        );
        assert_eq!(resolve("monitor", &l, &[]).unwrap().x, 1920.0);
        assert_eq!(resolve("monitor:HDMI-A-1", &l, &[]).unwrap().x, 0.0);
        assert_eq!(
            resolve("10,20.5 300x200", &l, &[]),
            Ok(LogicalRect::new(10.0, 20.5, 300.0, 200.0))
        );
        assert_eq!(resolve("last-region", &l, &ring), Ok(ring[0]));
        assert_eq!(resolve("last-region:2", &l, &ring), Ok(ring[1]));
        // Nothing there is nothing to capture; a malformed region is a
        // usage error.
        for missing in ["monitor:nope", "last-region:3", "active"] {
            assert!(
                matches!(resolve(missing, &l, &ring), Err(Unresolved::Missing(_))),
                "{missing}"
            );
        }
        for bad in [
            "last-region:0",
            "last-region:x",
            "window:class:foo",
            "10,20",
            "1,2 3x-4",
            "nonsense",
        ] {
            assert!(
                matches!(resolve(bad, &l, &ring), Err(Unresolved::Bad(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_box_on_no_monitor_is_off_screen() {
        // A short monitor beside a tall one: below it is a gap.
        let m = |id, x, h| Monitor {
            id: MonitorId(id),
            name: format!("M{id}"),
            rect: LogicalRect::new(x, 0.0, 1920.0, h),
            scale: 1.0,
        };
        let l = Layout::new(
            vec![m(0, 0.0, 1080.0), m(1, 1920.0, 2160.0)],
            vec![],
            MonitorId(0),
        );
        let on = |x, y, w, h| on_screen(LogicalRect::new(x, y, w, h), &l);
        assert!(on(10.0, 10.0, 100.0, 100.0));
        assert!(on(3800.0, 0.0, 100.0, 100.0), "partly off");
        assert!(!on(9000.0, 9000.0, 100.0, 100.0));
        assert!(!on(0.0, 0.0, 0.0, 0.0));
        assert!(!on(100.0, 1500.0, 100.0, 100.0), "in the gap");
    }
}
