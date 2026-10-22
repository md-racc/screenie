//! Auto-redact's OCR jobs, of the committed picture as it is when they're
//! asked for: cropped and read by tesseract inline (`--keys`, `--no-ui`)
//! or on a thread (the overlay), whose results the picker collects on
//! ticks.
//!
//! Scans are kept, by picture version and rect (docs/auto-redact.md,
//! Performance): a scan of a rect inside one already made, of the same
//! pixels, is that scan's words and codes there, at once. A prescan reads
//! each monitor in the background when the picker opens, so the first `r`
//! finds it (or waits for it, rather than starting another).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use screenie_core::geometry::PixelRect;
use screenie_core::model::Layer;
use screenie_core::ocr::OcrResult;
use screenie_core::session::{Analyzer, JobKind, JobResult, Session};
use screenie_image::Picture;

/// A crop, ready for tesseract: the PNG, its origin and upscale.
pub struct Crop {
    png: Vec<u8>,
    origin: (i32, i32),
    up: i32,
}

impl Crop {
    pub fn of(picture: &Picture, r: PixelRect) -> Result<Crop, String> {
        let (img, origin, up) = picture.ocr_crop_pixels(r);
        let png = screenie_image::encode::png(&img, 1).map_err(|e| e.to_string())?;
        Ok(Crop {
            png,
            origin: (origin.x, origin.y),
            up: up as i32,
        })
    }

    /// Run tesseract, and with `codes` zbar; boxes come back in the
    /// picture's pixels.
    pub fn recognize(&self, lang: &str, codes: bool) -> Result<OcrResult, String> {
        let (ox, oy, up) = (self.origin.0, self.origin.1, self.up);
        let map = |b: PixelRect| PixelRect::new(ox + b.x / up, oy + b.y / up, b.w / up, b.h / up);
        let mut result = screenie_platform::ocr::recognize(&self.png, lang, map)?;
        if codes {
            let scratch = crate::files::runtime_dir().map_err(|e| format!("runtime dir: {e}"))?;
            result.codes = screenie_platform::codes::find(&self.png, &scratch, map)?;
        }
        Ok(result)
    }
}

/// What a scan covered.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    version: u64,
    rect: PixelRect,
    codes: bool,
}

impl Key {
    /// Whether a scan of this answers `want`.
    fn covers(&self, want: &Key) -> bool {
        self.version == want.version
            && (self.codes || !want.codes)
            && contains(self.rect, want.rect)
    }
}

/// Scans made and making, shared with their threads.
#[derive(Default)]
struct Scans {
    done: Vec<(Key, OcrResult)>,
    running: Vec<Key>,
}

/// How many finished scans are kept.
const KEPT: usize = 8;

type Shared = Arc<(Mutex<Scans>, Condvar)>;

impl Scans {
    fn answer(&self, want: &Key) -> Option<OcrResult> {
        self.done
            .iter()
            .rev()
            .find(|(k, _)| k.covers(want))
            .map(|(_, r)| within(r, want.rect))
    }

    fn finish(&mut self, key: Key, result: &Result<OcrResult, String>) {
        self.running.retain(|k| *k != key);
        if let Ok(r) = result {
            self.done.push((key, r.clone()));
            let extra = self.done.len().saturating_sub(KEPT);
            self.done.drain(..extra);
        }
    }
}

fn contains(outer: PixelRect, inner: PixelRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.w <= outer.x + outer.w
        && inner.y + inner.h <= outer.y + outer.h
}

fn meets(a: PixelRect, b: PixelRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// A scan's words and codes that reach into `rect` (a secret the box cuts
/// still counts), and its text rebuilt from those words.
fn within(r: &OcrResult, rect: PixelRect) -> OcrResult {
    let words: Vec<_> = r
        .words
        .iter()
        .filter(|w| meets(w.bbox, rect))
        .cloned()
        .collect();
    let mut text = String::new();
    let mut line = None;
    for w in &words {
        if line.is_some() {
            text.push(if line == Some(w.line) { ' ' } else { '\n' });
        }
        text.push_str(&w.text);
        line = Some(w.line);
    }
    OcrResult {
        text,
        words,
        codes: r
            .codes
            .iter()
            .filter(|c| meets(c.bbox, rect))
            .cloned()
            .collect(),
    }
}

pub struct Ocr {
    picture: Picture,
    lang: String,
    /// Threaded: where results go.
    results: Option<Sender<(u64, JobResult)>>,
    scans: Shared,
}

impl Ocr {
    /// Answers each job before returning.
    pub fn inline(picture: Picture, lang: String) -> Ocr {
        Ocr {
            picture,
            lang,
            results: None,
            scans: Shared::default(),
        }
    }

    /// Answers on a thread; collect the results from the receiver.
    pub fn threaded(picture: Picture, lang: String) -> (Ocr, Receiver<(u64, JobResult)>) {
        let (tx, rx) = channel();
        let ocr = Ocr {
            picture,
            lang,
            results: Some(tx),
            scans: Shared::default(),
        };
        (ocr, rx)
    }

    /// Scan each of `rects` (with `codes` when the detectors include them)
    /// in the background, as the session's committed picture is now (its
    /// `layers`), for scans inside them to find.
    pub fn prescan(&mut self, layers: &[Layer], rects: &[PixelRect], codes: bool) {
        let keys = self.prescan_keys(layers, rects, codes);
        let frame = self.picture.committed().clone();
        self.scans.0.lock().expect("scans").running.extend(&keys);
        let (scans, lang) = (self.scans.clone(), self.lang.clone());
        std::thread::spawn(move || {
            // The crops are made here, so the picker's first frame isn't
            // kept waiting.
            let picture = Picture::new(frame);
            for key in keys {
                let result =
                    Crop::of(&picture, key.rect).and_then(|c| c.recognize(&lang, key.codes));
                let (lock, ready) = &*scans;
                lock.lock().expect("scans").finish(key, &result);
                ready.notify_all();
            }
        });
    }

    /// What a prescan of `rects` will scan: the picture with `layers`
    /// synced in, as a job's sync will see it.
    fn prescan_keys(&mut self, layers: &[Layer], rects: &[PixelRect], codes: bool) -> Vec<Key> {
        self.picture.sync(layers, None, None);
        let version = self.picture.version();
        rects
            .iter()
            .map(|&rect| Key {
                version,
                rect,
                codes,
            })
            .collect()
    }
}

impl Analyzer for Ocr {
    fn start(&mut self, s: &Session, id: u64, job: JobKind) -> Option<JobResult> {
        let JobKind::Ocr { rect, codes } = job;
        self.picture.sync(s.layers(), None, None);
        let key = Key {
            version: self.picture.version(),
            rect,
            codes,
        };
        let pending = {
            let scans = self.scans.0.lock().expect("scans");
            if let Some(r) = scans.answer(&key) {
                return Some(JobResult::Ocr(Ok(r)));
            }
            scans.running.iter().any(|k| k.covers(&key))
        };
        let Some(tx) = &self.results else {
            let result = Crop::of(&self.picture, rect).and_then(|c| c.recognize(&self.lang, codes));
            self.scans.0.lock().expect("scans").finish(key, &result);
            return Some(JobResult::Ocr(result));
        };
        // The crop is made on the thread too, so the overlay isn't kept
        // waiting (the frame's pixels are shared).
        let frame = self.picture.committed().clone();
        let (tx, lang, scans) = (tx.clone(), self.lang.clone(), self.scans.clone());
        if !pending {
            scans.0.lock().expect("scans").running.push(key);
        }
        std::thread::spawn(move || {
            let (lock, ready) = &*scans;
            if pending {
                // A scan covering this one is under way: its answer, when
                // it's in; or this one's own, if it failed.
                let mut guard = lock.lock().expect("scans");
                loop {
                    if let Some(r) = guard.answer(&key) {
                        let _ = tx.send((id, JobResult::Ocr(Ok(r))));
                        return;
                    }
                    if !guard.running.iter().any(|k| k.covers(&key)) {
                        break;
                    }
                    guard = ready.wait(guard).expect("scans");
                }
            }
            let result =
                Crop::of(&Picture::new(frame), rect).and_then(|c| c.recognize(&lang, codes));
            lock.lock().expect("scans").finish(key, &result);
            ready.notify_all();
            // The picker may be gone by now: then nobody wants it.
            let _ = tx.send((id, JobResult::Ocr(result)));
        });
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::ocr::{OcrCode, OcrWord};

    fn word(text: &str, x: i32, line: u32) -> OcrWord {
        OcrWord {
            text: text.into(),
            bbox: PixelRect::new(x, line as i32 * 20, 40, 12),
            confidence: Some(90.0),
            line,
        }
    }

    fn scan() -> OcrResult {
        OcrResult {
            text: String::new(),
            words: vec![word("a", 0, 0), word("b", 100, 0), word("c", 100, 1)],
            codes: vec![OcrCode {
                text: "q".into(),
                bbox: PixelRect::new(300, 0, 50, 50),
            }],
        }
    }

    #[test]
    fn a_scan_inside_one_made_is_answered_from_it() {
        let mut s = Scans::default();
        let all = Key {
            version: 1,
            rect: PixelRect::new(0, 0, 400, 100),
            codes: true,
        };
        s.finish(all, &Ok(scan()));
        let want = |rect, version, codes| Key {
            version,
            rect,
            codes,
        };
        // Words reaching into the rect, the text rebuilt from them.
        let r = s
            .answer(&want(PixelRect::new(90, 0, 60, 40), 1, true))
            .unwrap();
        assert_eq!(r.text, "b\nc");
        assert!(r.codes.is_empty());
        let r = s
            .answer(&want(PixelRect::new(120, 0, 250, 15), 1, false))
            .unwrap();
        assert_eq!((r.text.as_str(), r.codes.len()), ("b", 1));
        // Not other pixels, nor outside what was scanned.
        assert!(
            s.answer(&want(PixelRect::new(0, 0, 10, 10), 2, false))
                .is_none()
        );
        assert!(
            s.answer(&want(PixelRect::new(390, 0, 20, 10), 1, false))
                .is_none()
        );
        // Nor codes from a scan that didn't look for them.
        let mut t = Scans::default();
        t.finish(
            Key {
                codes: false,
                ..all
            },
            &Ok(scan()),
        );
        assert!(
            t.answer(&want(PixelRect::new(0, 0, 10, 10), 1, true))
                .is_none()
        );
    }

    #[test]
    fn failed_scans_are_not_kept_and_old_ones_go() {
        let mut s = Scans::default();
        let key = |x| Key {
            version: 1,
            rect: PixelRect::new(x, 0, 10, 10),
            codes: false,
        };
        s.running.push(key(0));
        s.finish(key(0), &Err("no".into()));
        assert!(s.running.is_empty() && s.done.is_empty());
        for x in 0..20 {
            s.finish(key(x), &Ok(OcrResult::default()));
        }
        assert_eq!(s.done.len(), KEPT);
        assert!(s.answer(&key(19)).is_some() && s.answer(&key(0)).is_none());
    }

    /// A small picture on one monitor, and a session over it.
    fn setup() -> (Picture, Session) {
        use screenie_core::geometry::LogicalRect;
        use screenie_core::layout::{Layout, Monitor, MonitorId};
        use screenie_core::session::Setup;
        use screenie_image::{Frame, Image, Rgb};
        let mon = Monitor {
            id: MonitorId(0),
            name: "A".into(),
            rect: LogicalRect::new(0.0, 0.0, 40.0, 20.0),
            scale: 2.0,
        };
        let layout = Layout::new(vec![mon], vec![], MonitorId(0));
        let frame = Frame::new(Image::filled(80, 40, Rgb(250, 250, 250)), layout.bounds);
        let session = Session::new(Setup {
            layout,
            pixels: *frame.map(),
            start: Some(LogicalRect::new(0.0, 0.0, 40.0, 20.0)),
            recall: vec![],
        });
        (Picture::new(frame), session)
    }

    #[test]
    fn a_prescan_is_of_the_picture_the_scans_will_ask_about() {
        use screenie_core::geometry::LogicalRect;
        use screenie_core::model::{CensorKind, CensorOp, LayerId, Op, OpData, OpId};
        let (picture, _) = setup();
        let (mut ocr, _rx) = Ocr::threaded(picture, "eng".into());
        let layers = [Layer {
            id: LayerId(1),
            clip: LogicalRect::new(0.0, 0.0, 40.0, 20.0),
            hidden: false,
            ops: vec![Op {
                id: OpId(2),
                data: OpData::Censor(CensorOp {
                    rect: LogicalRect::new(0.0, 0.0, 10.0, 10.0),
                    kind: CensorKind::Bar,
                    amount: 0.0,
                }),
            }],
        }];
        let rects = [PixelRect::new(0, 0, 80, 40)];
        let before = ocr.picture.version();
        let keys = ocr.prescan_keys(&layers, &rects, false);
        let version = ocr.picture.version();
        assert_ne!(version, before, "the layers are synced in");
        assert_eq!(keys.len(), 1);
        // Without codes in the detectors, it doesn't look for any.
        assert!(
            keys.iter().all(|k| k.version == version && !k.codes),
            "{keys:?}"
        );
        let keys = ocr.prescan_keys(&layers, &rects, true);
        assert!(keys.iter().all(|k| k.version == version && k.codes));
        // And the prescan itself starts, whatever its thread finds.
        ocr.prescan(&layers, &rects, false);
        // A job's sync, with the same layers, leaves it as it is.
        ocr.picture.sync(&layers, None, None);
        assert_eq!(ocr.picture.version(), version);
    }

    #[test]
    fn a_threaded_job_answers_on_the_receiver_and_finishes_its_scan() {
        let (picture, session) = setup();
        let (mut ocr, rx) = Ocr::threaded(picture, "eng".into());
        let job = JobKind::Ocr {
            rect: PixelRect::new(0, 0, 80, 40),
            codes: false,
        };
        assert!(ocr.start(&session, 7, job).is_none(), "not at once");
        // Read, or failed with no tesseract: either way it comes back.
        let (id, result) = rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert!(id == 7 && matches!(result, JobResult::Ocr(_)));
        assert!(ocr.scans.0.lock().unwrap().running.is_empty());
    }
}
