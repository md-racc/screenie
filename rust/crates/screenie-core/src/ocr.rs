//! OCR results as plain data (README, OCR): the full text, and each word
//! with its box and confidence. Shared by OCR output, auto-redact and
//! Measure's scale-bar reading. The engine lives in the platform crate.

use crate::geometry::PixelRect;

/// A recognised word.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrWord {
    pub text: String,
    /// In frame pixels.
    pub bbox: PixelRect,
    /// 0-100, as the engine reports it; `None` if it doesn't.
    pub confidence: Option<f32>,
    /// Which line of the text it's on (0 first), so lines can be rebuilt.
    pub line: u32,
}

/// A QR code or barcode found with the words (an auto-redact scan).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrCode {
    /// What it decodes to.
    pub text: String,
    /// Its outline's bounds, in frame pixels.
    pub bbox: PixelRect,
}

/// One OCR pass: the plain text, its words, and any codes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrResult {
    /// Words joined by spaces, lines by newlines, paragraphs by a blank
    /// line.
    pub text: String,
    pub words: Vec<OcrWord>,
    /// Only when the job asked for them.
    pub codes: Vec<OcrCode>,
}

impl OcrResult {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
