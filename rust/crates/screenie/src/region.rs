//! REGION (docs/cli.md, SOURCES AND REGIONS), resolved on a layout.

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;

/// A region as written, resolved to a logical rectangle. `ring` is the
/// recall ring on this layout, newest first. Errors are usage errors.
pub fn resolve(spec: &str, layout: &Layout, ring: &[LogicalRect]) -> Result<LogicalRect, String> {
    let spec = spec.trim();
    match spec {
        "all" => return Ok(layout.bounds),
        "monitor" => {
            return layout
                .monitor(layout.focused)
                .map(|m| m.rect)
                .ok_or_else(|| "no focused monitor".into());
        }
        "active" => {
            return Err("active (a window's own pixels) isn't in the port yet; \
                        use pick, or `shot monitor`"
                .into());
        }
        _ => {}
    }
    if let Some(name) = spec.strip_prefix("monitor:") {
        return layout
            .monitors
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.rect)
            .ok_or_else(|| format!("no monitor named {name:?}"));
    }
    if spec.starts_with("window:") {
        return Err("window regions aren't in the port yet".into());
    }
    if let Some(rest) = spec.strip_prefix("last-region") {
        let n: usize = match rest.strip_prefix(':') {
            Some(n) => n.parse().map_err(|_| format!("bad region {spec:?}"))?,
            None if rest.is_empty() => 1,
            None => return Err(format!("bad region {spec:?}")),
        };
        return n
            .checked_sub(1)
            .and_then(|i| ring.get(i))
            .copied()
            .ok_or_else(|| format!("no {spec} (the ring has {})", ring.len()));
    }
    geometry(spec).ok_or_else(|| format!("bad region {spec:?}: expected all, monitor, X,Y WxH..."))
}

/// slurp's `X,Y WxH`.
fn geometry(s: &str) -> Option<LogicalRect> {
    let (xy, wh) = s.split_once(' ')?;
    let (x, y) = xy.split_once(',')?;
    let (w, h) = wh.trim().split_once('x')?;
    let n = |v: &str| v.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    let (w, h) = (n(w)?, n(h)?);
    (w >= 0.0 && h >= 0.0).then_some(LogicalRect::new(n(x)?, n(y)?, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::layout::{Monitor, MonitorId};

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
        for bad in [
            "monitor:nope",
            "last-region:3",
            "last-region:0",
            "10,20",
            "1,2 3x-4",
            "nonsense",
        ] {
            assert!(resolve(bad, &l, &ring).is_err(), "{bad}");
        }
    }
}
