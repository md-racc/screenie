//! The prototype's test fixture (tests/test_modes.py): two 1920x1080
//! monitors side by side at scale 1, three tiled windows, and a starting
//! box at 100,170 560x90.

#![allow(dead_code)]

use screenie_core::geometry::{LogicalPoint, LogicalRect, PixelMap};
use screenie_core::keys::{Key, Mods};
use screenie_core::layout::{Layout, Monitor, MonitorId, Window, WindowId};
use screenie_core::session::{Button, RawEvent, Session, Setup};

pub const fn r(x: f64, y: f64, w: f64, h: f64) -> LogicalRect {
    LogicalRect::new(x, y, w, h)
}

pub const MONS: [LogicalRect; 2] = [r(0.0, 0.0, 1920.0, 1080.0), r(1920.0, 0.0, 1920.0, 1080.0)];
pub const WINDOWS: [LogicalRect; 3] = [
    r(1923.0, 3.0, 955.0, 1074.0),
    r(2883.0, 3.0, 954.0, 1074.0),
    r(3.0, 3.0, 955.0, 1074.0),
];
pub const START: LogicalRect = r(100.0, 170.0, 560.0, 90.0);
pub const LAYOUT: LogicalRect = r(0.0, 0.0, 3840.0, 1080.0);

pub fn layout() -> Layout {
    let monitors = MONS
        .iter()
        .enumerate()
        .map(|(i, &rect)| Monitor {
            id: MonitorId(i as u32),
            name: format!("DP-{}", i + 1),
            rect,
            scale: 1.0,
        })
        .collect();
    let windows = WINDOWS
        .iter()
        .enumerate()
        .map(|(i, &rect)| Window {
            id: WindowId(i as u64),
            rect,
            monitor: None,
            class: format!("w{i}"),
            title: String::new(),
        })
        .collect();
    Layout::new(monitors, windows, MonitorId(0))
}

pub fn session_at(start: LogicalRect, recall: Vec<LogicalRect>) -> Session {
    Session::new(Setup {
        layout: layout(),
        pixels: PixelMap::new(LAYOUT, 3840, 1080),
        start: Some(start),
        recall,
    })
}

pub fn session() -> Session {
    session_at(START, vec![])
}

pub fn p(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y)
}

/// Raw-event helpers, for what a script can't say (timing, holding).
pub trait Drive {
    fn keys(&mut self, script: &str);
    fn press(&mut self, code: u32, name: &str, mods: Mods, now: f64);
    fn release(&mut self, code: u32, name: &str, mods: Mods, now: f64);
    fn button(&mut self, button: Button, at: LogicalPoint, pressed: bool);
    fn motion(&mut self, at: LogicalPoint);
}

impl Drive for Session {
    fn keys(&mut self, script: &str) {
        self.play(script, 0.0).unwrap();
    }

    fn press(&mut self, code: u32, name: &str, mods: Mods, now: f64) {
        let key = Key::from_name(name);
        self.handle(RawEvent::KeyPress { code, key, mods }, now);
    }

    fn release(&mut self, code: u32, name: &str, mods: Mods, now: f64) {
        let key = Key::from_name(name);
        self.handle(RawEvent::KeyRelease { code, key, mods }, now);
    }

    fn button(&mut self, button: Button, at: LogicalPoint, pressed: bool) {
        let ev = if pressed {
            RawEvent::ButtonPress { button, at }
        } else {
            RawEvent::ButtonRelease { button, at }
        };
        self.handle(ev, 0.0);
    }

    fn motion(&mut self, at: LogicalPoint) {
        self.handle(RawEvent::Motion { at }, 0.0);
    }
}
