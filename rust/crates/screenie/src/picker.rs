//! The interactive picker: a session on the overlays.

use screenie_core::geometry::LogicalRect;
use std::sync::mpsc::Receiver;

use screenie_core::session::{JobResult, RawEvent, Session};
use screenie_image::ui::{Ui, View};
use screenie_image::{Image, Picture};
use screenie_platform::wayland::OverlayApp;

pub struct Picker {
    pub session: Session,
    pub picture: Picture,
    pub ui: Ui,
    monitors: Vec<LogicalRect>,
    /// Finished analysis jobs, from the analyzer's threads.
    jobs: Option<Receiver<(u64, JobResult)>>,
}

impl Picker {
    pub fn new(session: Session, picture: Picture, ui: Ui) -> Self {
        let monitors = session.layout().monitor_rects();
        let mut p = Self {
            session,
            picture,
            ui,
            monitors,
            jobs: None,
        };
        p.sync();
        p
    }

    /// Collect analysis results from here (see `analysis::Ocr::threaded`).
    pub fn take_jobs(&mut self, jobs: Receiver<(u64, JobResult)>) {
        self.jobs = Some(jobs);
    }

    /// Bring the picture up to date with the session.
    fn sync(&mut self) {
        let s = &self.session;
        self.picture
            .sync(s.layers(), s.open_layer(), s.editing().as_ref());
        self.picture.inspect(s.inspected());
    }
}

/// `Q`'s reader: zbar over the committed pixels of the box.
pub struct Codes(pub Picture);

impl screenie_core::session::CodeReader for Codes {
    fn read(&mut self, s: &Session, rect: LogicalRect) -> Result<Option<String>, String> {
        self.0.sync(s.layers(), None, None);
        let png = screenie_image::encode::png(&self.0.crop(rect), 1).map_err(|e| e.to_string())?;
        let dir = crate::files::runtime_dir().map_err(|e| format!("runtime dir: {e}"))?;
        screenie_platform::codes::decode(&png, &dir)
    }
}

impl OverlayApp for Picker {
    fn input(&mut self, ev: RawEvent, now: f64) {
        self.session.handle(ev, now);
        self.sync();
    }

    fn tick(&mut self, now: f64) -> bool {
        let mut moved = self.session.tick(now);
        if let Some(jobs) = &self.jobs {
            for (id, result) in jobs.try_iter() {
                self.session.job_done(id, result);
                moved = true;
            }
        }
        if moved {
            self.sync();
        }
        moved
    }

    fn wants_ticks(&self) -> bool {
        self.session.wants_ticks()
    }

    fn finished(&self) -> bool {
        self.session.is_finished()
    }

    fn draw(&mut self, index: usize, into: &mut Image) {
        let view = View::of(&self.session);
        let monitor = self.monitors[index];
        self.ui.draw(&self.picture, index, monitor, &view, into);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::geometry::{LogicalPoint, PixelPoint};
    use screenie_core::keys::{Key, Mods};
    use screenie_core::layout::{Layout, Monitor, MonitorId};
    use screenie_core::session::{Outcome, Setup};
    use screenie_image::{Frame, Rgb};

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
        Some(Picker::new(
            session,
            Picture::new(frame),
            Ui::new(&font).ok()?,
        ))
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
