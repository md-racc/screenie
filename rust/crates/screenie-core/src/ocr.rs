//! OCR results as plain data (README, OCR): the full text, and each word
//! with its box and confidence. Shared by OCR output and, later,
//! auto-redact and scale-bar reading. The engine lives in the platform
//! crate.

use crate::geometry::PixelRect;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrWord {
    pub text: String,
    /// In frame pixels.
    pub bbox: PixelRect,
    /// 0-100, as the engine reports it; `None` if it doesn't.
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrResult {
    pub text: String,
    pub words: Vec<OcrWord>,
}

impl OcrResult {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
