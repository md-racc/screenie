//! Finding a monospace font for the tags.

use std::path::PathBuf;
use std::process::Command;

/// Common monospace fonts, tried before asking fontconfig.
const CANDIDATES: &[&str] = &[
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
];

/// The bytes of a monospace font: a known path, else fontconfig's pick.
pub fn monospace() -> Option<Vec<u8>> {
    let known = CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists());
    let path = known.or_else(|| {
        let out = Command::new("fc-match")
            .args(["-f", "%{file}", "monospace"])
            .output()
            .ok()?;
        let p = PathBuf::from(String::from_utf8(out.stdout).ok()?);
        p.exists().then_some(p)
    })?;
    std::fs::read(path).ok()
}
