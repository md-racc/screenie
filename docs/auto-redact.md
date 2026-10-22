# Auto-redact: censor suggestions from OCR

**Status:** in the Rust port: `r` in Censor, `--redact suggest|auto`,
`"redacted"` in `--json`, card security codes, the OCR-tolerant email,
`phone`/`uuid`, `literal`/`domains`/`custom`, and `[redact]` config
codes (zbar's outlines), and the scan cache and prescan (all
2026-10-05). It is not in the prototype. `README.md` (Auto-redact) and `docs/architecture-rust.md`
(§54–57) take precedence over this page.

Screenshots leak things you didn't notice were in the box:
- an email in a sidebar;
- a token in a URL;
- your home path in a terminal title;
- a QR code on a slide.

screenie already has OCR, a QR decoder and a censor tool. Auto-redact joins
them: it finds likely secrets and proposes censors over them. **You accept
them, and it never silently decides for you**, except with an explicit
`--redact auto`.

## Where it lives

- **In the picker:** press `r` in Censor to scan the box and propose
  censors. Censor's current keys (`t`, `x`, `+`/`-`) don't use `r`.
- **From the command line:**
  - `--redact suggest` opens Censor with the suggestions already proposed.
  - `--redact auto` applies them without asking, for `shot` and `--no-ui`
    (see `docs/cli.md`).
- **Config:** `[redact]` (below), and profiles (`-p share` can turn it on).

## Pipeline

```
box pixels ──► OCR with word boxes ──► lines of text, each char mapped to its box
                                            │
       zbar (codes + their outlines) ──┐    ▼
                                       ├─► detectors ──► matches (char spans, codes)
       literal / custom config ────────┘                   │
                                                           ▼
                        merge spans ─► union of their word boxes ─► pad ─► proposed censors
```

1. **OCR with geometry.**
   - Tesseract's TSV/hOCR output gives each word's box and confidence. The
     picker's OCR already upscales 2× at 1× scale and flips dark
     backgrounds; reuse that.
   - Keep a character-to-box map, so a match in the middle of a word (the
     token part of a URL) censors only that part. Split word boxes
     proportionally by character count.
2. **Detectors** run on each OCR line. Each detector returns char spans in
   that line, a category and a confidence. Private-key blocks are the
   exception: they're matched from BEGIN to END across lines.
3. **Codes.** zbar reports each code's outline. A QR or barcode is proposed
   whole, whatever it contains (a code can hold a URL with a token or
   WiFi credentials). The decoded text is also run through the detectors,
   for the list's label (`code url-secret http…`). zbarimg reads the
   scan's own crop, in the same background job as the OCR.
4. **Boxes.**
   - Merge the spans of one match into the union of their char boxes.
   - Pad by ~20% of the line height, so ascenders, descenders and
     anti-aliasing are covered.
   - Merge censors that overlap.
5. **Proposals** become pending censor ops of the configured kind, marked
   as suggested and not yet locked.

## Detectors

| Detector | What | How |
|---|---|---|
| `email` | addresses | regex; OCR's misreads (`©`/`®` for `@`, spaces round it, `,` for `.`) are proposed unticked |
| `ip` | IPv4, IPv6, with or without port | regex, plus a parse check |
| `mac` | MAC addresses | regex |
| `token` | known key prefixes: `sk-`, `ghp_`/`gho_`/`github_pat_`, `glpat-`, `AKIA…`, `xox[bap]-`, JWTs (`eyJ…`) | prefix regexes |
| `entropy` | long random-looking strings (≥20 chars, high Shannon entropy) | entropy per char, with an OCR-noise allowance |
| `url-secret` | secret-looking query params in URLs (`token=`, `key=`, `sig=`, `code=`) | URL parse, param names |
| `card` | card numbers | digits plus a Luhn check; digits next to a card on its line (a list number, an expiry, a CVC) are left off until it passes |
| `security-code` | a card's CVV/CVC/CID | 3-4 isolated digits after a keyword (`CVV`, `CVC`, `CID`, `CSC`, "security code"), or within 4 line heights of a found card |
| `card-expiry` | a card's expiry date | `MM/YY` or `MM/YYYY` (a real month) after a keyword, a whole word (`exp`, `expiry`, `expires`, `valid thru`, `good thru`; not `export`), or within 4 line heights of a found card; never a lone date elsewhere |
| `iban` | IBANs | the mod-97 check; words after one on its line (a BIC, a currency) are left off until it passes |
| `phone` | phone numbers | 8-15 digits in groups, not a date; off by default (too many false positives); ticked only with `+` or brackets |
| `uuid` | UUIDs | regex, off by default |
| `home-path` | `$HOME/…`, `/home/<user>/…`, `~user` | `$USER` / `$HOME` |
| `host` | this machine's hostname, and domains you list (with subdomains) | match through OCR's confusions (below) |
| `private-key` | `-----BEGIN … PRIVATE KEY-----` blocks | propose the whole block |
| `literal` | strings from config (your name, your employer) | match through OCR's confusions (below) |
| `custom` | regexes from config | `[[redact.custom]] name = "…"`, `regex = "…"` |

Names (`host`, `domains`, `literal`) are matched as whole words (a domain
also with its subdomains), with both sides folded through OCR's usual
confusions: any case, `rn` for `m`, `0` for `o`, `1` and `|` for `l`. A custom detector's chip and `--json` `what` are its
`name`.

Checksums (Luhn, mod-97) keep the number detectors precise. The entropy and
OCR-tolerant detectors are for recall.

## UX in Censor

- Suggestions draw as **dashed** censors, previewed as the censor kind,
  with a category chip each ("email", "token"…).
- The side list shows them, top to bottom, like the filter stack:

  ```
  ● email   adi@…
  ● token   ghp_…
  · ip      10.0.…     (unticked: low confidence)
  ```

- Keys:

  | Key | Does |
  |---|---|
  | `[` `]` | select a suggestion |
  | Space | tick / untick it |
  | `a` | accept the ticked ones as locked censors (undoable, as one action) |
  | `d` | drop one |
  | `r` | rescan (after moving the box) |

  Accepted suggestions are ordinary censors from then on (`u` undoes the
  acceptance). Esc drops the suggestions, not the layer.
- Low-confidence matches are proposed **unticked**, so they're visible but
  not applied. Missing a secret is worse than an extra box, so the
  thresholds lean toward proposing.
- The status tag shows `scanning…` and then `5 found (4 ticked)`, and the
  list's labels show the matched text elided (`adi@…`), never whole.

## Performance

- Tesseract on a full 1080p screen takes about 1–2 s, and on a typical box
  0.2–0.5 s. That's too slow to block on, so:
  - scan in a worker thread, so the picker stays live and the suggestions
    appear when ready;
  - optionally start scanning the whole frame in the background as soon as
    the picker opens (`[redact] prescan = true`), then filter to the box
    instantly. That spends CPU on every shot, so it's off by default.
- Cache the OCR per frame version and region. Rescans after small box
  moves reuse the word boxes that are still inside.
- As built: the last 8 scans are kept, by the committed picture's version
  and rect. A scan of a rect inside a kept one (same pixels, and codes
  only from a scan that looked for them) is its words and codes reaching
  into the rect, at once; one inside a scan still running waits for it.
  A box that grew, or pixels that changed, scan again. Prescan reads each
  monitor (not the whole layout, so the box's monitor is enough), on its
  own thread, with codes. Its light/dark choice is the monitor's, not the
  box's, which may read a dark window on a light screen less well than
  the box's own scan (tesseract retries inverted lines); so it stays off
  by default.

## Honesty

- It's **best effort**. OCR misses small, stylised, low-contrast or
  rotated text, and text in images. `--redact auto` must say so in its
  help, and `--json` lists what it redacted (`"redacted": [...]`) so a
  script can check.
- Detectors read one line at a time, so a secret that wraps across a line
  break (a long token at a terminal's edge) is caught only if a piece is
  long and random enough for `entropy` on its own. Censor it by hand.
- It works only on text it can read, plus codes. Faces, avatars and
  handwriting are out of scope; a face detector could be an optional
  detector later.
- Everything runs locally. Nothing leaves the machine.

## Config

```toml
[redact]
mode = "off"              # off | suggest | auto (the default for --redact)
kind = "bar"              # bar | pixelate (10px blocks), for accepted suggestions
detectors = ["email", "ip", "mac", "token", "entropy", "url-secret", "card",
             "security-code", "card-expiry", "iban", "home-path", "host",
             "private-key",
             "codes"]         # the default; add "phone", "uuid"
literal = ["my-hostname", "Acme Corp"]
domains = ["corp.example.com"]
pad = 0.2                 # of the line height
prescan = false           # scan each monitor when the picker opens

[[redact.custom]]
name = "ticket"
regex = 'JIRA-\d+'
```

## Testing

- A fixture set of screenshots with known secrets, planted at several
  sizes, fonts, themes (dark and light) and scales, with the expected
  boxes.
- Measure recall (every planted secret covered) and precision (extra
  boxes) per detector. Recall is the gate.
- `--keys 'r a Return' --no-ui` drives it end to end (see `docs/cli.md`).

## Open questions

- Should Censor scan automatically on entry, or only on `r`? Auto is nicer,
  but costs ~0.3 s of CPU on every censor.
- Should suggestions use the bar even when the censor default is pixelate?
  Pixelated text can be recovered (Depix, Unredacter), and these are
  secrets by definition.
- Should URL secrets censor the whole URL, or only the secret's value?
