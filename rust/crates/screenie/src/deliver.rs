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
    /// The project written: the `--save-project` one, else the runtime
    /// copy's.
    pub project: Option<PathBuf>,
    pub copied: bool,
}

pub fn notifies(out: &Output) -> bool {
    out.notify
        .unwrap_or_else(|| !std::io::stdout().is_terminal())
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("can't find screenie itself: {e}"))
}

/// Write a new file in `dir`; with `project`, at a name its project is
/// free to have as well.
fn write_file(
    dir: &std::path::Path,
    stem: &str,
    ext: &str,
    mode: u32,
    bytes: &[u8],
    project: bool,
) -> Result<PathBuf, String> {
    let beside = project.then_some("screenie");
    let (path, mut f) = files::create_unique(dir, stem, ext, mode, beside)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A project to keep beside the image, encoded only if it's needed.
pub type ProjectBytes<'a> = &'a dyn Fn() -> Result<Vec<u8>, String>;

/// Write a shot's project beside it: private, never overwriting.
fn write_project(beside: &std::path::Path, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = files::project_of(beside);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Write an image's files: the history copy (PNG) and its project in
/// `runtime`, unless there's none (`--no-history`), and the `--save` copy
/// (`bytes`, in `--format`) with its project if `--save-project`.
fn keep(
    png: &[u8],
    bytes: &[u8],
    out: &Output,
    project: Option<ProjectBytes>,
    naming: &files::Naming,
    runtime: Option<&std::path::Path>,
    d: &mut Delivered,
) -> Result<(), String> {
    let ext = out.format.ext();
    let mut project_bytes: Option<Vec<u8>> = None;
    let mut project_once = |p: ProjectBytes| -> Result<Vec<u8>, String> {
        if project_bytes.is_none() {
            project_bytes = Some(p()?);
        }
        Ok(project_bytes.clone().expect("just made"))
    };
    if let Some(runtime) = runtime {
        let shots = runtime.join("shots");
        let path = write_file(
            &shots,
            &files::stamp(),
            "png",
            0o600,
            png,
            project.is_some(),
        )?;
        if let Some(p) = project {
            d.project = Some(write_project(&path, &project_once(p)?)?);
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
        let with_project = out.save_project && project.is_some();
        let path = write_file(&dir, &stem, ext, 0o644, bytes, with_project)?;
        // Absolute, for the uri-list, the report and the notification.
        let path = std::path::absolute(&path).unwrap_or(path);
        if let (true, Some(p)) = (out.save_project, project) {
            d.project = Some(write_project(&path, &project_once(p)?)?);
        }
        d.saved = Some(path);
    }
    Ok(())
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
    let mut d = Delivered::default();
    let runtime = if out.no_history {
        None
    } else {
        Some(files::runtime_dir().map_err(|e| format!("runtime dir: {e}"))?)
    };
    keep(
        &png,
        &bytes,
        out,
        project,
        naming,
        runtime.as_deref(),
        &mut d,
    )?;
    if out.stdout {
        to_stdout(&mut std::io::stdout().lock(), &bytes)?;
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
                data: format!("{}\r\n", file_uri(p)).into_bytes(),
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

/// An absolute path as a `file://` URI (RFC 8089), its bytes
/// percent-encoded but for the unreserved ones and `/`.
fn file_uri(p: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in p.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            uri.push(char::from(b));
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

/// Write a result to stdout, flushed, so a reader that's gone (`| true`)
/// is a delivery failure and not a panic or a lost tail.
fn to_stdout(w: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    w.write_all(bytes)
        .and_then(|()| w.flush())
        .map_err(|e| format!("stdout: {e}"))
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
    let on = monitors
        .iter()
        .filter(|m| m.rect.intersects(&rect))
        .map(|m| m.scale);
    // Off-screen (an opened image): its own pixels are native.
    let own = f64::from(img.width()) / rect.w;
    let k = match scale {
        OutputScale::Native => on.reduce(f64::max),
        OutputScale::Min => on.reduce(f64::min),
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
        to_stdout(
            &mut std::io::stdout().lock(),
            format!("{text}\n").as_bytes(),
        )?;
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
    fn a_closed_stdout_is_an_error_not_a_panic() {
        struct Gone(bool);
        impl Write for Gone {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self.0 {
                    Err(std::io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(b.len())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        for at_write in [true, false] {
            let e = to_stdout(&mut Gone(at_write), b"x\n").unwrap_err();
            assert!(e.starts_with("stdout:"), "{e}");
        }
        let mut v = Vec::new();
        to_stdout(&mut v, b"#3399cc\n").unwrap();
        assert_eq!(v, b"#3399cc\n");
    }

    #[test]
    fn a_saved_files_uri_is_percent_encoded() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;
        assert_eq!(
            file_uri(Path::new("/home/u/s/Issue #12 · repo.png")),
            "file:///home/u/s/Issue%20%2312%20%C2%B7%20repo.png"
        );
        assert_eq!(
            file_uri(Path::new("/a/100%?.png")),
            "file:///a/100%25%3F.png"
        );
        assert_eq!(
            file_uri(Path::new(OsStr::from_bytes(b"/a/\xff.png"))),
            "file:///a/%FF.png"
        );
    }

    /// What `keep` writes, in a fresh directory.
    #[test]
    fn an_images_files_are_where_cli_md_says() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("screenie-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (rt, saves) = (root.join("rt"), root.join("saves"));
        let img = Image::filled(2, 2, Rgb(1, 2, 3));
        let png = encode::png(&img, 1).unwrap();
        let project = || Ok(b"project".to_vec());
        let naming = files::Naming::default();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let names = |dir: &std::path::Path| {
            let mut v: Vec<String> = std::fs::read_dir(dir)
                .map(|d| {
                    d.map(|e| e.unwrap().file_name().into_string().unwrap())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };
        let kept = |out: &Output, project: Option<ProjectBytes>, runtime| {
            let mut d = Delivered::default();
            keep(&png, &png, out, project, &naming, runtime, &mut d).map(|()| d)
        };

        // History: a private copy and project, and `last` naming the copy.
        let d = kept(&Output::default(), Some(&project), Some(&rt)).unwrap();
        let shot = d.shot.unwrap();
        let shot_project = d.project.unwrap();
        assert_eq!(shot_project, files::project_of(&shot));
        assert_eq!((mode(&shot) & 0o077, mode(&shot_project) & 0o077), (0, 0));
        assert_eq!(
            std::fs::read_to_string(rt.join("last")).unwrap(),
            format!("{}\n", shot.display())
        );

        // --save DIR: the image, and no project without --save-project;
        // --no-history (no runtime): nothing there.
        let save_in = |at: PathBuf, format, save_project| Output {
            save: Some(Some(at)),
            format,
            save_project,
            ..Output::default()
        };
        let before = names(&rt.join("shots"));
        let d = kept(
            &save_in(saves.clone(), Format::Png, false),
            Some(&project),
            None,
        )
        .unwrap();
        assert_eq!((d.shot, d.project), (None, None));
        assert_eq!(names(&saves).len(), 1);
        assert!(names(&saves)[0].ends_with(".png"));
        assert_eq!(names(&rt.join("shots")), before);

        // A file name: the format's extension wins, and a taken one gets -1.
        let named = save_in(saves.join("out.png"), Format::Jpeg, false);
        let a = kept(&named, None, None).unwrap().saved.unwrap();
        let b = kept(&named, None, None).unwrap().saved.unwrap();
        assert_eq!((a, b), (saves.join("out.jpg"), saves.join("out-1.jpg")));

        // --save-project: beside the image, never over another project.
        std::fs::write(saves.join("p.screenie"), b"older").unwrap();
        let d = kept(
            &save_in(saves.join("p.png"), Format::Png, true),
            Some(&project),
            None,
        )
        .unwrap();
        assert_eq!(d.saved, Some(saves.join("p-1.png")));
        assert_eq!(d.project, Some(saves.join("p-1.screenie")));
        assert_eq!(std::fs::read(saves.join("p.screenie")).unwrap(), b"older");
        assert_eq!(mode(&saves.join("p-1.screenie")) & 0o077, 0);
        std::fs::remove_dir_all(&root).unwrap();
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
