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

#[test]
fn what_screenie_writes_it_reads_back() {
    let img = noise(64, 48);
    let webp = screenie_image::encode::webp(&img).unwrap();
    assert_eq!(
        decode::decode(&webp, Rgb(0, 0, 0)).unwrap().data(),
        img.data(),
        "lossless"
    );
    let ppm = screenie_image::encode::ppm(&img);
    assert_eq!(
        decode::decode(&ppm, Rgb(0, 0, 0)).unwrap().data(),
        img.data()
    );
    // JPEG is lossy: close on a smooth picture, and smaller at lower quality.
    let smooth = screenie_image::Image::filled(64, 48, Rgb(40, 120, 200));
    let hi = screenie_image::encode::jpeg(&smooth, 95).unwrap();
    let back = decode::decode(&hi, Rgb(0, 0, 0)).unwrap();
    let c = back.rgb(PixelPoint::new(30, 20)).unwrap();
    assert!(close(c, Rgb(40, 120, 200), 4), "{c:?}");
    let lo = screenie_image::encode::jpeg(&img, 30).unwrap();
    assert!(lo.len() < screenie_image::encode::jpeg(&img, 95).unwrap().len());
}

/// The CRC a PNG chunk ends with (over its type and data).
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

#[test]
fn a_huge_image_is_refused_before_it_is_allocated() {
    // A real 1x1 PNG whose header claims 200000x200000 (120 GB as RGBA).
    let mut png =
        screenie_image::encode::png(&screenie_image::Image::filled(1, 1, Rgb(1, 2, 3)), 1).unwrap();
    assert_eq!(crc32(b"IEND"), 0xAE42_6082); // the CRC is right
    png[16..20].copy_from_slice(&200_000u32.to_be_bytes());
    png[20..24].copy_from_slice(&200_000u32.to_be_bytes());
    let crc = crc32(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());
    let err = decode::decode(&png, Rgb(0, 0, 0)).unwrap_err();
    assert!(err.contains("too big"), "{err}");
    let err = decode::decode(b"P6\n200000 200000\n255\n\x00", Rgb(0, 0, 0)).unwrap_err();
    assert!(err.contains("too big"), "{err}");
}

#[test]
fn a_ppm_with_bad_numbers_is_refused_not_a_panic() {
    let bad = |b: &[u8]| decode::decode(b, Rgb(0, 0, 0)).unwrap_err();
    // Width times height past 32 bits: caught by the size cap.
    assert!(bad(b"P6\n65536 65537\n255\n").contains("too big"));
    // Samples past 32 bits, and past the maxval.
    assert!(bad(b"P3\n1 1\n255\n4294967295 0 0\n").contains("a bad sample"));
    assert!(bad(b"P3\n1 1\n255\n300 0 0\n").contains("a bad sample"));
    let ok = decode::decode(b"P3\n1 1\n255\n255 0 0\n", Rgb(0, 0, 0)).unwrap();
    assert_eq!(ok.rgb(PixelPoint::new(0, 0)), Some(Rgb(255, 0, 0)));
}

/// A JPEG of a red left half and a blue right half, with an EXIF block
/// giving `orientation` (none for none).
fn jpeg_turned(orientation: Option<u16>) -> Vec<u8> {
    let mut img = screenie_image::Image::filled(16, 8, Rgb(255, 0, 0));
    let blue = screenie_image::Image::filled(8, 8, Rgb(0, 0, 255));
    img.blit(&blue, blue.extent(), 8, 0);
    let jpeg = screenie_image::encode::jpeg(&img, 95).unwrap();
    let Some(o) = orientation else { return jpeg };
    // APP1: "Exif", then a little-endian TIFF header and one directory
    // with one entry: Orientation, SHORT, 1, the value.
    let mut tiff = b"II*\0".to_vec();
    tiff.extend(8u32.to_le_bytes());
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(0x0112u16.to_le_bytes());
    tiff.extend(3u16.to_le_bytes());
    tiff.extend(1u32.to_le_bytes());
    tiff.extend(o.to_le_bytes());
    tiff.extend([0, 0]);
    tiff.extend(0u32.to_le_bytes());
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend(u16::try_from(2 + 6 + tiff.len()).unwrap().to_be_bytes());
    app1.extend(b"Exif\0\0");
    app1.extend(tiff);
    [&jpeg[..2], &app1, &jpeg[2..]].concat()
}

#[test]
fn a_jpeg_is_turned_upright_by_its_exif_orientation() {
    let (red, blue) = (Rgb(255, 0, 0), Rgb(0, 0, 255));
    for (o, size, first) in [
        (None, (16, 8), red),
        (Some(1), (16, 8), red),
        (Some(2), (16, 8), blue), // mirrored
        (Some(6), (8, 16), red),  // turned clockwise: the left on top
        (Some(8), (8, 16), blue), // anticlockwise: the right on top
        (Some(42), (16, 8), red), // nonsense: as stored
    ] {
        let img = decode::decode(&jpeg_turned(o), Rgb(0, 0, 0)).unwrap();
        assert_eq!((img.width(), img.height()), size, "{o:?}");
        let at = |x, y| img.rgb(PixelPoint::new(x, y)).unwrap();
        // Its two ends: top and bottom if it's tall, else left and right.
        let (w, h) = (size.0 as i32, size.1 as i32);
        let (a, b) = if h > w {
            (at(w / 2, 1), at(w / 2, h - 2))
        } else {
            (at(1, h / 2), at(w - 2, h / 2))
        };
        let other = if first == red { blue } else { red };
        assert!(
            close(a, first, 40) && close(b, other, 40),
            "{o:?}: {a:?} {b:?}"
        );
    }
}
