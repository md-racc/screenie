//! OCR with tesseract, reading its TSV output so words keep their boxes
//! and confidences.

use std::io::Write;
use std::process::{Command, Stdio};

use screenie_core::geometry::PixelRect;
use screenie_core::ocr::{OcrResult, OcrWord};

/// Run tesseract on a PNG. `map` turns the PNG's pixel boxes back into
/// frame pixels (the crop's origin and any upscaling).
pub fn recognize(
    png: &[u8],
    lang: &str,
    map: impl Fn(PixelRect) -> PixelRect,
) -> Result<OcrResult, String> {
    recognize_with("tesseract", png, lang, map)
}

fn recognize_with(
    bin: &str,
    png: &[u8],
    lang: &str,
    map: impl Fn(PixelRect) -> PixelRect,
) -> Result<OcrResult, String> {
    let mut child = Command::new(bin)
        .args(["stdin", "stdout", "-l", lang, "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("can't run {bin} (is tesseract installed?): {e}"))?;
    // Tesseract loads its language before it reads the image, and exits
    // at once if that fails; its own message beats the broken pipe.
    let sent = child.stdin.take().expect("piped").write_all(png);
    let out = child
        .wait_with_output()
        .map_err(|e| format!("reading {bin}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "{bin} failed: {}",
            err.trim().chars().take(200).collect::<String>()
        ));
    }
    sent.map_err(|e| format!("sending the image to {bin}: {e}"))?;
    Ok(parse_tsv(&String::from_utf8_lossy(&out.stdout), map))
}

/// Tesseract's TSV: one row per item; words are level 5. Lines join with
/// newlines and paragraphs with a blank line, as its plain text does.
pub fn parse_tsv(tsv: &str, map: impl Fn(PixelRect) -> PixelRect) -> OcrResult {
    let mut words = Vec::new();
    let mut text = String::new();
    let mut last: Option<(u32, u32, u32)> = None;
    let mut line_no: u32 = 0;
    for row in tsv.lines().skip(1) {
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() < 12 || f[0] != "5" {
            continue;
        }
        let n = |i: usize| f[i].trim().parse::<i64>().unwrap_or(0);
        let word = f[11].trim();
        if word.is_empty() {
            continue;
        }
        let (block, par, line) = (n(2) as u32, n(3) as u32, n(4) as u32);
        match last {
            None => {}
            Some((b, p, _)) if (b, p) != (block, par) => {
                text.push_str("\n\n");
                line_no += 1;
            }
            Some((_, _, l)) if l != line => {
                text.push('\n');
                line_no += 1;
            }
            Some(_) => text.push(' '),
        }
        last = Some((block, par, line));
        text.push_str(word);
        let bbox = map(PixelRect::new(
            n(6) as i32,
            n(7) as i32,
            n(8) as i32,
            n(9) as i32,
        ));
        let confidence = f[10].trim().parse::<f32>().ok().filter(|c| *c >= 0.0);
        words.push(OcrWord {
            text: word.to_string(),
            bbox,
            confidence,
            line: line_no,
        });
    }
    OcrResult {
        text,
        words,
        codes: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t400\t100\t-1\t
5\t1\t1\t1\t1\t1\t10\t20\t30\t12\t96.5\tThe
5\t1\t1\t1\t1\t2\t44\t20\t40\t12\t91.0\tquick
5\t1\t1\t1\t2\t1\t10\t40\t30\t12\t90.0\tbrown
5\t1\t2\t1\t1\t1\t10\t80\t30\t12\t88.0\tfox
";

    #[test]
    fn words_lines_and_paragraphs() {
        let r = parse_tsv(TSV, |b| {
            PixelRect::new(b.x / 2 + 100, b.y / 2 + 50, b.w / 2, b.h / 2)
        });
        assert_eq!(r.text, "The quick\nbrown\n\nfox");
        assert_eq!(r.words.len(), 4);
        assert_eq!(r.words[1].bbox, PixelRect::new(122, 60, 20, 6));
        assert_eq!(r.words[0].confidence, Some(96.5));
        assert_eq!(
            r.words.iter().map(|w| w.line).collect::<Vec<_>>(),
            [0, 0, 1, 2]
        );
    }

    #[test]
    fn no_words_no_text() {
        assert!(parse_tsv("level\n1\t1\t0\t0\t0\t0\t0\t0\t9\t9\t-1\t\n", |b| b).is_empty());
    }

    #[test]
    fn a_missing_engine_is_an_error() {
        assert!(recognize_with("screenie-test-no-such-binary", b"", "eng", |b| b).is_err());
    }

    #[test]
    fn an_engine_that_exits_early_reports_its_failure_not_the_pipe() {
        // `false` exits 1 without reading; 1 MiB overflows the pipe.
        let err = recognize_with("false", &vec![0u8; 1 << 20], "eng", |b| b).unwrap_err();
        assert!(err.starts_with("false failed"), "{err}");
    }
}
