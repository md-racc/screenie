//! Pictures from elsewhere than the screen (`--from`, `edit FILE`): an
//! image file or stdin, shown on the focused monitor (or, for `shot`, on
//! its own), and screenie's own project PNGs.

use std::io::Read;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::{Layout, Monitor, MonitorId};
use screenie_core::model::{Project, ProjectFile};
use screenie_image::{Frame, Image, Picture, Rgb};

use crate::cli::Source;
use crate::region::Unresolved;

/// What's round an image shown on a monitor.
pub const BACKGROUND: Rgb = Rgb(24, 24, 28);

/// The bytes of an image source.
pub fn read(source: &Source) -> Result<Vec<u8>, String> {
    use screenie_platform::clipboard::Pasted;
    match source {
        Source::File(path) => std::fs::read(path).map_err(|e| format!("{}: {e}", path.display())),
        Source::Stdin => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| format!("reading the image from stdin: {e}"))?;
            if bytes.is_empty() {
                return Err("no image on stdin".into());
            }
            Ok(bytes)
        }
        Source::Clipboard => match screenie_platform::clipboard::paste_image()? {
            // A shot's project, so it reopens with its layers.
            Some(Pasted::Shot(shot)) => {
                let project = crate::files::project_of(&shot);
                std::fs::read(if project.is_file() { &project } else { &shot })
                    .map_err(|e| format!("{}: {e}", shot.display()))
            }
            Some(Pasted::Image(bytes)) => Ok(bytes),
            None => Err("no image on the clipboard".into()),
        },
        Source::Screen | Source::Last(_) | Source::Recent => Err("not an image source".into()),
    }
}

/// The PNG text keyword a project travels under.
const PROJECT_KEY: &str = "screenie-project";

/// A project file: the uncensored capture, with the project and layout.
pub fn project_png(base: &Image, project: &Project, layout: &Layout) -> Result<Vec<u8>, String> {
    let json = ProjectFile::new(project.clone(), layout.clone()).to_json();
    screenie_image::encode::png_with_text(base, 1, Some((PROJECT_KEY, &json)))
        .map_err(|e| e.to_string())
}

/// A screenie project PNG's capture and project, if `bytes` is one. One
/// this screenie can't read (a newer screenie's, say) is an error: its
/// pixels are the bare capture, which may hold what the layers censor.
pub fn project_in(bytes: &[u8]) -> Result<Option<(Image, ProjectFile)>, String> {
    let Ok((img, Some(text))) = screenie_image::encode::read_png(bytes, PROJECT_KEY) else {
        return Ok(None);
    };
    let file = ProjectFile::from_json(&text)
        .map_err(|e| format!("a screenie project this screenie can't open: {e}"))?;
    Ok(Some((img, file)))
}

/// The monospace font, set as the one text and callout numbers are drawn
/// in: anything that draws layers needs it first, or they're drawn without
/// their text. None if there's no such font.
pub fn font() -> Option<&'static [u8]> {
    static FONT: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let font = screenie_platform::fonts::monospace()?;
        screenie_image::render::set_font(&font).ok()?;
        Some(font)
    })
    .as_deref()
}

/// A project's capture with its visible layers drawn in, over its layout:
/// what to deliver or show when it isn't reopened editable. Never the bare
/// capture, which may hold what the layers censor.
pub fn flattened(img: Image, file: &ProjectFile) -> Frame {
    font();
    let mut picture = Picture::new(Frame::new(img, file.layout.bounds));
    picture.sync(&file.project.layers, None, None);
    picture.committed().clone()
}

/// `flattened`'s pixels.
pub fn flatten(img: Image, file: &ProjectFile) -> Image {
    flattened(img, file).image().clone()
}

/// An image source's picture: a project PNG with its layers drawn in, else
/// the image decoded.
pub fn image(bytes: &[u8]) -> Result<Image, String> {
    match project_in(bytes)? {
        Some((img, file)) => Ok(flatten(img, &file)),
        None => screenie_image::decode::decode(bytes, BACKGROUND),
    }
}

/// A layout of just one monitor (where an image is shown).
pub fn one_monitor(m: &Monitor) -> Layout {
    Layout::new(vec![m.clone()], vec![], m.id)
}

/// A monitor an image is shown on, for `shot` (no display): its own size,
/// one logical pixel to a pixel.
pub fn its_own(img: &Image) -> Monitor {
    Monitor {
        id: MonitorId(0),
        name: "image".into(),
        rect: LogicalRect::new(0.0, 0.0, f64::from(img.width()), f64::from(img.height())),
        scale: 1.0,
    }
}

/// A region over an image whose rect is `image` at `k` pixels a logical
/// pixel: `all`, or `X,Y WxH` in the image's own pixels. A rectangle must
/// lie within the image: one with no area, or off it, has nothing there
/// (`Missing`); one partly off it, or any other region, is `Bad`.
pub fn region(spec: &str, image: LogicalRect, k: f64) -> Result<LogicalRect, Unresolved> {
    let spec = spec.trim();
    if spec == "all" {
        return Ok(image);
    }
    let g = crate::region::geometry(spec).ok_or_else(|| {
        Unresolved::Bad(format!(
            "bad region {spec:?}: all, or X,Y WxH in the image's pixels"
        ))
    })?;
    let r = LogicalRect::new(image.x + g.x / k, image.y + g.y / k, g.w / k, g.h / k);
    let size = format!("{}x{}", (image.w * k).round(), (image.h * k).round());
    if r.w <= 0.0 || r.h <= 0.0 {
        return Err(Unresolved::Missing("the region is empty".into()));
    }
    if !r.intersects(&image) {
        return Err(Unresolved::Missing(format!(
            "the region is outside the image ({size})"
        )));
    }
    let inside = r.x >= image.x - 1e-9
        && r.y >= image.y - 1e-9
        && r.right() <= image.right() + 1e-9
        && r.bottom() <= image.bottom() + 1e-9;
    if !inside {
        return Err(Unresolved::Bad(format!(
            "{spec:?} isn't inside the image ({size})"
        )));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_over_an_image_are_in_its_pixels() {
        // A 400x300 image at 2 pixels a logical pixel, at 100,50.
        let image = LogicalRect::new(100.0, 50.0, 200.0, 150.0);
        assert_eq!(region("all", image, 2.0).unwrap(), image);
        assert_eq!(
            region("10,20 100x60", image, 2.0).unwrap(),
            LogicalRect::new(105.0, 60.0, 50.0, 30.0)
        );
        // Nothing there: exit 3.
        for missing in ["0,0 0x5", "900,900 10x10"] {
            assert!(
                matches!(region(missing, image, 2.0), Err(Unresolved::Missing(_))),
                "{missing}"
            );
        }
        // Partly off its edge, or not a region over an image: exit 2.
        for bad in ["350,250 100x100", "monitor"] {
            assert!(
                matches!(region(bad, image, 2.0), Err(Unresolved::Bad(_))),
                "{bad}"
            );
        }
    }

    /// A 20x10 project on one monitor: a black bar over the left half, and
    /// a hidden one over the right.
    fn barred() -> ProjectFile {
        use screenie_core::model::{CensorKind, CensorOp, Layer, LayerId, Op, OpData, OpId};
        let mon = Monitor {
            id: MonitorId(0),
            name: "A".into(),
            rect: LogicalRect::new(0.0, 0.0, 20.0, 10.0),
            scale: 1.0,
        };
        let bar = |id, x, hidden| Layer {
            id: LayerId(id),
            clip: LogicalRect::new(0.0, 0.0, 20.0, 10.0),
            hidden,
            ops: vec![Op {
                id: OpId(id + 10),
                data: OpData::Censor(CensorOp {
                    rect: LogicalRect::new(x, 0.0, 10.0, 10.0),
                    kind: CensorKind::Bar,
                    amount: 0.0,
                }),
            }],
        };
        let mut project = Project::new(LogicalRect::new(0.0, 0.0, 20.0, 10.0));
        project.layers = vec![bar(1, 0.0, false), bar(2, 10.0, true)];
        ProjectFile::new(project, one_monitor(&mon))
    }

    #[test]
    fn a_project_flattens_with_its_layers_not_bare() {
        let file = barred();
        let img = Image::filled(20, 10, Rgb(200, 200, 200));
        let flat = flatten(img.clone(), &file);
        let at = |x| {
            flat.rgb(screenie_core::geometry::PixelPoint::new(x, 5))
                .unwrap()
        };
        assert_eq!((at(2), at(15)), (Rgb(0, 0, 0), Rgb(200, 200, 200)));
        let half = flattened(img, &file).crop(LogicalRect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(
            half.rgb(screenie_core::geometry::PixelPoint::new(5, 5)),
            Some(Rgb(0, 0, 0))
        );
    }

    #[test]
    fn a_project_png_reads_back_as_written() {
        let file = barred();
        let img = Image::filled(20, 10, Rgb(200, 200, 200));
        let png = project_png(&img, &file.project, &file.layout).unwrap();
        let (back, read) = project_in(&png).unwrap().unwrap();
        assert_eq!((back.width(), back.height()), (20, 10));
        assert_eq!(read, file);
        // Its picture is drawn in; a plain PNG's is itself.
        let drawn = image(&png).unwrap();
        assert_eq!(
            drawn.rgb(screenie_core::geometry::PixelPoint::new(2, 5)),
            Some(Rgb(0, 0, 0))
        );
        let plain = screenie_image::encode::png(&img, 1).unwrap();
        assert!(project_in(&plain).unwrap().is_none());
        assert!(project_in(b"not an image").unwrap().is_none());
        assert_eq!(
            image(&plain)
                .unwrap()
                .rgb(screenie_core::geometry::PixelPoint::new(2, 5)),
            Some(Rgb(200, 200, 200))
        );
    }

    #[test]
    fn a_project_it_cant_read_is_an_error_never_the_bare_capture() {
        let file = barred();
        let img = Image::filled(20, 10, Rgb(200, 200, 200));
        let with = |json: String| {
            screenie_image::encode::png_with_text(&img, 1, Some((PROJECT_KEY, &json))).unwrap()
        };
        let mut newer = serde_json::to_value(&file).unwrap();
        newer["project"]["version"] = (screenie_core::model::VERSION + 1).into();
        let newer = with(newer.to_string());
        assert!(
            project_in(&newer)
                .unwrap_err()
                .contains("newer than this screenie")
        );
        assert!(image(&newer).is_err());
        let garbage = with(r#"{"format":"screenie-project","garbage":1}"#.into());
        assert!(project_in(&garbage).is_err() && image(&garbage).is_err());
    }

    #[test]
    fn a_flattened_project_has_its_text_and_callout_numbers() {
        use screenie_core::geometry::{LogicalPoint, PixelPoint};
        use screenie_core::model::{CalloutOp, Layer, LayerId, Op, OpData, OpId, TextOp};
        if screenie_platform::fonts::monospace().is_none() {
            return;
        }
        let mon = Monitor {
            id: MonitorId(0),
            name: "A".into(),
            rect: LogicalRect::new(0.0, 0.0, 100.0, 40.0),
            scale: 1.0,
        };
        let op = |id, data| Op { id: OpId(id), data };
        let mut project = Project::new(mon.rect);
        project.layers.push(Layer {
            id: LayerId(1),
            clip: mon.rect,
            hidden: false,
            ops: vec![
                op(
                    2,
                    OpData::Text(TextOp {
                        origin: LogicalPoint::new(4.0, 4.0),
                        text: "WWW".into(),
                        rgb: [0.0; 3],
                        size: 20.0,
                        chip: false,
                    }),
                ),
                op(
                    3,
                    OpData::Callout(CalloutOp {
                        centre: LogicalPoint::new(80.0, 20.0),
                        n: 8,
                        rgb: [1.0, 0.0, 0.0],
                        size: 30.0,
                    }),
                ),
            ],
        });
        let file = ProjectFile::new(project, one_monitor(&mon));
        let bg = Rgb(255, 255, 255);
        let flat = flatten(Image::filled(100, 40, bg), &file);
        let differ = |x0, x1, than| {
            (x0..x1).any(|x| (4..36).any(|y| flat.rgb(PixelPoint::new(x, y)) != Some(than)))
        };
        assert!(differ(4, 50, bg), "the text is drawn");
        // Inside the disc (radius 15 round 80,20), the number's white ink.
        let inked = (74..87).any(|x| {
            (12..28).any(|y| {
                matches!(flat.rgb(PixelPoint::new(x, y)),
                    Some(Rgb(r, g, b)) if r > 200 && g > 128 && b > 128)
            })
        });
        assert!(inked, "the callout's number is drawn");
    }
}
