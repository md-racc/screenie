//! The interactive picker: a session on the overlays.

use screenie_core::geometry::{LogicalPoint, LogicalRect};
use std::sync::mpsc::Receiver;

use screenie_core::session::{JobResult, RawEvent, Session};
use screenie_image::ui::{Ui, View};
use screenie_image::{Image, Picture};
use screenie_platform::wayland::{OverlayApp, Pointer};

pub struct Picker {
    pub session: Session,
    picture: Picture,
    pub ui: Ui,
    monitors: Vec<LogicalRect>,
    /// The session changed since the picture was last brought up to date.
    /// Input only marks this and drawing syncs, so a fast mouse costs one
    /// sync a frame rather than one an event: a pen trace in progress is
    /// re-rendered whole on each sync.
    stale: bool,
    /// Finished analysis jobs, from the analyzer's threads.
    jobs: Option<Receiver<(u64, JobResult)>>,
    /// Where the pointer is, for the palette item under it.
    hover: Option<LogicalPoint>,
    /// The left button went down on a chip: its drag and release are the
    /// chip's too (they do nothing).
    on_chip: bool,
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
            stale: true,
            hover: None,
            on_chip: false,
        };
        p.sync();
        p
    }

    /// Collect analysis results from here (see `analysis::Ocr::threaded`).
    pub fn take_jobs(&mut self, jobs: Receiver<(u64, JobResult)>) {
        self.jobs = Some(jobs);
    }

    /// The picture, up to date with the session.
    pub fn picture(&mut self) -> &Picture {
        self.sync();
        &self.picture
    }

    /// Bring the picture up to date with the session, if it changed.
    fn sync(&mut self) {
        if !std::mem::take(&mut self.stale) {
            return;
        }
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
        use screenie_core::session::Button;
        use screenie_image::ui::Hit;
        self.stale = true;
        // The chips take the left button: a palette item does what its key
        // would, and the picture under a chip never sees the press.
        match ev {
            RawEvent::ButtonPress {
                button: Button::Left,
                at,
            } => {
                if let Some(hit) = self.ui.hit(at) {
                    self.on_chip = true;
                    if let Hit::Item(intent) = hit {
                        self.session.act(intent);
                    }
                    return;
                }
            }
            RawEvent::Motion { at } => {
                self.hover = Some(at);
                if self.on_chip {
                    return;
                }
            }
            RawEvent::ButtonRelease {
                button: Button::Left,
                ..
            } if std::mem::take(&mut self.on_chip) => return,
            _ => {}
        }
        self.session.handle(ev, now);
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
            self.stale = true;
        }
        moved
    }

    fn wants_ticks(&self) -> bool {
        self.session.wants_ticks()
    }

    fn finished(&self) -> bool {
        self.session.is_finished()
    }

    fn pointer(&self) -> Pointer {
        use screenie_core::boxes::Handle;
        if self.on_chip || self.hover.is_some_and(|p| self.ui.hit(p).is_some()) {
            return Pointer::Default;
        }
        match self.session.pointer_handle() {
            Handle::None => Pointer::Default,
            Handle::Box => Pointer::Move,
            Handle::Edges([Some(_), None]) => Pointer::ResizeX,
            Handle::Edges([None, Some(_)]) => Pointer::ResizeY,
            Handle::Edges([Some(x), Some(y)]) if x == y => Pointer::ResizeNwse,
            Handle::Edges(_) => Pointer::ResizeNesw,
        }
    }

    fn draw(&mut self, index: usize, into: &mut Image) {
        self.sync();
        let mut view = View::of(&self.session);
        view.hover = self.hover;
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
    fn a_fast_pen_syncs_the_picture_once_a_frame_not_once_an_event() {
        let Some(mut p) = picker() else { return };
        let mut img = Image::new(1920, 1080);
        let key = |p: &mut Picker, pressed: bool| {
            let (code, key, mods) = (1, Key::Char('a'), Mods::default());
            let ev = if pressed {
                RawEvent::KeyPress { code, key, mods }
            } else {
                RawEvent::KeyRelease { code, key, mods }
            };
            p.input(ev, 0.0);
        };
        key(&mut p, true);
        key(&mut p, false);
        let at = LogicalPoint::new(150.0, 150.0);
        let left = screenie_core::session::Button::Left;
        p.input(RawEvent::ButtonPress { button: left, at }, 0.0);
        p.draw(0, &mut img);
        let before = p.picture.generation();
        for i in 0..500 {
            let at = LogicalPoint::new(110.0 + f64::from(i % 180), 150.0);
            p.input(RawEvent::Motion { at }, 0.0);
        }
        assert_eq!(p.picture.generation(), before, "input doesn't render");
        p.draw(0, &mut img);
        p.draw(1, &mut img);
        assert_eq!(p.picture.generation(), before + 1, "one sync for the frame");
        assert!(
            p.session.editing().is_some(),
            "the trace is still in progress"
        );
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

    fn left(p: &mut Picker, at: LogicalPoint, pressed: bool) {
        let button = screenie_core::session::Button::Left;
        let ev = if pressed {
            RawEvent::ButtonPress { button, at }
        } else {
            RawEvent::ButtonRelease { button, at }
        };
        p.input(ev, 0.0);
    }

    #[test]
    fn a_click_on_a_palette_item_does_what_its_key_does() {
        let Some(mut p) = picker() else { return };
        let mut img = Image::new(1920, 1080);
        p.draw(0, &mut img);
        let items = p.ui.items();
        let censor = screenie_core::intent::Intent::Tool(screenie_core::intent::Tool::Censor);
        let (r, _) = items
            .iter()
            .find(|(_, i)| *i == censor)
            .expect("a censor item");
        let at = r.centre();
        left(&mut p, at, true);
        left(&mut p, at, false);
        assert_eq!(p.session.mode_name(), "censor");
        // Hovered, it's drawn inverted: the frame differs, and only there.
        let mut q = picker().unwrap();
        let mut plain = Image::new(1920, 1080);
        q.draw(0, &mut plain);
        q.input(RawEvent::Motion { at }, 0.0);
        q.draw(0, &mut img);
        let changed = plain.damage(&img);
        assert!(!changed.is_empty());
        assert!(
            changed.iter().all(|t| {
                LogicalRect::new(
                    f64::from(t.x),
                    f64::from(t.y),
                    f64::from(t.w),
                    f64::from(t.h),
                )
                .intersects(r)
            }),
            "{changed:?}"
        );
    }

    #[test]
    fn a_press_on_a_chip_never_reaches_the_picture() {
        let Some(mut p) = picker() else { return };
        let mut img = Image::new(1920, 1080);
        p.draw(0, &mut img);
        let before = p.session.rect();
        // The status chip, below the box: a drag from it draws nothing.
        let (r, _) = p.ui.items().into_iter().next().expect("drawn items");
        let chip = LogicalPoint::new(r.x - 1.0, r.y + 1.0); // its line's padding
        assert!(p.ui.hit(chip).is_some());
        left(&mut p, chip, true);
        p.input(
            RawEvent::Motion {
                at: LogicalPoint::new(900.0, 700.0),
            },
            0.0,
        );
        left(&mut p, LogicalPoint::new(900.0, 700.0), false);
        assert_eq!(p.session.rect(), before);
        assert_eq!(p.session.mode_name(), "select");
    }

    /// Frame costs, for the performance audit (docs/architecture-rust.md
    /// §69): `cargo test --release -p screenie frame_costs -- --ignored
    /// --nocapture`. Two 1920x1080 monitors, a noisy frame, a big box.
    #[test]
    #[ignore]
    fn frame_costs() {
        use std::time::Instant;
        let Some(font) = screenie_platform::fonts::monospace() else {
            return;
        };
        let mon = |i, x| Monitor {
            id: MonitorId(i),
            name: format!("DP-{i}"),
            rect: LogicalRect::new(x, 0.0, 1920.0, 1080.0),
            scale: 1.0,
        };
        let layout = Layout::new(vec![mon(0, 0.0), mon(1, 1920.0)], vec![], MonitorId(0));
        let mut img = Image::new(3840, 1080);
        let mut seed = 1u32;
        for b in img.data_mut().chunks_exact_mut(4) {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            b.copy_from_slice(&[
                (seed >> 24) as u8,
                (seed >> 16) as u8,
                (seed >> 8) as u8,
                255,
            ]);
        }
        let frame = Frame::new(img, layout.bounds);
        let fresh = || {
            let session = Session::new(Setup {
                layout: layout.clone(),
                pixels: *frame.map(),
                start: Some(LogicalRect::new(200.0, 100.0, 1400.0, 800.0)),
                recall: vec![],
            });
            Picker::new(
                session,
                Picture::new(frame.clone()),
                Ui::new(&font).unwrap(),
            )
        };
        let key = |p: &mut Picker, k: Key, mods: Mods| {
            p.input(
                RawEvent::KeyPress {
                    code: 9,
                    key: k,
                    mods,
                },
                0.0,
            );
            p.input(
                RawEvent::KeyRelease {
                    code: 9,
                    key: k,
                    mods: Mods::default(),
                },
                0.0,
            );
        };
        let ch = |c| Key::Char(c);
        let none = Mods::default();
        let mut out = Image::new(1920, 1080);
        let mut shm = vec![0u8; 1920 * 1080 * 4];
        // Each step: `step` (input), then a frame of monitor 0, timed in parts.
        let mut run = |name: &str,
                       p: &mut Picker,
                       steps: usize,
                       step: &mut dyn FnMut(&mut Picker, usize)| {
            p.draw(0, &mut out); // warm the caches
            let (mut sync, mut ui, mut copy, mut diff, mut area) = (0.0, 0.0, 0.0, 0.0, 0.0);
            let mut prev = out.clone();
            for i in 0..steps {
                step(p, i);
                let t = Instant::now();
                p.sync();
                sync += t.elapsed().as_secs_f64();
                let t = Instant::now();
                let view = View::of(&p.session);
                let m = p.monitors[0];
                p.ui.draw(&p.picture, 0, m, &view, &mut out);
                ui += t.elapsed().as_secs_f64();
                let t = Instant::now();
                let d = out.damage(&prev);
                diff += t.elapsed().as_secs_f64();
                area += d.iter().map(|r| f64::from(r.w * r.h)).sum::<f64>() / (1920.0 * 1080.0);
                prev.data_mut().copy_from_slice(out.data());
                let t = Instant::now();
                for (d, s) in shm.chunks_exact_mut(4).zip(out.data().chunks_exact(4)) {
                    d.copy_from_slice(&[s[2], s[1], s[0], 255]);
                }
                copy += t.elapsed().as_secs_f64();
            }
            let n = steps as f64 / 1000.0;
            eprintln!(
                "{name:<22} sync {:>5.2}  ui {:>5.2}  copy {:>5.2}  diff {:>5.2}  ms/frame; damaged {:>5.1}%",
                sync / n,
                ui / n,
                copy / n,
                diff / n,
                area / steps as f64 * 100.0
            );
        };
        let mut p = fresh();
        run("select, idle", &mut p, 50, &mut |_, _| {});
        let mut p = fresh();
        run("select, box moving", &mut p, 50, &mut |p, _| {
            key(p, ch('l'), none)
        });
        let mut p = fresh();
        key(&mut p, ch('a'), none);
        let left = screenie_core::session::Button::Left;
        p.input(
            RawEvent::ButtonPress {
                button: left,
                at: LogicalPoint::new(300.0, 300.0),
            },
            0.0,
        );
        run("annotate, pen (3000)", &mut p, 3000, &mut |p, i| {
            let at = LogicalPoint::new(
                300.0 + (i % 1000) as f64 * 1.2,
                300.0 + (i / 1000) as f64 * 100.0 + (i as f64 / 10.0).sin() * 50.0,
            );
            p.input(RawEvent::Motion { at }, 0.0);
        });
        let mut p = fresh();
        key(&mut p, ch('x'), none);
        run("censor, box moving", &mut p, 50, &mut |p, _| {
            key(p, ch('l'), none)
        });
        let mut p = fresh();
        key(&mut p, ch('f'), none);
        key(&mut p, ch('5'), none);
        run("filter blur, moving", &mut p, 20, &mut |p, _| {
            key(p, ch('h'), none)
        });
        eprintln!("  filter status: {}", p.session.status());
        let mut p = fresh();
        key(&mut p, ch('z'), none);
        run("loupe, moving", &mut p, 50, &mut |p, _| {
            key(p, ch('l'), none)
        });
    }
}
