//! Auto-redact's OCR jobs. The picker's crop is taken at once, from the
//! committed picture; tesseract then runs inline (`--keys`, `--no-ui`) or
//! on a thread (the overlay), whose results the picker collects on ticks.

use std::sync::mpsc::{Receiver, Sender, channel};

use screenie_core::geometry::PixelRect;
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

    /// Run tesseract; word boxes come back in the picture's pixels.
    pub fn recognize(&self, lang: &str) -> Result<OcrResult, String> {
        let (ox, oy, up) = (self.origin.0, self.origin.1, self.up);
        screenie_platform::ocr::recognize(&self.png, lang, |b| {
            PixelRect::new(ox + b.x / up, oy + b.y / up, b.w / up, b.h / up)
        })
    }
}

pub struct Ocr {
    picture: Picture,
    lang: String,
    /// Threaded: where results go.
    results: Option<Sender<(u64, JobResult)>>,
}

impl Ocr {
    /// Answers each job before returning.
    pub fn inline(picture: Picture, lang: String) -> Ocr {
        Ocr {
            picture,
            lang,
            results: None,
        }
    }

    /// Answers on a thread; collect the results from the receiver.
    pub fn threaded(picture: Picture, lang: String) -> (Ocr, Receiver<(u64, JobResult)>) {
        let (tx, rx) = channel();
        let ocr = Ocr {
            picture,
            lang,
            results: Some(tx),
        };
        (ocr, rx)
    }
}

impl Analyzer for Ocr {
    fn start(&mut self, s: &Session, id: u64, job: JobKind) -> Option<JobResult> {
        let JobKind::Ocr { rect } = job;
        self.picture.sync(s.layers(), None, None);
        let crop = match Crop::of(&self.picture, rect) {
            Ok(c) => c,
            Err(e) => return Some(JobResult::Ocr(Err(e))),
        };
        let Some(tx) = &self.results else {
            return Some(JobResult::Ocr(crop.recognize(&self.lang)));
        };
        let (tx, lang) = (tx.clone(), self.lang.clone());
        std::thread::spawn(move || {
            // The picker may be gone by now: then nobody wants it.
            let _ = tx.send((id, JobResult::Ocr(crop.recognize(&lang))));
        });
        None
    }
}
