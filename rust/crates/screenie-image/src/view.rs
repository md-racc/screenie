//! View filters: display-only transforms for the lenses. They never reach
//! the project or the output.

use screenie_core::lens::ViewFilter;

use crate::image::Image;

fn lum(p: &[u8]) -> f32 {
    0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2])
}

/// One S-curve pass around mid-grey (overlaying a picture on itself).
pub(crate) fn s_curve(v: f32) -> f32 {
    let x = v / 255.0;
    let y = if x < 0.5 {
        2.0 * x * x
    } else {
        1.0 - 2.0 * (1.0 - x) * (1.0 - x)
    };
    y * 255.0
}

/// Whether a greyscale picture is mostly dark: the mean of a 40x20 grid
/// of samples below mid-grey (the reading view flips those, as does OCR).
pub fn is_dark(img: &Image) -> bool {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return false;
    }
    let mut total = 0.0;
    for j in 0..20 {
        let y = (j * 2 + 1) * h / 40;
        for i in 0..40 {
            let x = (i * 2 + 1) * w / 80;
            total += f32::from(img.data()[(y * w + x) * 4 + 1]);
        }
    }
    total / 800.0 < 128.0
}

/// Apply a view filter to a whole image.
pub fn apply(img: &mut Image, filter: ViewFilter) {
    let set = |img: &mut Image, f: &dyn Fn(&[u8]) -> [u8; 3]| {
        for px in img.data_mut().chunks_exact_mut(4) {
            let c = f(px);
            px[..3].copy_from_slice(&c);
        }
    };
    let grey = |p: &[u8]| [lum(p).round() as u8; 3];
    match filter {
        ViewFilter::Off => {}
        ViewFilter::Grey => set(img, &grey),
        ViewFilter::Invert => set(img, &|p| [255 - p[0], 255 - p[1], 255 - p[2]]),
        ViewFilter::Contrast => {
            set(img, &|p| [s_curve(lum(p)).round() as u8; 3]);
            if is_dark(img) {
                set(img, &|p| [255 - p[0], 255 - p[1], 255 - p[2]]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rgb;
    use screenie_core::geometry::PixelPoint;

    fn at(img: &Image) -> Rgb {
        img.rgb(PixelPoint::new(0, 0)).unwrap()
    }

    #[test]
    fn grey_and_invert() {
        let mut img = Image::filled(2, 2, Rgb(200, 100, 50));
        apply(&mut img, ViewFilter::Invert);
        assert_eq!(at(&img), Rgb(55, 155, 205));
        apply(&mut img, ViewFilter::Grey);
        let v = at(&img).0;
        assert_eq!(at(&img), Rgb(v, v, v));
    }

    #[test]
    fn contrast_reads_dark_pictures_as_dark_on_light() {
        let mut dark = Image::filled(80, 40, Rgb(30, 30, 40));
        apply(&mut dark, ViewFilter::Contrast);
        assert!(at(&dark).0 > 200, "{:?}", at(&dark)); // flipped to light
        let mut light = Image::filled(80, 40, Rgb(220, 220, 220));
        apply(&mut light, ViewFilter::Contrast);
        assert!(at(&light).0 > 230); // stays light, and lighter
    }
}
