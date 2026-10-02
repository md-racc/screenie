//! Auto-redact's analysis (docs/auto-redact.md): OCR words in, censor
//! proposals out. Proposals are temporary analysis, never project data,
//! until they're accepted as ordinary censors.
//!
//! Lines are rebuilt from the words, every character keeps a box (a word's
//! box split by character count), detectors return character spans, and a
//! span's boxes become one padded rectangle, so a secret in the middle of a
//! word (a URL's token) censors only that part.

use std::sync::LazyLock;

use regex::Regex;

use crate::geometry::PixelRect;
use crate::ocr::OcrResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Email,
    Ip,
    Mac,
    Token,
    Entropy,
    UrlSecret,
    Card,
    Iban,
    HomePath,
    Host,
    PrivateKey,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Email => "email",
            Category::Ip => "ip",
            Category::Mac => "mac",
            Category::Token => "token",
            Category::Entropy => "entropy",
            Category::UrlSecret => "url-secret",
            Category::Card => "card",
            Category::Iban => "iban",
            Category::HomePath => "home-path",
            Category::Host => "host",
            Category::PrivateKey => "private-key",
        }
    }
}

/// A suggested censor, in frame pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub rect: PixelRect,
    pub category: Category,
    /// The matched text, elided (never shown whole).
    pub label: String,
    /// Low-confidence matches are proposed unticked: shown, not applied.
    pub ticked: bool,
}

/// What some detectors need to know about this machine.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub user: String,
    pub home: String,
    pub hostname: String,
}

impl Env {
    pub fn from_system() -> Env {
        let var = |k| std::env::var(k).unwrap_or_default();
        let hostname = std::fs::read_to_string("/etc/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| var("HOSTNAME"));
        Env {
            user: var("USER"),
            home: var("HOME"),
            hostname,
        }
    }
}

/// A line of OCR text with a box for every character.
#[derive(Clone, Debug, Default)]
pub struct Line {
    pub text: String,
    pub boxes: Vec<PixelRect>,
}

/// Rebuild lines from words: words joined by spaces, each character a
/// slice of its word's box; a space spans the gap between two words.
pub fn lines(ocr: &OcrResult) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut current: Option<u32> = None;
    for w in &ocr.words {
        if current != Some(w.line) {
            out.push(Line::default());
            current = Some(w.line);
        }
        let line = out.last_mut().expect("just pushed");
        if let Some(prev) = line.boxes.last().copied() {
            let x0 = prev.x + prev.w;
            let gap = PixelRect::new(
                x0,
                w.bbox.y.min(prev.y),
                (w.bbox.x - x0).max(0),
                w.bbox.h.max(prev.h),
            );
            line.text.push(' ');
            line.boxes.push(gap);
        }
        let n = w.text.chars().count().max(1) as i32;
        for (i, c) in w.text.chars().enumerate() {
            let i = i as i32;
            let x0 = w.bbox.x + w.bbox.w * i / n;
            let x1 = w.bbox.x + w.bbox.w * (i + 1) / n;
            line.text.push(c);
            line.boxes
                .push(PixelRect::new(x0, w.bbox.y, x1 - x0, w.bbox.h));
        }
    }
    out
}

/// A match: a category, and character offsets in a line.
#[derive(Clone, Debug, PartialEq)]
struct Found {
    line: usize,
    start: usize,
    end: usize,
    category: Category,
    sure: bool,
}

fn re(s: &str) -> Regex {
    Regex::new(s).expect("a valid built-in pattern")
}

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"));
static IPV4: LazyLock<Regex> = LazyLock::new(|| re(r"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b"));
static IPV6: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7}\b"));
static MAC: LazyLock<Regex> = LazyLock::new(|| re(r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b"));
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}",
        r"|glpat-[A-Za-z0-9_-]{16,}|AKIA[0-9A-Z]{16}|xox[bapsr]-[A-Za-z0-9-]{10,}",
        r"|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,})"
    ))
});
static URL_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)[?&](?:token|key|api_?key|secret|sig|signature|code|password|pass|auth|access_token)=([^&\s]+)",
    )
});
static CARD: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d(?:[ -]?\d){12,18}\b"));
static IBAN: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]{2}\d{2}(?: ?[A-Z0-9]){11,30}\b"));
static LONG: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9+/=_-]{20,}"));

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

fn iban_ok(s: &str) -> bool {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 {
        return false;
    }
    let (head, tail) = s.split_at(4);
    let mut rem: u64 = 0;
    for c in tail.chars().chain(head.chars()) {
        let v = match c.to_digit(36) {
            Some(v) => v as u64,
            None => return false,
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem == 1
}

/// Shannon entropy, bits per character.
fn entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_insert(0u32) += 1;
    }
    let n = s.chars().count() as f64;
    counts
        .values()
        .map(|&k| {
            let p = f64::from(k) / n;
            -p * p.log2()
        })
        .sum()
}

/// Byte offsets to char offsets.
fn chars(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn detect_line(i: usize, text: &str, env: &Env) -> Vec<Found> {
    let mut found = Vec::new();
    let mut push = |start: usize, end: usize, category, sure| {
        found.push(Found {
            line: i,
            start: chars(text, start),
            end: chars(text, end),
            category,
            sure,
        });
    };
    for m in EMAIL.find_iter(text) {
        push(m.start(), m.end(), Category::Email, true);
    }
    for m in IPV4.find_iter(text) {
        let addr = m.as_str().split(':').next().unwrap_or("");
        if addr.parse::<std::net::Ipv4Addr>().is_ok() {
            push(m.start(), m.end(), Category::Ip, true);
        }
    }
    for m in IPV6.find_iter(text) {
        let s = m.as_str();
        if s.matches(':').count() >= 2
            && s.parse::<std::net::Ipv6Addr>().is_ok()
            && !MAC.is_match(s)
        {
            push(m.start(), m.end(), Category::Ip, true);
        }
    }
    for m in MAC.find_iter(text) {
        push(m.start(), m.end(), Category::Mac, true);
    }
    for m in TOKEN.find_iter(text) {
        push(m.start(), m.end(), Category::Token, true);
    }
    for c in URL_SECRET.captures_iter(text) {
        let v = c.get(1).expect("the value group");
        push(v.start(), v.end(), Category::UrlSecret, true);
    }
    for m in CARD.find_iter(text) {
        let d: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
        if (13..=19).contains(&d.len()) && luhn(&d) {
            push(m.start(), m.end(), Category::Card, true);
        }
    }
    for m in IBAN.find_iter(text) {
        if iban_ok(m.as_str()) {
            push(m.start(), m.end(), Category::Iban, true);
        }
    }
    for m in LONG.find_iter(text) {
        let s = m.as_str();
        let mixed =
            s.chars().any(|c| c.is_ascii_digit()) && s.chars().any(|c| c.is_ascii_alphabetic());
        let h = entropy(s);
        if mixed && h >= 3.5 {
            push(m.start(), m.end(), Category::Entropy, h >= 4.0);
        }
    }
    if !env.user.is_empty() {
        for prefix in [format!("/home/{}", env.user), format!("~{}", env.user)] {
            let mut from = 0;
            while let Some(at) = text[from..].find(&prefix) {
                let start = from + at;
                let end = text[start..]
                    .find(char::is_whitespace)
                    .map_or(text.len(), |e| start + e);
                push(start, end, Category::HomePath, true);
                from = end.max(start + 1);
            }
        }
    }
    if env.hostname.len() >= 3 {
        let lower = text.to_lowercase();
        let host = env.hostname.to_lowercase();
        let mut from = 0;
        while let Some(at) = lower[from..].find(&host) {
            let start = from + at;
            push(start, start + host.len(), Category::Host, true);
            from = start + host.len();
        }
    }
    found
}

/// Private keys: from a BEGIN ... PRIVATE KEY line through its END line,
/// all of it as one block (an unterminated one runs to the last line).
fn private_keys(lines: &[Line]) -> Vec<PixelRect> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        if open.is_none() && l.text.contains("BEGIN") && l.text.contains("PRIVATE KEY") {
            open = Some(i);
            continue;
        }
        if let Some(s) = open {
            if l.text.contains("END") && l.text.contains("PRIVATE KEY") {
                let boxes: Vec<PixelRect> =
                    lines[s..=i].iter().flat_map(|l| l.boxes.clone()).collect();
                out.extend(union(&boxes));
                open = None;
            }
        }
    }
    if let Some(s) = open {
        let boxes: Vec<PixelRect> = lines[s..].iter().flat_map(|l| l.boxes.clone()).collect();
        out.extend(union(&boxes));
    }
    out
}

fn union(boxes: &[PixelRect]) -> Option<PixelRect> {
    let mut it = boxes.iter().filter(|b| b.w > 0 && b.h > 0);
    let f = *it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (f.x, f.y, f.x + f.w, f.y + f.h);
    for b in it {
        x0 = x0.min(b.x);
        y0 = y0.min(b.y);
        x1 = x1.max(b.x + b.w);
        y1 = y1.max(b.y + b.h);
    }
    Some(PixelRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn overlaps(a: &PixelRect, b: &PixelRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// Elide a match for the list: its first few characters, then "…".
fn elide(s: &str) -> String {
    let head: String = s.chars().take(4).collect();
    format!("{head}…")
}

/// Proposed censors for an OCR result: padded by `pad` of the line height,
/// overlapping ones merged (a sure match wins the tick and the label).
/// Overlap is of the matched text, before padding, so matches on lines
/// next to each other stay apart.
pub fn propose(ocr: &OcrResult, env: &Env, pad: f64) -> Vec<Proposal> {
    let lines = lines(ocr);
    let mut found: Vec<Found> = lines
        .iter()
        .enumerate()
        .flat_map(|(i, l)| detect_line(i, &l.text, env))
        .collect();
    // The surer, longer match first, so merges keep its label.
    found.sort_by_key(|f| {
        (
            std::cmp::Reverse(f.sure),
            std::cmp::Reverse(f.end - f.start),
        )
    });
    // Each proposal, with the unpadded box it merges by.
    let mut out: Vec<(Proposal, PixelRect)> = private_keys(&lines)
        .into_iter()
        .map(|r| {
            let p = Proposal {
                rect: r,
                category: Category::PrivateKey,
                label: "-----BEGIN…".into(),
                ticked: true,
            };
            (p, r)
        })
        .collect();
    for f in found {
        let l = &lines[f.line];
        let Some(r) = union(&l.boxes[f.start..f.end.min(l.boxes.len())]) else {
            continue;
        };
        let lh = l.boxes.iter().map(|b| b.h).max().unwrap_or(r.h);
        let p = (f64::from(lh) * pad).ceil() as i32;
        let rect = PixelRect::new(r.x - p, r.y - p, r.w + 2 * p, r.h + 2 * p);
        let text: String = l.text.chars().skip(f.start).take(f.end - f.start).collect();
        if let Some((prev, core)) = out.iter_mut().find(|(_, c)| overlaps(c, &r)) {
            prev.rect = union(&[prev.rect, rect]).expect("two boxes");
            *core = union(&[*core, r]).expect("two boxes");
            prev.ticked |= f.sure;
            continue;
        }
        let p = Proposal {
            rect,
            category: f.category,
            label: elide(&text),
            ticked: f.sure,
        };
        out.push((p, r));
    }
    // Merging can make boxes meet that didn't: settle that too.
    let mut merged: Vec<(Proposal, PixelRect)> = Vec::new();
    for (p, c) in out {
        if let Some((prev, core)) = merged.iter_mut().find(|(_, k)| overlaps(k, &c)) {
            prev.rect = union(&[prev.rect, p.rect]).expect("two boxes");
            *core = union(&[*core, c]).expect("two boxes");
            prev.ticked |= p.ticked;
        } else {
            merged.push((p, c));
        }
    }
    merged.into_iter().map(|(p, _)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::OcrWord;

    /// One word per string, 10px a character, on lines 20px apart.
    fn ocr(lines: &[&[&str]]) -> OcrResult {
        let mut words = vec![];
        for (li, line) in lines.iter().enumerate() {
            let mut x = 0;
            for w in line.iter() {
                let n = w.chars().count() as i32;
                words.push(OcrWord {
                    text: w.to_string(),
                    bbox: PixelRect::new(x, li as i32 * 20, n * 10, 12),
                    confidence: Some(90.0),
                    line: li as u32,
                });
                x += n * 10 + 10;
            }
        }
        OcrResult {
            text: String::new(),
            words,
        }
    }

    fn env() -> Env {
        Env {
            user: "adi".into(),
            home: "/home/adi".into(),
            hostname: "myhost".into(),
        }
    }

    fn cats(lines: &[&[&str]]) -> Vec<(&'static str, bool)> {
        propose(&ocr(lines), &env(), 0.0)
            .iter()
            .map(|p| (p.category.name(), p.ticked))
            .collect()
    }

    #[test]
    fn character_boxes_split_words_and_span_gaps() {
        let l = &lines(&ocr(&[&["ab", "cd"]]))[0];
        assert_eq!(l.text, "ab cd");
        assert_eq!(l.boxes[1], PixelRect::new(10, 0, 10, 12));
        assert_eq!(l.boxes[2], PixelRect::new(20, 0, 10, 12)); // the gap
        assert_eq!(l.boxes[4], PixelRect::new(40, 0, 10, 12));
    }

    #[test]
    fn detectors() {
        assert_eq!(
            cats(&[&["mail", "adi@example.org", "now"]]),
            [("email", true)]
        );
        assert_eq!(cats(&[&["host", "10.0.0.12:8080"]]), [("ip", true)]);
        assert_eq!(cats(&[&["999.1.1.1"]]), []); // not an address
        assert_eq!(cats(&[&["fe80::1ff:fe23:4567:890a"]]), [("ip", true)]);
        assert_eq!(cats(&[&["aa:bb:cc:dd:ee:ff"]]), [("mac", true)]);
        assert_eq!(
            cats(&[&["ghp_0123456789abcdefghijABCDEFGHIJ"]]),
            [("token", true)]
        );
        assert_eq!(cats(&[&["4111", "1111", "1111", "1111"]]), [("card", true)]);
        assert_eq!(cats(&[&["4111", "1111", "1111", "1112"]]), []); // fails Luhn
        assert_eq!(cats(&[&["GB82WEST12345698765432"]]), [("iban", true)]);
        assert_eq!(cats(&[&["/home/adi/notes.txt"]]), [("home-path", true)]);
        assert_eq!(cats(&[&["ssh", "MYHOST"]]), [("host", true)]);
        assert_eq!(cats(&[&["just", "some", "ordinary", "words"]]), []);
    }

    #[test]
    fn a_url_secret_censors_only_its_value() {
        let p = propose(
            &ocr(&[&["https://x.io/a?id=7&token=abc123def"]]),
            &env(),
            0.0,
        );
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].category, Category::UrlSecret);
        // "https://x.io/a?id=7&token=" is 26 characters: the value starts there.
        assert_eq!(p[0].rect.x, 260);
        assert_eq!(p[0].label, "abc1…");
    }

    #[test]
    fn random_looking_strings_are_proposed_and_the_less_random_unticked() {
        assert_eq!(cats(&[&["k9Xq2LmP7vR4tZ8wB1nC5yH3"]]), [("entropy", true)]);
        assert_eq!(cats(&[&["aaaabbbb1111ccccdddd2222"]]), []);
    }

    #[test]
    fn a_private_key_block_is_proposed_whole() {
        let p = propose(
            &ocr(&[
                &["-----BEGIN", "OPENSSH", "PRIVATE", "KEY-----"],
                &["b3BlbnNzaC1rZXktdjEAAAAA"],
                &["-----END", "OPENSSH", "PRIVATE", "KEY-----"],
                &["after"],
            ]),
            &env(),
            0.0,
        );
        assert_eq!(p.len(), 1, "{p:?}");
        assert_eq!(p[0].rect.y, 0);
        assert_eq!(p[0].rect.y + p[0].rect.h, 52); // three lines, not the fourth
    }

    #[test]
    fn matches_on_neighbouring_lines_stay_apart_however_padded() {
        // Lines 20px apart, 12px high: padded by 6, their boxes would meet.
        let p = propose(
            &ocr(&[&["/home/adi/a.txt"], &["/home/adi/b.txt"]]),
            &env(),
            0.5,
        );
        assert_eq!(p.len(), 2, "{p:?}");
    }

    #[test]
    fn padding_grows_by_the_line_height() {
        let p = propose(&ocr(&[&["adi@example.org"]]), &env(), 0.2);
        assert_eq!(p[0].rect, PixelRect::new(-3, -3, 156, 18));
    }
}
