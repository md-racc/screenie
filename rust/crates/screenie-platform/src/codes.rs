//! QR codes and barcodes, with zbar's zbarimg.

use std::io::Write;
use std::path::Path;
use std::process::Command;

/// The code in a PNG, if any. `scratch` is a private directory for the
/// temporary file zbarimg reads.
pub fn decode(png: &[u8], scratch: &Path) -> Result<Option<String>, String> {
    decode_with("zbarimg", png, scratch)
}

fn decode_with(bin: &str, png: &[u8], scratch: &Path) -> Result<Option<String>, String> {
    let path = scratch.join(format!("code-{}.png", std::process::id()));
    let mut f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(png)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    drop(f);
    let out = Command::new(bin).args(["-q", "--raw"]).arg(&path).output();
    let _ = std::fs::remove_file(&path);
    let out = out.map_err(|_| "zbarimg (zbar) is not installed".to_string())?;
    // zbarimg exits 4 when it found nothing.
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.code() {
        Some(0) if !text.is_empty() => Ok(Some(text)),
        Some(0 | 4) => Ok(None),
        _ => Err(format!(
            "zbarimg failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_png() -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 40, 40);
        e.set_color(png::ColorType::Grayscale);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[255; 1600]).unwrap();
        w.finish().unwrap();
        out
    }

    #[test]
    fn nothing_in_a_blank_image() {
        if Command::new("zbarimg").arg("--version").output().is_err() {
            return;
        }
        assert_eq!(decode(&blank_png(), &std::env::temp_dir()), Ok(None));
    }

    #[test]
    fn a_missing_zbar_is_an_error() {
        let err = decode_with(
            "screenie-no-such-zbarimg",
            &blank_png(),
            &std::env::temp_dir(),
        );
        assert!(err.unwrap_err().contains("not installed"));
    }
}
