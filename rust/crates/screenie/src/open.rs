//! Pictures from elsewhere than the screen (`--from`, `edit FILE`): an
//! image file or stdin, shown on the focused monitor (or, for `shot`, on
//! its own), and screenie's own project PNGs.

use std::io::Read;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::{Layout, Monitor, MonitorId};
use screenie_core::model::ProjectFile;
use screenie_image::{Frame, Image, Picture, Rgb};

use crate::cli::Source;

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

/// A screenie project PNG's capture and project, if `bytes` is one.
pub fn project_in(bytes: &[u8]) -> Option<(Image, ProjectFile)> {
    let (img, text) = screenie_image::encode::read_png(bytes, crate::PROJECT_KEY).ok()?;
    Some((img, ProjectFile::from_json(&text?).ok()?))
}

/// A project's capture with its visible layers drawn in: what to show when
/// it can't be reopened editable (its monitors are gone). Never the bare
/// capture, which may hold what the layers censor.
pub fn flatten(img: Image, file: &ProjectFile) -> Image {
    let mut picture = Picture::new(Frame::new(img, file.layout.bounds));
    picture.sync(&file.project.layers, None, None);
    picture.committed().image().clone()
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
/// pixel: `all`, or `X,Y WxH` in the image's own pixels.
pub fn region(spec: &str, image: LogicalRect, k: f64) -> Result<LogicalRect, String> {
    let spec = spec.trim();
    if spec == "all" {
        return Ok(image);
    }
    let g = crate::region::geometry(spec)
        .ok_or_else(|| format!("bad region {spec:?}: all, or X,Y WxH in the image's pixels"))?;
    let r = LogicalRect::new(image.x + g.x / k, image.y + g.y / k, g.w / k, g.h / k);
    let inside = r.x >= image.x - 1e-9
        && r.y >= image.y - 1e-9
        && r.right() <= image.right() + 1e-9
        && r.bottom() <= image.bottom() + 1e-9;
    if r.w <= 0.0 || r.h <= 0.0 || !inside {
        return Err(format!(
            "{spec:?} isn't inside the image ({}x{})",
            (image.w * k).round(),
            (image.h * k).round()
        ));
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
        assert!(
            region("350,250 100x100", image, 2.0).is_err(),
            "off its edge"
        );
        assert!(region("monitor", image, 2.0).is_err());
    }

    #[test]
    fn a_project_flattens_with_its_layers_not_bare() {
        use screenie_core::model::{
            CensorKind, CensorOp, Layer, LayerId, Op, OpData, OpId, Project,
        };
        let mon = Monitor {
            id: MonitorId(0),
            name: "A".into(),
            rect: LogicalRect::new(0.0, 0.0, 20.0, 10.0),
            scale: 1.0,
        };
        let mut project = Project::new(LogicalRect::new(0.0, 0.0, 20.0, 10.0));
        project.layers.push(Layer {
            id: LayerId(1),
            clip: LogicalRect::new(0.0, 0.0, 20.0, 10.0),
            hidden: false,
            ops: vec![Op {
                id: OpId(2),
                data: OpData::Censor(CensorOp {
                    rect: LogicalRect::new(0.0, 0.0, 10.0, 10.0),
                    kind: CensorKind::Bar,
                    amount: 0.0,
                }),
            }],
        });
        let file = ProjectFile::new(project, one_monitor(&mon));
        let img = Image::filled(20, 10, Rgb(200, 200, 200));
        let flat = flatten(img, &file);
        let at = |x| {
            flat.rgb(screenie_core::geometry::PixelPoint::new(x, 5))
                .unwrap()
        };
        assert_eq!((at(2), at(15)), (Rgb(0, 0, 0), Rgb(200, 200, 200)));
    }
}
