//! QR codes and barcodes, with zbar's zbarimg.

use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use screenie_core::geometry::PixelRect;

/// The code in a PNG, if any. `scratch` is a private directory for the
/// temporary file zbarimg reads.
pub fn decode(png: &[u8], scratch: &Path) -> Result<Option<String>, String> {
    decode_with("zbarimg", png, scratch)
}

fn decode_with(bin: &str, png: &[u8], scratch: &Path) -> Result<Option<String>, String> {
    let text = run(bin, &["-q", "--raw"], png, scratch)?;
    Ok(text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()))
}

/// A code and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Code {
    pub text: String,
    /// Its outline's bounds, in the PNG's pixels.
    pub bbox: PixelRect,
}

/// Every code in a PNG, with its outline.
pub fn find(png: &[u8], scratch: &Path) -> Result<Vec<Code>, String> {
    Ok(run("zbarimg", &["-q", "--xml"], png, scratch)?
        .map(|xml| parse_xml(&xml))
        .unwrap_or_default())
}

/// zbarimg's output, or `None` when it found nothing.
fn run(bin: &str, args: &[&str], png: &[u8], scratch: &Path) -> Result<Option<String>, String> {
    // Unique a call: scans on threads may overlap.
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let path = scratch.join(format!("code-{}-{n}.png", std::process::id()));
    let mut f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(png)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    drop(f);
    let out = Command::new(bin).args(args).arg(&path).output();
    let _ = std::fs::remove_file(&path);
    let out = out.map_err(|_| "zbarimg (zbar) is not installed".to_string())?;
    // zbarimg exits 4 when it found nothing.
    match out.status.code() {
        Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned())),
        Some(4) => Ok(None),
        _ => Err(format!(
            "zbarimg failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// `--xml`'s symbols: each one's polygon (bounded) and data.
fn parse_xml(xml: &str) -> Vec<Code> {
    xml.split("<symbol ")
        .skip(1)
        .filter_map(|sym| {
            let points = sym.split("points='").nth(1)?.split('\'').next()?;
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for p in points.split_whitespace() {
                let (x, y) = p.split_once(',')?;
                let (x, y): (i32, i32) = (x.parse().ok()?, y.parse().ok()?);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            if x0 > x1 {
                return None;
            }
            let data = sym.split("<![CDATA[").nth(1)?.split("]]>").next()?;
            Some(Code {
                text: data.to_string(),
                bbox: PixelRect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1),
            })
        })
        .collect()
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
    fn xml_symbols_become_bounded_codes() {
        let xml = "<barcodes><source><index num='0'>\
            <symbol type='QR-Code' quality='1'><polygon points='+10,+20 +10,+60 +50,+60 +50,+20'/>\
            <data><![CDATA[https://x.io/?token=abc]]></data></symbol>\
            <symbol type='EAN-13' quality='9'><polygon points='+100,+5 +140,+7'/>\
            <data><![CDATA[4006381333931]]></data></symbol></index></source></barcodes>";
        let codes = parse_xml(xml);
        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].text, "https://x.io/?token=abc");
        assert_eq!(codes[0].bbox, PixelRect::new(10, 20, 41, 41));
        assert_eq!(codes[1].bbox, PixelRect::new(100, 5, 41, 3));
        assert!(parse_xml("<barcodes/>").is_empty());
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
