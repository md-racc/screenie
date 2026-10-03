//! OCR end to end: text drawn with the tag font, read back by the real
//! tesseract. Skipped when tesseract or a font isn't installed.

use screenie_core::geometry::{LogicalRect, PixelRect};
use screenie_image::ui::{Paint, fill};
use screenie_image::{Frame, Image, Picture, Rgb, encode};

fn text_image(text: &str, dark: bool) -> Option<Picture> {
    let font = screenie_platform::fonts::monospace()?;
    let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).ok()?;
    let (bg, fg): (u8, u8) = if dark { (30, 220) } else { (240, 20) };
    let mut img = Image::filled(600, 60, Rgb(bg, bg, bg));
    let mut pen = 20.0f32;
    for c in text.chars() {
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
        let Some(pic) = text_image("The quick brown fox", dark) else {
            return;
        };
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

fn read(pic: &Picture) -> screenie_core::ocr::OcrResult {
    let (img, origin, up) = pic.ocr_crop(LogicalRect::new(0.0, 0.0, 600.0, 60.0));
    let png = encode::png(&img, 1).unwrap();
    let up = up as i32;
    screenie_platform::ocr::recognize(&png, "eng", |b| {
        PixelRect::new(origin.x + b.x / up, origin.y + b.y / up, b.w / up, b.h / up)
    })
    .unwrap()
}

#[test]
fn auto_redact_finds_secrets_in_real_ocr() {
    if !have_tesseract() {
        eprintln!("no tesseract: skipped");
        return;
    }
    let Some(pic) = text_image("mail adi@example.org from 10.0.0.12", false) else {
        return;
    };
    let env = screenie_core::redact::Env {
        user: "adi".into(),
        home: "/home/adi".into(),
        hostname: "myhost".into(),
    };
    let found = screenie_core::redact::propose(&read(&pic), &env, 0.2);
    let names: Vec<&str> = found.iter().map(|p| p.category.name()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(
        names.contains(&"email") && names.contains(&"ip"),
        "{names:?}"
    );
    // The email's box covers its text and not "mail" before it.
    let email = found.iter().find(|p| p.category.name() == "email").unwrap();
    assert!(email.rect.x > 60 && email.rect.x < 100, "{:?}", email.rect);
    assert!(
        email.rect.w > 150 && email.rect.y < 25 && email.rect.y + email.rect.h > 40,
        "{:?}",
        email.rect
    );
}

#[test]
fn auto_redact_finds_a_card_and_its_security_code_in_real_ocr() {
    if !have_tesseract() {
        eprintln!("no tesseract: skipped");
        return;
    }
    let Some(pic) = text_image("4111 1111 1111 1111 CVV 123", false) else {
        return;
    };
    let found = screenie_core::redact::propose(&read(&pic), &Default::default(), 0.2);
    let names: Vec<&str> = found.iter().map(|p| p.category.name()).collect();
    assert_eq!(names, ["card", "security-code"], "{names:?}");
}

#[test]
fn a_scale_bar_reads_from_real_ocr() {
    if !have_tesseract() {
        eprintln!("no tesseract: skipped");
        return;
    }
    let Some(pic) = text_image("0      50      100 m", false) else {
        return;
    };
    let r = read(&pic);
    let at = |t: &str| {
        let w = r.words.iter().find(|w| w.text == t).expect(t);
        screenie_core::geometry::PixelPoint::new(w.bbox.x + w.bbox.w / 2, w.bbox.y - 6)
    };
    // A line drawn along the bar, from its 0 to its 100. Tesseract tends to
    // drop the lone "0", so its start is a bar's length left of the 100
    // from the 50.
    let (fifty, hundred) = (at("50"), at("100"));
    let zero = screenie_core::geometry::PixelPoint::new(2 * fifty.x - hundred.x, hundred.y);
    let got = screenie_core::measure::scale_bar(&r, zero, hundred);
    assert_eq!(got.as_deref(), Some("100 m"), "{:?}", r.text);
}
