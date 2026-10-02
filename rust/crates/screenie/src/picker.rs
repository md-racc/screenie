//! The interactive picker: a session on the overlays.

use screenie_core::geometry::LogicalRect;
use screenie_core::session::{RawEvent, Session};
use screenie_image::Frame;
use screenie_image::Image;
use screenie_image::ui::{Ui, View};
use screenie_platform::wayland::OverlayApp;

pub struct Picker {
    pub session: Session,
    pub frame: Frame,
    pub ui: Ui,
    monitors: Vec<LogicalRect>,
}

impl Picker {
    pub fn new(session: Session, frame: Frame, ui: Ui) -> Self {
        let monitors = session.layout().monitor_rects();
        Self {
            session,
            frame,
            ui,
            monitors,
        }
    }

    fn view(&self) -> View {
        let s = &self.session;
        View {
            rect: s.rect(),
            cursor: s.selection().cursor_point(),
            active: true,
            status: s.status(),
            top: if s.show_help() { s.legend() } else { vec![] },
            monitors: self.monitors.clone(),
        }
    }
}

impl OverlayApp for Picker {
    fn input(&mut self, ev: RawEvent, now: f64) {
        self.session.handle(ev, now);
    }

    fn tick(&mut self, now: f64) -> bool {
        self.session.tick(now)
    }

    fn wants_ticks(&self) -> bool {
        self.session.wants_ticks()
    }

    fn finished(&self) -> bool {
        self.session.is_finished()
    }

    fn draw(&mut self, index: usize, into: &mut Image) {
        let view = self.view();
        let monitor = self.monitors[index];
        self.ui.draw(&self.frame, index, monitor, &view, into);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::geometry::{LogicalPoint, PixelPoint};
    use screenie_core::keys::{Key, Mods};
    use screenie_core::layout::{Layout, Monitor, MonitorId};
    use screenie_core::session::{Outcome, Setup};
    use screenie_image::Rgb;

    fn picker() -> Option<Picker> {
        let font = screenie_platform::fonts::monospace()?;
        let mon = |i, x| Monitor {
            id: MonitorId(i),
            name: format!("DP-{i}"),
            rect: LogicalRect::new(x, 0.0, 1920.0, 1080.0),
            scale: 1.0,
        };
        let layout = Layout::new(vec![mon(0, 0.0), mon(1, 1920.0)], vec![], MonitorId(0));
        let frame = Frame::new(Image::filled(3840, 1080, Rgb(100, 100, 100)), layout.bounds);
        let session = Session::new(Setup {
            layout,
            pixels: *frame.map(),
            start: Some(LogicalRect::new(100.0, 100.0, 200.0, 100.0)),
            recall: vec![],
        });
        Some(Picker::new(session, frame, Ui::new(&font).ok()?))
    }

    #[test]
    fn raw_input_reaches_the_session_and_the_picture_follows() {
        let Some(mut p) = picker() else { return };
        let mut img = Image::new(1920, 1080);
        p.draw(0, &mut img);
        assert_eq!(img.rgb(PixelPoint::new(150, 150)), Some(Rgb(100, 100, 100)));
        p.input(
            RawEvent::KeyPress {
                code: 1,
                key: Key::Char('l'),
                mods: Mods::SHIFT,
            },
            0.0,
        );
        p.input(
            RawEvent::KeyRelease {
                code: 1,
                key: Key::Char('l'),
                mods: Mods::SHIFT,
            },
            0.0,
        );
        p.draw(0, &mut img);
        assert_eq!(img.rgb(PixelPoint::new(102, 150)), Some(Rgb(50, 50, 50))); // left of the moved box
        assert!(!p.finished());
        let at = LogicalPoint::new(10.0, 10.0);
        p.input(
            RawEvent::ButtonPress {
                button: screenie_core::session::Button::Right,
                at,
            },
            0.0,
        );
        assert!(p.finished());
        assert!(matches!(p.session.outcome(), Some(Outcome::Done(_))));
    }
}
