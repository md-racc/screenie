//! The Wayland clipboard.
//!
//! Wayland's clipboard lives only as long as the client offering it, so
//! screenie starts a small detached copy of itself to serve the selection
//! until something else is copied (as wl-copy does). The parent hands it
//! the data on stdin and waits until it has taken the selection.

use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use wl_clipboard_rs::copy::{MimeSource, MimeType, Options, Source};

/// One representation of the clipboard's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub mime: String,
    pub data: Vec<u8>,
}

/// What screenie's own shots also offer: the shot's path, so opening the
/// clipboard reopens the shot (its layers too), not just its pixels.
pub const SHOT_MIME: &str = "application/x-screenie-shot";

/// The image types read off the clipboard, in order of preference (the
/// ones screenie decodes).
const IMAGE_MIMES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/x-portable-pixmap",
];

/// An image on the clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pasted {
    /// One of screenie's shots, still in the runtime directory.
    Shot(PathBuf),
    /// Any other image, encoded.
    Image(Vec<u8>),
}

/// The image on the clipboard, if it holds one.
pub fn paste_image() -> Result<Option<Pasted>, String> {
    let Some(types) = offered()? else {
        return Ok(None);
    };
    let mut shot = None;
    let mime = choose(&types, || {
        shot = shot_path().ok().flatten();
        shot.is_some()
    });
    match mime {
        None => Ok(None),
        Some(SHOT_MIME) => Ok(shot.map(Pasted::Shot)),
        Some(mime) => Ok(Some(Pasted::Image(read(mime)?))),
    }
}

/// Whether the clipboard holds an image `paste_image` would take, without
/// transferring it.
pub fn holds_image() -> bool {
    offered()
        .ok()
        .flatten()
        .is_some_and(|types| choose(&types, || shot_path().is_ok_and(|p| p.is_some())).is_some())
}

/// The types the clipboard offers; `None` when it's empty.
fn offered() -> Result<Option<HashSet<String>>, String> {
    use wl_clipboard_rs::paste::{self, ClipboardType, Error, Seat};
    match paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(t) => Ok(Some(t)),
        Err(Error::ClipboardEmpty | Error::NoMimeType) => Ok(None),
        Err(e) => Err(format!("the clipboard: {e}")),
    }
}

/// Which type to paste: a shot, if one is offered and `shot_ok` (its file
/// is still there), else the preferred image type.
fn choose(types: &HashSet<String>, shot_ok: impl FnOnce() -> bool) -> Option<&'static str> {
    if types.contains(SHOT_MIME) && shot_ok() {
        return Some(SHOT_MIME);
    }
    IMAGE_MIMES.iter().copied().find(|m| types.contains(*m))
}

/// The offered shot's path, if its file is still there.
fn shot_path() -> Result<Option<PathBuf>, String> {
    let path = PathBuf::from(String::from_utf8_lossy(&read(SHOT_MIME)?).trim());
    Ok(path.is_file().then_some(path))
}

fn read(mime: &str) -> Result<Vec<u8>, String> {
    use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};
    let (mut pipe, _) = paste::get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(mime),
    )
    .map_err(|e| format!("the clipboard: {e}"))?;
    let mut out = Vec::new();
    pipe.read_to_end(&mut out)
        .map_err(|e| format!("the clipboard: {e}"))?;
    Ok(out)
}

/// The hidden argument that makes the executable a clipboard server.
pub const SERVE_ARG: &str = "__serve-clipboard";

fn encode(offers: &[Offer]) -> Vec<u8> {
    let mut out = Vec::new();
    for o in offers {
        out.extend_from_slice(o.mime.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(&(o.data.len() as u64).to_le_bytes());
        out.extend_from_slice(&o.data);
    }
    out
}

fn decode(mut r: impl BufRead) -> io::Result<Vec<Offer>> {
    let mut offers = Vec::new();
    loop {
        let mut mime = String::new();
        if r.read_line(&mut mime)? == 0 {
            return Ok(offers);
        }
        let mut len = [0u8; 8];
        r.read_exact(&mut len)?;
        let mut data = vec![0; u64::from_le_bytes(len) as usize];
        r.read_exact(&mut data)?;
        offers.push(Offer {
            mime: mime.trim_end_matches('\n').to_owned(),
            data,
        });
    }
}

/// Put `offers` on the clipboard, served by a detached child started from
/// `exe`. Returns once the child holds the selection.
pub fn copy(exe: &Path, offers: &[Offer]) -> io::Result<()> {
    let mut child = Command::new(exe)
        .arg(SERVE_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Outlive the parent, apart from its terminal and its directory
        // (which it would keep busy).
        .process_group(0)
        .current_dir("/")
        .spawn()?;
    {
        let mut stdin = child.stdin.take().expect("piped");
        stdin.write_all(&encode(offers))?;
    } // closed: the child has it all
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("piped")).read_line(&mut line)?;
    started(&line)
}

/// The server's first line: `ready`, or `error: why`.
fn started(line: &str) -> io::Result<()> {
    let line = line.trim();
    if line == "ready" {
        return Ok(());
    }
    let why = line
        .strip_prefix("error: ")
        .unwrap_or("the clipboard server didn't start");
    Err(io::Error::other(why.to_owned()))
}

/// The child's side: read the offers, take the selection, say so, then
/// serve until replaced.
pub fn serve() -> io::Result<()> {
    let offers = decode(BufReader::new(io::stdin().lock()))?;
    let sources = offers
        .into_iter()
        .map(|o| MimeSource {
            source: Source::Bytes(o.data.into_boxed_slice()),
            mime_type: MimeType::Specific(o.mime),
        })
        .collect();
    let mut opts = Options::new();
    opts.foreground(true);
    let mut out = io::stdout().lock();
    let prepared = match opts.prepare_copy_multi(sources) {
        Ok(p) => p,
        Err(e) => {
            writeln!(out, "error: {e}")?;
            out.flush()?;
            return Err(io::Error::other(e));
        }
    };
    writeln!(out, "ready")?;
    out.flush()?;
    drop(out);
    prepared.serve().map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_round_trip() {
        let offers = vec![
            Offer {
                mime: "image/png".into(),
                data: vec![0, 1, 2, 10, 255],
            },
            Offer {
                mime: "text/uri-list".into(),
                data: b"file:///x\n".to_vec(),
            },
        ];
        assert_eq!(decode(&encode(&offers)[..]).unwrap(), offers);
    }

    #[test]
    fn a_live_shot_wins_then_images_by_preference() {
        let types = |ms: &[&str]| ms.iter().map(|m| m.to_string()).collect::<HashSet<_>>();
        let both = types(&[SHOT_MIME, "image/png"]);
        assert_eq!(choose(&both, || true), Some(SHOT_MIME));
        assert_eq!(choose(&both, || false), Some("image/png"));
        assert_eq!(
            choose(&types(&["image/webp", "image/jpeg"]), || true),
            Some("image/jpeg")
        );
        assert_eq!(choose(&types(&["text/plain"]), || true), None);
    }

    #[test]
    fn started_reports_the_servers_error() {
        assert!(started("ready\n").is_ok());
        assert_eq!(
            started("error: no seat\n").unwrap_err().to_string(),
            "no seat"
        );
        assert_eq!(
            started("").unwrap_err().to_string(),
            "the clipboard server didn't start"
        );
    }
}
