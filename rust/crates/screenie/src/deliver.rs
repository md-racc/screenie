//! Delivering a result: the runtime copy and `last`, `--save`, stdout,
//! the clipboard, and a notification.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use screenie_image::{Image, encode};
use screenie_platform::clipboard::{self, Offer};
use screenie_platform::notify::notify;

use crate::cli::Output;
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
    ext: &str,
    mode: u32,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let (path, mut f) = files::create_unique(dir, &files::stamp(), ext, mode)
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
) -> Result<Delivered, String> {
    let png = encode::png(img, out.png_level).map_err(|e| format!("encoding: {e}"))?;
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
        let path = write_file(&shots, "png", 0o600, &png)?;
        if let Some(p) = project {
            write_project(&path, &project_once(p)?)?;
        }
        files::prune(&shots, files::KEEP);
        let _ = std::fs::write(runtime.join("last"), format!("{}\n", path.display()));
        d.shot = Some(path);
    }
    if let Some(dir) = &out.save {
        let dir = files::expand(dir.as_deref().unwrap_or(&files::pictures_dir()));
        let path = write_file(&dir, "png", 0o644, &png)?;
        if let (true, Some(p)) = (out.save_project, project) {
            write_project(&path, &project_once(p)?)?;
        }
        d.saved = Some(path);
    }
    if out.stdout {
        std::io::stdout()
            .lock()
            .write_all(&png)
            .map_err(|e| format!("stdout: {e}"))?;
    }
    if out.copies() {
        let mut offers = vec![Offer {
            mime: "image/png".into(),
            data: png,
        }];
        if let Some(p) = &d.saved {
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
