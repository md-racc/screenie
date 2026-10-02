//! The Wayland clipboard.
//!
//! Wayland's clipboard lives only as long as the client offering it, so
//! screenie starts a small detached copy of itself to serve the selection
//! until something else is copied (as wl-copy does). The parent hands it
//! the data on stdin and waits until it has taken the selection.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use wl_clipboard_rs::copy::{MimeSource, MimeType, Options, Source};

/// One representation of the clipboard's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub mime: String,
    pub data: Vec<u8>,
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
        .process_group(0) // outlive the parent, apart from its terminal
        .spawn()?;
    {
        let mut stdin = child.stdin.take().expect("piped");
        stdin.write_all(&encode(offers))?;
    } // closed: the child has it all
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("piped")).read_line(&mut line)?;
    if line.trim() == "ready" {
        Ok(())
    } else {
        Err(io::Error::other("the clipboard server didn't start"))
    }
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
    let prepared = opts.prepare_copy_multi(sources).map_err(io::Error::other)?;
    let mut out = io::stdout().lock();
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
}
