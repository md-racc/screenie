//! Opening images for `--from FILE` and `edit FILE`: PNG, JPEG, WebP and
//! PPM/PGM, flattened over a background. Fixtures are 4x3: blue, with a red
//! top-left and a green bottom-right pixel.

use screenie_core::geometry::PixelPoint;
use screenie_image::{Rgb, decode};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn close(a: Rgb, b: Rgb, tol: u8) -> bool {
    a.0.abs_diff(b.0) <= tol && a.1.abs_diff(b.1) <= tol && a.2.abs_diff(b.2) <= tol
}

#[test]
fn every_format_reads_the_same_picture() {
    for (name, tol) in [
        ("rgb.png", 0),
        ("rgb.ppm", 0),
        ("rgb-ascii.ppm", 0),
        ("rgb.webp", 0),
        ("rgb.jpg", 40), // lossy, and 4x3 is all edges
    ] {
        let img =
            decode::decode(&fixture(name), Rgb(0, 0, 0)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((img.width(), img.height()), (4, 3), "{name}");
        let at = |x, y| img.rgb(PixelPoint::new(x, y)).unwrap();
        assert!(
            close(at(1, 1), Rgb(0x20, 0x40, 0x80), tol),
            "{name}: {:?}",
            at(1, 1)
        );
        if tol == 0 {
            assert_eq!(at(0, 0), Rgb(255, 0, 0), "{name}");
            assert_eq!(at(3, 2), Rgb(0, 255, 0), "{name}");
        }
    }
}

#[test]
fn grey_reads_as_grey_and_alpha_is_flattened_over_the_background() {
    let img = decode::decode(&fixture("grey.pgm"), Rgb(0, 0, 0)).unwrap();
    let c = img.rgb(PixelPoint::new(1, 1)).unwrap();
    assert!(c.0 == c.1 && c.1 == c.2, "{c:?}");
    for name in ["alpha.png", "alpha.webp"] {
        // rgba(32,64,128,0.5) over white: halfway to it.
        let img = decode::decode(&fixture(name), Rgb(255, 255, 255)).unwrap();
        let c = img.rgb(PixelPoint::new(0, 0)).unwrap();
        assert!(close(c, Rgb(143, 159, 191), 2), "{name}: {c:?}");
    }
}

#[test]
fn anything_else_says_what_it_takes() {
    let err = decode::decode(b"GIF89a....", Rgb(0, 0, 0)).unwrap_err();
    assert!(err.contains("PNG, JPEG, WebP or PPM"), "{err}");
    assert!(
        decode::decode(b"P6\n4 3\n255\n\x00", Rgb(0, 0, 0)).is_err(),
        "short"
    );
}

fn noise(w: u32, h: u32) -> screenie_image::Image {
    let data = (0..w * h)
        .flat_map(|i| {
            let v = (i.wrapping_mul(2654435761) >> 8) as u8;
            [v, v.wrapping_mul(3), v.wrapping_add(90), 255]
        })
        .collect();
    screenie_image::Image::from_rgba(w, h, data).unwrap()
}

#[test]
fn an_image_placed_on_a_monitor_crops_back_exactly() {
    use screenie_core::geometry::LogicalRect;
    let mon = LogicalRect::new(1920.0, 0.0, 1920.0, 1080.0);
    for (w, h, scale) in [
        (100, 50, 1.0),
        (3840, 1000, 1.0),
        (2500, 1000, 1.0),
        (300, 200, 2.0),
    ] {
        let img = noise(w, h);
        let (frame, rect) = screenie_image::compose::place(img.clone(), mon, scale, Rgb(0, 0, 0));
        let px = frame.map().to_pixels(rect);
        assert_eq!((px.w, px.h), (w as i32, h as i32), "{w}x{h}@{scale}");
        assert!(mon.contains(rect.centre()), "on the monitor");
        // Centred.
        let (cx, cy) = (rect.centre().x - mon.x, rect.centre().y - mon.y);
        assert!(
            (cx - 960.0).abs() <= 1.0 && (cy - 540.0).abs() <= 1.0,
            "{rect:?}"
        );
        assert_eq!(frame.crop(rect).data(), img.data(), "{w}x{h}@{scale}");
    }
}
