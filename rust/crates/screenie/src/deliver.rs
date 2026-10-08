//! Delivering a result: the runtime copy and `last`, `--save`, stdout,
//! the clipboard, and a notification.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use screenie_image::{Image, encode};
use screenie_platform::clipboard::{self, Offer};
use screenie_platform::notify::notify;

use crate::cli::{Format, Output};

/// `--format jpeg` without `--quality`.
pub const JPEG_QUALITY: u8 = 90;
use crate::files;

#[derive(Debug, Default)]
pub struct Delivered {
    /// Where `--save` put it.
    pub saved: Option<PathBuf>,
    /// The runtime copy (what `last` points at).
    pub shot: Option<PathBuf>,
    pub copied: bool,
}

pub fn notifies(out: &Output) -> bool {
    out.notify
        .unwrap_or_else(|| !std::io::stdout().is_terminal())
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("can't find screenie itself: {e}"))
}

fn write_file(
    dir: &std::path::Path,
    stem: &str,
    ext: &str,
    mode: u32,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let (path, mut f) = files::create_unique(dir, stem, ext, mode)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A project to keep beside the image, encoded only if it's needed.
pub type ProjectBytes<'a> = &'a dyn Fn() -> Result<Vec<u8>, String>;

/// Write a shot's project beside it: private, never overwriting.
fn write_project(beside: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = files::project_of(beside);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Deliver an image. `what` names it for the notification ("Area").
/// `project`, if given, is kept beside the runtime copy (and beside the
/// `--save` copy with `--save-project`).
pub fn image(
    img: &Image,
    out: &Output,
    what: &str,
    project: Option<ProjectBytes>,
    naming: &files::Naming,
) -> Result<Delivered, String> {
    let png = encode::png(img, out.png_level).map_err(|e| format!("encoding: {e}"))?;
    // What's handed out is in --format; history keeps PNG (last, projects).
    let bytes = match out.format {
        Format::Png => png.clone(),
        Format::Jpeg => encode::jpeg(img, out.quality.unwrap_or(JPEG_QUALITY))?,
        Format::Webp => encode::webp(img)?,
        Format::Ppm => encode::ppm(img),
    };
    let ext = out.format.ext();
    let mut d = Delivered::default();
    let mut project_bytes: Option<Vec<u8>> = None;
    let mut project_once = |p: ProjectBytes| -> Result<Vec<u8>, String> {
        if project_bytes.is_none() {
            project_bytes = Some(p()?);
        }
        Ok(project_bytes.clone().expect("just made"))
    };
    if !out.no_history {
        let runtime = files::runtime_dir().map_err(|e| format!("runtime dir: {e}"))?;
        let shots = runtime.join("shots");
        let path = write_file(&shots, &files::stamp(), "png", 0o600, &png)?;
        if let Some(p) = project {
            write_project(&path, &project_once(p)?)?;
        }
        files::prune(&shots, files::KEEP);
        let _ = std::fs::write(runtime.join("last"), format!("{}\n", path.display()));
        d.shot = Some(path);
    }
    if let Some(dir) = &out.save {
        // A directory, or a template naming the file; the extension
        // always follows --format, so the name says what's in it.
        let text = dir.as_deref().map(|d| d.to_string_lossy().to_string());
        let (dir, stem) = match text {
            Some(t) if files::is_template(&t) => {
                let (dir, stem, _) = files::render_template(&t, &jiff::Zoned::now(), naming)?;
                (dir, stem)
            }
            _ => (
                files::expand(dir.as_deref().unwrap_or(&files::pictures_dir())),
                files::stamp(),
            ),
        };
        let path = write_file(&dir, &stem, ext, 0o644, &bytes)?;
        if let (true, Some(p)) = (out.save_project, project) {
            write_project(&path, &project_once(p)?)?;
        }
        d.saved = Some(path);
    }
    if out.stdout {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| format!("stdout: {e}"))?;
    }
    if out.copies() {
        // The chosen format first, and PNG for apps that take only that.
        let mut offers = vec![Offer {
            mime: out.format.mime().into(),
            data: bytes,
        }];
        if out.format != Format::Png {
            offers.push(Offer {
                mime: "image/png".into(),
                data: png,
            });
        }
        if let Some(shot) = &d.shot {
            offers.push(Offer {
                mime: clipboard::SHOT_MIME.into(),
                data: shot.display().to_string().into_bytes(),
            });
        }
        if let (Some(p), false) = (&d.saved, out.no_copy_uri) {
            offers.push(Offer {
                mime: "text/uri-list".into(),
                data: format!("file://{}\r\n", p.display()).into_bytes(),
            });
        }
        clipboard::copy(&exe()?, &offers).map_err(|e| format!("clipboard: {e}"))?;
        d.copied = true;
    }
    if notifies(out) {
        let body = match (&d.saved, d.copied) {
            (Some(p), _) => format!("{what} saved to {}", p.display()),
            (None, true) => format!("{what} copied to clipboard"),
            (None, false) => what.to_string(),
        };
        notify("Screenshot", &body);
    }
    Ok(d)
}

/// An image of the logical `rect` (on `monitors`, if it's of the screen),
/// at the resolution `--scale` asks for: resized only if that differs.
pub fn at_scale(
    img: Image,
    rect: screenie_core::geometry::LogicalRect,
    monitors: &[screenie_core::layout::Monitor],
    scale: crate::cli::OutputScale,
) -> Image {
    use crate::cli::OutputScale;
    let on: Vec<f64> = monitors
        .iter()
        .filter(|m| m.rect.intersects(&rect))
        .map(|m| m.scale)
        .collect();
    // Off-screen (an opened image): its own pixels are native.
    let own = f64::from(img.width()) / rect.w;
    let k = match scale {
        OutputScale::Native => on
            .iter()
            .copied()
            .fold(None, |a: Option<f64>, s| Some(a.map_or(s, |a| a.max(s)))),
        OutputScale::Min => on
            .iter()
            .copied()
            .fold(None, |a: Option<f64>, s| Some(a.map_or(s, |a| a.min(s)))),
        OutputScale::Factor(f) => Some(f),
    }
    .unwrap_or(own);
    let (w, h) = (
        (rect.w * k).round().max(1.0) as u32,
        (rect.h * k).round().max(1.0) as u32,
    );
    if (w, h) == (img.width(), img.height()) {
        img
    } else {
        screenie_image::compose::resize(&img, w, h)
    }
}

/// A colour as `--colour-format` writes it.
pub fn colour(c: screenie_image::Rgb, f: crate::cli::ColourFormat) -> String {
    use crate::cli::ColourFormat;
    match f {
        ColourFormat::Hex => c.hex(),
        ColourFormat::Rgb => format!("rgb({}, {}, {})", c.0, c.1, c.2),
        ColourFormat::Hsl => {
            let [r, g, b] = [c.0, c.1, c.2].map(|v| f64::from(v) / 255.0);
            let (max, min) = (r.max(g).max(b), r.min(g).min(b));
            let l = (max + min) / 2.0;
            let d = max - min;
            let s = if d == 0.0 {
                0.0
            } else {
                d / (1.0 - (2.0 * l - 1.0).abs())
            };
            let h = if d == 0.0 {
                0.0
            } else if max == r {
                60.0 * ((g - b) / d).rem_euclid(6.0)
            } else if max == g {
                60.0 * ((b - r) / d + 2.0)
            } else {
                60.0 * ((r - g) / d + 4.0)
            };
            format!(
                "hsl({}, {}%, {}%)",
                h.round(),
                (s * 100.0).round(),
                (l * 100.0).round()
            )
        }
    }
}

/// Deliver a text result (a colour, a code). `title` heads the
/// notification ("Colour copied").
pub fn text(text: &str, out: &Output, title: &str) -> Result<(), String> {
    if out.stdout {
        println!("{text}");
    }
    if out.copies() {
        // The names wl-copy offers text under, for older apps too.
        let offers: Vec<Offer> = [
            "text/plain;charset=utf-8",
            "text/plain",
            "UTF8_STRING",
            "STRING",
            "TEXT",
        ]
        .into_iter()
        .map(|mime| Offer {
            mime: mime.into(),
            data: text.as_bytes().to_vec(),
        })
        .collect();
        clipboard::copy(&exe()?, &offers).map_err(|e| format!("clipboard: {e}"))?;
    }
    if notifies(out) {
        notify(title, &text.chars().take(200).collect::<String>());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColourFormat;
    use screenie_image::Rgb;

    #[test]
    fn the_output_scale_is_the_boxs_monitors() {
        use crate::cli::OutputScale;
        use screenie_core::geometry::LogicalRect;
        use screenie_core::layout::{Monitor, MonitorId};
        let mon = |i, x, scale| Monitor {
            id: MonitorId(i),
            name: format!("M{i}"),
            rect: LogicalRect::new(x, 0.0, 100.0, 100.0),
            scale,
        };
        let mons = [mon(0, 0.0, 1.0), mon(1, 100.0, 2.0)];
        // The frame is at 2x (the highest); a box on the 1x monitor alone.
        let img = Image::new(40, 20);
        let on_one = LogicalRect::new(10.0, 10.0, 20.0, 10.0);
        let size = |i: &Image| (i.width(), i.height());
        assert_eq!(
            size(&at_scale(img.clone(), on_one, &mons, OutputScale::Native)),
            (20, 10)
        );
        // Across both: native is 2x, min 1x.
        let across = LogicalRect::new(90.0, 10.0, 20.0, 10.0);
        assert_eq!(
            size(&at_scale(img.clone(), across, &mons, OutputScale::Native)),
            (40, 20)
        );
        assert_eq!(
            size(&at_scale(img.clone(), across, &mons, OutputScale::Min)),
            (20, 10)
        );
        assert_eq!(
            size(&at_scale(
                img.clone(),
                on_one,
                &mons,
                OutputScale::Factor(0.5)
            )),
            (10, 5)
        );
        // An opened image (no monitors): its own pixels, unless a factor.
        assert_eq!(
            size(&at_scale(img.clone(), on_one, &[], OutputScale::Native)),
            (40, 20)
        );
    }

    #[test]
    fn colours_in_each_format() {
        let c = Rgb(0x33, 0x99, 0xcc);
        assert_eq!(colour(c, ColourFormat::Hex), "#3399cc");
        assert_eq!(colour(c, ColourFormat::Rgb), "rgb(51, 153, 204)");
        assert_eq!(colour(c, ColourFormat::Hsl), "hsl(200, 60%, 50%)");
        assert_eq!(
            colour(Rgb(128, 128, 128), ColourFormat::Hsl),
            "hsl(0, 0%, 50%)"
        );
        assert_eq!(
            colour(Rgb(255, 0, 0), ColourFormat::Hsl),
            "hsl(0, 100%, 50%)"
        );
    }
}
