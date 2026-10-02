//! OCR end to end: text drawn with the tag font, read back by the real
//! tesseract. Skipped when tesseract or a font isn't installed.

use screenie_core::geometry::{LogicalRect, PixelRect};
use screenie_image::ui::{Paint, fill};
use screenie_image::{Frame, Image, Picture, Rgb, encode};

fn text_image(dark: bool) -> Option<Picture> {
    let font = screenie_platform::fonts::monospace()?;
    let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).ok()?;
    let (bg, fg): (u8, u8) = if dark { (30, 220) } else { (240, 20) };
    let mut img = Image::filled(600, 60, Rgb(bg, bg, bg));
    let mut pen = 20.0f32;
    for c in "The quick brown fox".chars() {
        let (m, cov) = font.rasterize(c, 22.0);
        for y in 0..m.height {
            for x in 0..m.width {
                let a = f32::from(cov[y * m.width + x]) / 255.0;
                let px = pen as i32 + m.xmin + x as i32;
                let py = 40 - m.height as i32 - m.ymin + y as i32;
                let v = f32::from(fg) / 255.0;
                fill(
                    &mut img,
                    PixelRect::new(px, py, 1, 1),
                    Paint::rgba(v, v, v, a),
                );
            }
        }
        pen += m.advance_width;
    }
    Some(Picture::new(Frame::new(
        img,
        LogicalRect::new(0.0, 0.0, 600.0, 60.0),
    )))
}

fn have_tesseract() -> bool {
    std::process::Command::new("tesseract")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn reads_text_and_its_word_boxes_light_or_dark() {
    if !have_tesseract() {
        eprintln!("no tesseract: skipped");
        return;
    }
    for dark in [false, true] {
        let Some(pic) = text_image(dark) else { return };
        let rect = LogicalRect::new(0.0, 0.0, 600.0, 60.0);
        let (img, origin, up) = pic.ocr_crop(rect);
        assert_eq!(up, 2);
        let png = encode::png(&img, 1).unwrap();
        let up = up as i32;
        let r = screenie_platform::ocr::recognize(&png, "eng", |b| {
            PixelRect::new(origin.x + b.x / up, origin.y + b.y / up, b.w / up, b.h / up)
        })
        .unwrap();
        assert!(r.text.contains("quick brown"), "dark {dark}: {:?}", r.text);
        let quick = r
            .words
            .iter()
            .find(|w| w.text == "quick")
            .expect("the word");
        assert!(quick.bbox.x > 50 && quick.bbox.x < 150, "{:?}", quick.bbox); // in frame pixels
        assert!(quick.confidence.unwrap() > 50.0);
    }
}
