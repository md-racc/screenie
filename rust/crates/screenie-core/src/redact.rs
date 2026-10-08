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
    /// A card's CVV/CVC/CID: 3-4 digits after a keyword, or near a card.
    SecurityCode,
    /// A card's expiry date: `MM/YY` after a keyword, or near a card.
    CardExpiry,
    Iban,
    HomePath,
    Host,
    PrivateKey,
    /// Off by default: too many false positives.
    Phone,
    /// Off by default.
    Uuid,
    /// A string from `[redact] literal`.
    Literal,
    /// A `[[redact.custom]]` regex, by its name.
    Custom(&'static str),
    /// A QR code or barcode, whatever it holds (`codes` in the config).
    Code,
}

/// The built-in detectors, as `[redact] detectors` names them.
pub const DETECTORS: &[Category] = &[
    Category::Email,
    Category::Ip,
    Category::Mac,
    Category::Token,
    Category::Entropy,
    Category::UrlSecret,
    Category::Card,
    Category::SecurityCode,
    Category::CardExpiry,
    Category::Iban,
    Category::HomePath,
    Category::Host,
    Category::PrivateKey,
    Category::Phone,
    Category::Uuid,
];

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
            Category::SecurityCode => "security-code",
            Category::CardExpiry => "card-expiry",
            Category::Iban => "iban",
            Category::HomePath => "home-path",
            Category::Host => "host",
            Category::PrivateKey => "private-key",
            Category::Phone => "phone",
            Category::Uuid => "uuid",
            Category::Literal => "literal",
            Category::Custom(name) => name,
            Category::Code => "code",
        }
    }

    /// A built-in detector by its name.
    pub fn from_name(name: &str) -> Option<Category> {
        DETECTORS.iter().copied().find(|c| c.name() == name)
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

/// A `[[redact.custom]]` detector.
#[derive(Clone, Debug)]
pub struct Custom {
    pub name: &'static str,
    pub regex: Regex,
}

impl Custom {
    /// The name lives for the run (custom detectors are made once, from
    /// the config), so it can be a category like the built-in ones.
    pub fn new(name: &str, pattern: &str) -> Result<Custom, String> {
        let regex = Regex::new(pattern).map_err(|e| format!("{name}: {e}"))?;
        Ok(Custom {
            name: Box::leak(name.to_owned().into_boxed_str()),
            regex,
        })
    }
}

/// What to look for, and how to censor it: `[redact]` config over the
/// defaults.
#[derive(Clone, Debug)]
pub struct Rules {
    pub env: Env,
    /// The built-in detectors that run.
    pub detectors: Vec<Category>,
    /// Matched case-insensitively, tolerant of OCR's confusions.
    pub literal: Vec<String>,
    /// Matched with their subdomains, as `host`.
    pub domains: Vec<String>,
    pub custom: Vec<Custom>,
    /// Whether QR codes and barcodes are proposed (`codes`).
    pub codes: bool,
    /// Of the line height, round each match.
    pub pad: f64,
    /// What accepted suggestions become.
    pub kind: crate::model::CensorKind,
    /// Scan every monitor when the picker opens (the host does it).
    pub prescan: bool,
}

/// A pixelate suggestion's block size, logical px (Censor's default).
pub const PIXELATE: f64 = 10.0;

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            env: Env::default(),
            detectors: DETECTORS
                .iter()
                .copied()
                .filter(|c| !matches!(c, Category::Phone | Category::Uuid))
                .collect(),
            literal: vec![],
            domains: vec![],
            custom: vec![],
            codes: true,
            pad: 0.2,
            kind: crate::model::CensorKind::Bar,
            prescan: false,
        }
    }
}

impl Rules {
    fn on(&self, c: Category) -> bool {
        self.detectors.contains(&c)
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

/// OCR reads `@` as `©` or `®`, puts spaces round it, and reads a dot as
/// a comma: such matches are proposed, unticked.
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"[A-Za-z0-9._%+-]+ ?[@©®] ?[A-Za-z0-9-]+(?:[.,][A-Za-z0-9-]+)*[.,][A-Za-z]{2,}")
});
static PHONE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:\+\d{1,3}[ .-]?)?(?:\(\d{1,4}\)[ .-]?)?\d{2,4}(?:[ .-]?\d{2,4}){1,4}")
});
static ISO_DATE: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{4}-\d{2}-\d{2}$"));
static UUID: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\b[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\b")
});
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
/// What a security code is labelled.
/// OCR often runs the label into the code (`cvv:123`, `CVV123`).
static CODE_WORD: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:cvv2?|cvc2?|cid|csc|security\s+code|card\s+code)(?:\b|[0-9])"));
static DIGITS: LazyLock<Regex> = LazyLock::new(|| re(r"[0-9]+"));

/// 3 or 4 digits on their own, as char offsets: not part of a longer
/// number, a date (`12/27`), a decimal (`1.234`) or a time (`123:00`); a
/// label before them (`cvv:123`, `#123`) is fine.
fn isolated_codes(text: &str) -> Vec<(usize, usize)> {
    let cs: Vec<char> = text.chars().collect();
    DIGITS
        .find_iter(text)
        .filter_map(|m| {
            let (start, end) = (chars(text, m.start()), chars(text, m.end()));
            let before = start.checked_sub(1).map(|i| cs[i]);
            let after = cs.get(end).copied();
            let after2 = cs.get(end + 1).copied();
            let ok = (3..=4).contains(&(end - start))
                && !matches!(before, Some('/' | '.' | ',' | '-'))
                && !matches!(after, Some('/' | ',' | ':' | '-'))
                // a decimal point, not a full stop
                && !(after == Some('.') && after2.is_some_and(|c| c.is_ascii_digit()));
            ok.then_some((start, end))
        })
        .collect()
}
/// A code this many line heights from a card, or less, is the card's.
const CODE_NEAR: i32 = 4;
/// A keyword this many characters before a code, or less, labels it.
const CODE_AFTER: usize = 6;

/// The cards found, each with its line's height.
fn cards(lines: &[Line], found: &[Found]) -> Vec<(PixelRect, i32)> {
    found
        .iter()
        .filter(|f| f.category == Category::Card)
        .filter_map(|f| {
            let l = &lines[f.line];
            let r = union(&l.boxes[f.start..f.end.min(l.boxes.len())])?;
            Some((r, l.boxes.iter().map(|b| b.h).max().unwrap_or(r.h)))
        })
        .collect()
}

/// Whether `r` is within `CODE_NEAR` line heights of one of `cards` (and
/// not far off to the side).
fn near_card(r: PixelRect, cards: &[(PixelRect, i32)]) -> bool {
    cards.iter().any(|&(card, lh)| {
        let gap_y = (r.y - (card.y + card.h)).max(card.y - (r.y + r.h)).max(0);
        let gap_x = (r.x - (card.x + card.w)).max(card.x - (r.x + r.w)).max(0);
        gap_y <= CODE_NEAR * lh && gap_x <= card.w.max(10 * lh)
    })
}

/// `MM/YY` or `MM/YYYY`, a real month, not part of a longer date or number.
static EXPIRY: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:^|[^0-9/])((?:0?[1-9]|1[0-2]) ?/ ?(?:[0-9]{4}|[0-9]{2}))(?:$|[^0-9/])")
});
/// What an expiry date is labelled. OCR runs labels into dates
/// (`Thru:12/30`, `EXP12/27`).
static EXPIRY_WORD: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:exp(?:iry|ires|iration|\.)?|valid\s*(?:thru|through|until)|good\s*thru)")
});

/// Card expiry dates, which only mean something next to a card or a label
/// (dates are everywhere): `MM/YY` after a keyword on its line, or within
/// `CODE_NEAR` line heights of a card that was found.
fn card_expiries(lines: &[Line], found: &[Found]) -> Vec<Found> {
    let cards = cards(lines, found);
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let words: Vec<usize> = EXPIRY_WORD
            .find_iter(&l.text)
            .map(|m| chars(&l.text, m.end()))
            .collect();
        for m in EXPIRY.captures_iter(&l.text).filter_map(|c| c.get(1)) {
            let (start, end) = (chars(&l.text, m.start()), chars(&l.text, m.end()));
            let labelled = words
                .iter()
                .any(|&w_end| w_end <= start && start - w_end <= CODE_AFTER + 2);
            let near = union(&l.boxes[start..end.min(l.boxes.len())])
                .is_some_and(|r| near_card(r, &cards));
            if labelled || near {
                out.push(Found {
                    line: i,
                    start,
                    end,
                    category: Category::CardExpiry,
                    sure: true,
                });
            }
        }
    }
    out
}

/// Security codes, which only mean something next to a card or a label:
/// isolated 3-4 digit numbers after a keyword on their line, or within
/// `CODE_NEAR` line heights of a card that was found.
fn security_codes(lines: &[Line], found: &[Found]) -> Vec<Found> {
    let cards = cards(lines, found);
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let words: Vec<(usize, usize)> = CODE_WORD
            .find_iter(&l.text)
            .map(|m| (chars(&l.text, m.start()), chars(&l.text, m.end())))
            .collect();
        for (start, end) in isolated_codes(&l.text) {
            let in_card = found.iter().any(|f| {
                f.line == i && f.category == Category::Card && start < f.end && f.start < end
            });
            if in_card {
                continue;
            }
            // The keyword's match may take the code's first digit (`CVV123`).
            let labelled = words
                .iter()
                .any(|&(_, w_end)| w_end <= start + 1 && start + 1 - w_end <= CODE_AFTER + 1);
            let near = union(&l.boxes[start..end]).is_some_and(|r| near_card(r, &cards));
            if labelled || near {
                out.push(Found {
                    line: i,
                    start,
                    end,
                    category: Category::SecurityCode,
                    sure: true,
                });
            }
        }
    }
    out
}
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

/// Text folded for matching names through OCR's confusions (case, `rn`
/// for `m`, `0` for `o`, `1` and `|` for `l`), with each folded char's
/// span in the original, in chars.
fn fold(text: &str) -> (Vec<char>, Vec<(usize, usize)>) {
    let cs: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let (mut out, mut spans) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < cs.len() {
        let (c, n) = match (cs[i], cs.get(i + 1)) {
            ('r', Some('n')) => ('m', 2),
            ('0', _) => ('o', 1),
            ('1' | '|', _) => ('l', 1),
            (c, _) => (c, 1),
        };
        out.push(c);
        spans.push((i, i + n));
        i += n;
    }
    (out, spans)
}

/// Where `needle` occurs in `text` through OCR's confusions, as char
/// spans of `text`. (Lowercasing keeps char counts for the scripts OCR
/// reads here.)
fn fuzzy_find(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let (hay, spans) = fold(text);
    let (needle, _) = fold(needle);
    if needle.is_empty() || needle.len() > hay.len() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut k = 0;
    while k + needle.len() <= hay.len() {
        if hay[k..k + needle.len()] == needle[..] {
            out.push((spans[k].0, spans[k + needle.len() - 1].1));
            k += needle.len();
        } else {
            k += 1;
        }
    }
    out
}

fn detect_line(i: usize, text: &str, rules: &Rules) -> Vec<Found> {
    let env = &rules.env;
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
    let mut found_chars: Vec<Found> = Vec::new();
    let mut push_chars = |start: usize, end: usize, category, sure| {
        found_chars.push(Found {
            line: i,
            start,
            end,
            category,
            sure,
        });
    };
    let on = |c| rules.on(c);
    for m in EMAIL.find_iter(text).filter(|_| on(Category::Email)) {
        let s = m.as_str();
        let clean = !s.contains([' ', '©', '®']) && !s.contains(',');
        push(m.start(), m.end(), Category::Email, clean);
    }
    for m in UUID.find_iter(text).filter(|_| on(Category::Uuid)) {
        push(m.start(), m.end(), Category::Uuid, true);
    }
    for m in PHONE.find_iter(text).filter(|_| on(Category::Phone)) {
        let s = m.as_str();
        let digits = s.chars().filter(char::is_ascii_digit).count();
        let before = text[..m.start()].chars().next_back();
        let after = text[m.end()..].chars().next();
        let joined =
            |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || ".:/".contains(c));
        if (8..=15).contains(&digits) && !joined(before) && !joined(after) && !ISO_DATE.is_match(s)
        {
            // An international or bracketed number looks like one; a run of
            // digit groups may be anything.
            push(
                m.start(),
                m.end(),
                Category::Phone,
                s.starts_with('+') || s.contains('('),
            );
        }
    }
    for m in IPV4.find_iter(text).filter(|_| on(Category::Ip)) {
        let addr = m.as_str().split(':').next().unwrap_or("");
        if addr.parse::<std::net::Ipv4Addr>().is_ok() {
            push(m.start(), m.end(), Category::Ip, true);
        }
    }
    for m in IPV6.find_iter(text).filter(|_| on(Category::Ip)) {
        let s = m.as_str();
        if s.matches(':').count() >= 2
            && s.parse::<std::net::Ipv6Addr>().is_ok()
            && !MAC.is_match(s)
        {
            push(m.start(), m.end(), Category::Ip, true);
        }
    }
    for m in MAC.find_iter(text).filter(|_| on(Category::Mac)) {
        push(m.start(), m.end(), Category::Mac, true);
    }
    for m in TOKEN.find_iter(text).filter(|_| on(Category::Token)) {
        push(m.start(), m.end(), Category::Token, true);
    }
    for c in URL_SECRET
        .captures_iter(text)
        .filter(|_| on(Category::UrlSecret))
    {
        let v = c.get(1).expect("the value group");
        push(v.start(), v.end(), Category::UrlSecret, true);
    }
    for m in CARD.find_iter(text).filter(|_| on(Category::Card)) {
        let d: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
        if (13..=19).contains(&d.len()) && luhn(&d) {
            push(m.start(), m.end(), Category::Card, true);
        }
    }
    for m in IBAN.find_iter(text).filter(|_| on(Category::Iban)) {
        if iban_ok(m.as_str()) {
            push(m.start(), m.end(), Category::Iban, true);
        }
    }
    for m in LONG.find_iter(text).filter(|_| on(Category::Entropy)) {
        let s = m.as_str();
        let mixed =
            s.chars().any(|c| c.is_ascii_digit()) && s.chars().any(|c| c.is_ascii_alphabetic());
        let h = entropy(s);
        if mixed && h >= 3.5 {
            push(m.start(), m.end(), Category::Entropy, h >= 4.0);
        }
    }
    if !env.user.is_empty() && on(Category::HomePath) {
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
    for c in &rules.custom {
        for m in c.regex.find_iter(text).filter(|m| !m.is_empty()) {
            push(m.start(), m.end(), Category::Custom(c.name), true);
        }
    }
    if env.hostname.len() >= 3 && on(Category::Host) {
        for (start, end) in fuzzy_find(text, &env.hostname) {
            push_chars(start, end, Category::Host, true);
        }
    }
    let cs: Vec<char> = text.chars().collect();
    for d in rules.domains.iter().filter(|d| !d.is_empty()) {
        for (mut start, end) in fuzzy_find(text, d) {
            // With its subdomains.
            while start > 0 && (cs[start - 1].is_alphanumeric() || "-.".contains(cs[start - 1])) {
                start -= 1;
            }
            push_chars(start, end, Category::Host, true);
        }
    }
    for l in rules.literal.iter().filter(|l| !l.trim().is_empty()) {
        for (start, end) in fuzzy_find(text, l.trim()) {
            push_chars(start, end, Category::Literal, true);
        }
    }
    found.extend(found_chars);
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

/// Elide a match for the list: its first few characters (four at most,
/// and never more than half, so a short secret isn't shown whole), then "…".
fn elide(s: &str) -> String {
    let keep = (s.chars().count() / 2).min(4);
    let head: String = s.chars().take(keep).collect();
    format!("{head}…")
}

/// Proposed censors for an OCR result: padded by `pad` of the line height,
/// overlapping ones merged (a sure match wins the tick and the label).
/// Overlap is of the matched text, before padding, so matches on lines
/// next to each other stay apart.
pub fn propose(ocr: &OcrResult, rules: &Rules) -> Vec<Proposal> {
    let pad = rules.pad;
    let lines = lines(ocr);
    let mut found: Vec<Found> = lines
        .iter()
        .enumerate()
        .flat_map(|(i, l)| detect_line(i, &l.text, rules))
        .collect();
    // Both read the cards, so before either is added.
    let codes = rules
        .on(Category::SecurityCode)
        .then(|| security_codes(&lines, &found));
    let expiries = rules
        .on(Category::CardExpiry)
        .then(|| card_expiries(&lines, &found));
    found.extend(
        codes
            .into_iter()
            .flatten()
            .chain(expiries.into_iter().flatten()),
    );
    // The surer, longer match first, so merges keep its label.
    found.sort_by_key(|f| {
        (
            std::cmp::Reverse(f.sure),
            std::cmp::Reverse(f.end - f.start),
        )
    });
    // Each proposal, with the unpadded box it merges by.
    let keys = if rules.on(Category::PrivateKey) {
        private_keys(&lines)
    } else {
        vec![]
    };
    // Codes, whole: what one decodes to says what it is, in the label.
    let codes = ocr.codes.iter().filter(|_| rules.codes).map(|c| {
        let what = detect_line(0, &c.text, rules)
            .first()
            .map(|f| format!("{} ", f.category.name()))
            .unwrap_or_default();
        let p = Proposal {
            rect: c.bbox,
            category: Category::Code,
            label: format!("{what}{}", elide(&c.text)),
            ticked: true,
        };
        (p, c.bbox)
    });
    let mut out: Vec<(Proposal, PixelRect)> = codes
        .chain(keys.into_iter().map(|r| {
            let p = Proposal {
                rect: r,
                category: Category::PrivateKey,
                label: "-----BEGIN…".into(),
                ticked: true,
            };
            (p, r)
        }))
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
            codes: vec![],
        }
    }

    fn env() -> Env {
        Env {
            user: "adi".into(),
            home: "/home/adi".into(),
            hostname: "myhost".into(),
        }
    }

    fn rules(pad: f64) -> Rules {
        Rules {
            env: env(),
            pad,
            ..Rules::default()
        }
    }

    fn cats(lines: &[&[&str]]) -> Vec<(&'static str, bool)> {
        cats_with(lines, &rules(0.0))
    }

    fn cats_with(lines: &[&[&str]], rules: &Rules) -> Vec<(&'static str, bool)> {
        propose(&ocr(lines), rules)
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
            &rules(0.0),
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
            &rules(0.0),
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
            &rules(0.5),
        );
        assert_eq!(p.len(), 2, "{p:?}");
    }

    #[test]
    fn a_security_code_near_a_card_is_found_and_the_card_groups_are_not() {
        let card = ["4111", "1111", "1111", "1111"];
        // Below the card, after a keyword; the expiry date is its own.
        let got = cats(&[&card, &["Exp", "12/27", "CVV", "123"]]);
        assert_eq!(
            got,
            [
                ("card", true),
                ("card-expiry", true),
                ("security-code", true)
            ]
        );
        // Below the card, no keyword: an isolated 3-digit number nearby.
        let got = cats(&[&card, &["Name", "on", "card"], &["737"]]);
        assert_eq!(got, [("card", true), ("security-code", true)]);
        // Amex codes are four digits.
        let got = cats(&[&["3782", "822463", "10005"], &["CID", "1234"]]);
        assert_eq!(got, [("card", true), ("security-code", true)]);
    }

    #[test]
    fn a_security_code_ocr_ran_into_its_label_is_still_found() {
        // What tesseract made of "CVV: 123" on a card.
        let card = ["4111", "1111", "1111", "1111"];
        let got = cats(&[&card, &["cvv:123", "Valid", "Thru:", "12/30"]]);
        assert_eq!(
            got,
            [
                ("card", true),
                ("card-expiry", true),
                ("security-code", true)
            ]
        );
        assert_eq!(cats(&[&["CVV123"]]), [("security-code", true)]);
        assert_eq!(cats(&[&["CVC#123"]]), [("security-code", true)]);
        // A time still isn't one, nor a code with a colon after it.
        assert_eq!(cats(&[&card, &["at", "12:30", "123:00"]]), [("card", true)]);
    }

    #[test]
    fn a_short_secret_is_never_shown_whole() {
        assert_eq!(elide("123"), "1…");
        assert_eq!(elide("1234"), "12…");
        assert_eq!(elide("adi@example.org"), "adi@…");
    }

    #[test]
    fn a_keyword_finds_a_security_code_without_a_card() {
        assert_eq!(cats(&[&["CVC:", "4321"]]), [("security-code", true)]);
        assert_eq!(
            cats(&[&["Security", "code", "987"]]),
            [("security-code", true)]
        );
        assert_eq!(cats(&[&["cvv2", "123"]]), [("security-code", true)]);
    }

    #[test]
    fn other_small_numbers_are_left_alone() {
        // No card and no keyword.
        assert_eq!(cats(&[&["room", "123", "at", "12:30"]]), []);
        // A card, but the number is far below it.
        let mut far: Vec<&[&str]> = vec![&["4111", "1111", "1111", "1111"]];
        far.extend(std::iter::repeat_n(&["text"][..], 20));
        far.push(&["123"]);
        assert_eq!(cats(&far), [("card", true)]);
        // Parts of dates, times and decimals aren't codes.
        let got = cats(&[
            &["4111", "1111", "1111", "1111"],
            &["12/123", "1.234", "123:00"],
        ]);
        assert_eq!(got, [("card", true)]);
    }

    #[test]
    fn padding_grows_by_the_line_height() {
        let p = propose(&ocr(&[&["adi@example.org"]]), &rules(0.2));
        assert_eq!(p[0].rect, PixelRect::new(-3, -3, 156, 18));
    }
    #[test]
    fn an_email_ocr_misread_is_proposed_unticked() {
        assert_eq!(cats(&[&["adi©example.org"]]), [("email", false)]);
        assert_eq!(cats(&[&["adi", "@", "example.org"]]), [("email", false)]);
        assert_eq!(cats(&[&["adi@example,org"]]), [("email", false)]);
    }

    #[test]
    fn phone_and_uuid_are_off_until_asked_for() {
        let uuid = ["123e4567-e89b-12d3-a456-426614174000"];
        let phone: &[&str] = &["call", "+44", "20", "7946", "0958"];
        assert_eq!(cats(&[phone]), []);
        // Random-looking, but not enough to be sure.
        assert_eq!(cats(&[&uuid]), [("entropy", false)]);
        let mut on = rules(0.0);
        on.detectors.extend([Category::Phone, Category::Uuid]);
        assert_eq!(cats_with(&[phone], &on), [("phone", true)]);
        assert_eq!(cats_with(&[&uuid], &on), [("uuid", true)]);
        // Bare digit groups are only maybe a phone; a date isn't one.
        assert_eq!(
            cats_with(&[&["0207", "946", "0958"]], &on),
            [("phone", false)]
        );
        assert_eq!(cats_with(&[&["on", "2026-10-05"]], &on), []);
    }

    #[test]
    fn a_detector_turned_off_finds_nothing() {
        let mut r = rules(0.0);
        r.detectors.retain(|c| *c != Category::Email);
        assert_eq!(cats_with(&[&["adi@example.org"]], &r), []);
    }

    #[test]
    fn literals_and_domains_match_through_ocr_confusions() {
        let r = Rules {
            literal: vec!["Acme Corp".into()],
            domains: vec!["corp.example.com".into()],
            ..rules(0.0)
        };
        // "rn" for "m", any case.
        assert_eq!(
            cats_with(&[&["at", "ACRNE", "corp", "today"]], &r),
            [("literal", true)]
        );
        let p = propose(&ocr(&[&["see", "wiki.corp.exarnple.com"]]), &r);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].category, Category::Host);
        assert_eq!(p[0].rect.x, 40); // the subdomain included
        assert_eq!(p[0].rect.w, 22 * 10);
    }

    #[test]
    fn a_custom_regex_is_its_own_category() {
        let r = Rules {
            custom: vec![Custom {
                name: "ticket",
                regex: Regex::new(r"JIRA-\d+").unwrap(),
            }],
            ..rules(0.0)
        };
        assert_eq!(
            cats_with(&[&["fixes", "JIRA-1234"]], &r),
            [("ticket", true)]
        );
    }

    #[test]
    fn detector_names_round_trip() {
        for c in DETECTORS {
            assert_eq!(Category::from_name(c.name()), Some(*c));
        }
        assert_eq!(Category::from_name("codes"), None);
    }

    #[test]
    fn codes_are_proposed_whole_and_labelled_by_what_they_hold() {
        let mut o = ocr(&[&["scan", "me"]]);
        o.codes = vec![
            crate::ocr::OcrCode {
                text: "https://x.io/?token=abc123".into(),
                bbox: PixelRect::new(100, 40, 80, 80),
            },
            crate::ocr::OcrCode {
                text: "4006381333931".into(),
                bbox: PixelRect::new(300, 40, 90, 50),
            },
        ];
        let p = propose(&o, &rules(0.2));
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p.iter().all(|p| p.category == Category::Code && p.ticked));
        assert_eq!(p[0].rect, PixelRect::new(100, 40, 80, 80)); // unpadded
        assert_eq!(p[0].label, "url-secret http…");
        assert_eq!(p[1].label, "4006…");
        let off = Rules {
            codes: false,
            ..rules(0.2)
        };
        assert!(propose(&o, &off).is_empty());
    }

    #[test]
    fn a_card_expiry_is_found_by_its_label_or_its_card() {
        let card = ["4111", "1111", "1111", "1111"];
        // A label, with or without a card; OCR running it in.
        assert_eq!(cats(&[&["Expires", "09/2029"]]), [("card-expiry", true)]);
        assert_eq!(cats(&[&["EXP12/27"]]), [("card-expiry", true)]);
        assert_eq!(cats(&[&["Good", "thru", "1/26"]]), [("card-expiry", true)]);
        // Near a card, no label.
        assert_eq!(
            cats(&[&card, &["MR", "J", "Q", "PUBLIC", "03/28"]]),
            [("card", true), ("card-expiry", true)]
        );
        // The box covers only the date, not its label.
        let p = propose(&ocr(&[&["exp", "05/31"]]), &rules(0.0));
        assert_eq!((p.len(), p[0].rect.x), (1, 40));
    }

    #[test]
    fn other_dates_are_left_alone() {
        // No card and no label: dates are everywhere.
        assert_eq!(cats(&[&["due", "12/27"]]), []);
        // Not a month, nor part of a longer date, even by a card.
        let card: &[&str] = &["4111", "1111", "1111", "1111"];
        assert_eq!(cats(&[card, &["13/27"]]), [("card", true)]);
        assert_eq!(cats(&[card, &["12/27/2024"]]), [("card", true)]);
        assert_eq!(cats(&[card, &["2024/12/27"]]), [("card", true)]);
        // Turned off in the config: none.
        let mut r = rules(0.0);
        r.detectors.retain(|c| *c != Category::CardExpiry);
        assert_eq!(cats_with(&[&["exp", "12/27"]], &r), []);
    }
}
