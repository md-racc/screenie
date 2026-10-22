# Auto-redact

Auto-redact finds likely secrets in the box and proposes censors over
them. It reads the box with OCR (tesseract) and decodes QR codes and
barcodes (zbar), runs detectors over what it reads, and proposes a censor
over each match. You review the proposals and accept them. Nothing is
censored without you, except with `--redact auto`.

Everything runs locally. Nothing leaves the machine.

## Starting a scan

- **`r` in Censor** scans the box. Entering Censor never scans by itself.
- **`--redact suggest`** opens Censor on the box and scans it. In the
  overlay the scan runs in the background, so the picker opens at once and
  the suggestions appear when they're ready. With `--keys`, the scan
  finishes before the keys play, so they can review it. It needs the
  picker: `shot` and `--no-ui` refuse it.
- **`--redact auto`** scans and accepts the ticked suggestions without
  asking, then commits the Censor layer. It runs before `--keys` and before
  the picker opens, and it is the only way to auto-redact with `shot` or
  `--no-ui`.
- `--redact` opens Censor itself, so it can't be combined with `--mode`.
- **`[redact] mode`** (`off`, `suggest` or `auto`; default `off`) is
  `--redact` from the config or a profile (`-p share`). `suggest` is
  skipped under `shot` and `--no-ui`, and `mode` gives way to `--mode`. A
  profile's `[redact] mode` replaces a `[pick] mode` set below it, and the
  other way round.
- A scan that fails (no tesseract, no zbarimg while `codes` is on, an OCR
  error) shows `scan failed: …` in the status. With `--redact auto` it is
  exit 4.

## Reviewing suggestions

Each suggestion is drawn as a dashed box with its category in a chip above
it (below it at a monitor's top). The selected one is drawn brighter.
Ticked ones are filled dark, with the text faintly through, as a preview of
the bar. The side list shows them in reading order:

```text
> ● email      alex@…
  ● token      ghp_…
  · entropy    k9Xq…
```

`>` is the selection, `●` ticked, `·` unticked. A label shows at most the
first four characters of the match, and never more than half of it, then
`…`. The status shows `scanning…`, then `N found (M ticked)`.

| Key | Action |
|---|---|
| `[` / `]` | Select the next suggestion down / up |
| `Space` | Tick or untick it |
| `d` | Drop it |
| `a` / `Enter` | Accept the ticked ones as censors, and return to Censor |
| `r` | Scan again |
| `Esc` | Drop the suggestions, and return to Censor |

- Accepting adds one censor per ticked suggestion to the open Censor
  layer, as one undo step: `u` in Censor takes them all back. Unticked
  suggestions are dropped.
- Accepted suggestions are ordinary censors of `[redact] kind`: a black
  bar by default, or pixelation at 10px blocks. A bar is the default
  because pixelated text can sometimes be recovered.
- Suggestions are never saved. Only accepted censors reach the project.
- Low-confidence matches are proposed **unticked**: visible, but not
  applied unless you tick them. Missing a secret is worse than an extra
  box, so the detectors lean toward proposing.

## How the box is read

- The scan reads the box as committed: layers already committed are in
  it, the open Censor layer is not, so censors you are drawing don't hide
  text from it.
- The crop is turned grey, inverted when it's mostly dark, and upscaled 2×
  when the capture's scale is below 1.5. Tesseract reads it in the OCR
  language (`--ocr-lang`, `[ocr] lang`; default `eng`) and returns each
  word with its box.
- Each character gets a slice of its word's box, split by character
  count. A match in the middle of a word (a token in a URL) censors only
  that part.
- Detectors run one OCR line at a time. Private-key blocks are the
  exception: they run from their BEGIN line to their END line.
- With `codes` on, zbarimg reads the same crop. Each code is proposed
  whole, by its outline's bounds.

## Detectors

These run unless `[redact] detectors` says otherwise:

| Detector | Finds | Rule |
|---|---|---|
| `email` | Email addresses | `name@domain.tld`. OCR's misreads (`©` or `®` for `@`, spaces round the `@`, `,` for `.`) are proposed unticked. |
| `ip` | IPv4 addresses (with an optional `:port`) and IPv6 addresses | Must parse as an address (`999.1.1.1` doesn't). An IPv6 address needs two groups and a digit, so `std::fs` and `DB::Add` aren't addresses. |
| `mac` | MAC addresses | Six hex pairs joined by `:` or `-`. |
| `token` | Keys with a known prefix | `sk-`, `ghp_` `gho_` `ghu_` `ghs_` `ghr_`, `github_pat_`, `glpat-`, `AKIA`, `xoxb-` `xoxa-` `xoxp-` `xoxs-` `xoxr-`, and JWTs (`eyJ…`, three dot-separated parts). |
| `entropy` | Long random-looking strings | 20 or more characters of `A-Z a-z 0-9 + / = _ -`, with both letters and digits, and a Shannon entropy of at least 3.5 bits per character. Ticked from 4.0. |
| `url-secret` | Secret URL parameters | The value after `?` or `&` of `token`, `key`, `api_key`, `apikey`, `secret`, `sig`, `signature`, `code`, `password`, `pass`, `auth` or `access_token`, in any case. Only the value is censored. |
| `card` | Card numbers | 13 to 19 digits that pass the Luhn check, written as one group or in groups of 4 or more digits (the last may have 3): 4-4-4-4, 4-6-5, 4-4-4-4-3. Digits beside a card on its line (a list number before it, an expiry or code after it) are left off until it passes. At most one group before the card is tried, and a row of short numbers is never a card. |
| `security-code` | A card's CVV, CVC or CID | 3 or 4 digits on their own (not part of a longer number, a date, a decimal or a time), within 6 characters after a keyword: `CVV`, `CVV2`, `CVC`, `CVC2`, `CID`, `CSC`, `security code` or `card code`, in any case, also when OCR runs it into the digits (`CVV123`). Or such digits within 4 line heights of a card found in the same scan, and no further to its side than the card's width or 10 line heights, whichever is more. The card's own digits never count. |
| `card-expiry` | A card's expiry date | `MM/YY` or `MM/YYYY` with a real month, not part of a longer date or number, within 8 characters after a keyword: `exp`, `exp.`, `exp date`, `expire`, `expires`, `expired`, `expiry`, `expiring`, `expiration`, `valid thru`, `valid through`, `valid until` or `good thru`. A keyword is a whole word (not `export` or `expected`), but may be run into the date (`EXP12/27`). Or such a date within 4 line heights of a card found in the same scan, and no further to its side than the card's width or 10 line heights, whichever is more. A lone date anywhere else is never proposed. |
| `iban` | IBANs | Two capital letters, two digits, then 11 to 30 capitals or digits (single spaces allowed), passing the mod-97 check. Words after one on its line (a BIC, a currency) are cut off until it passes. |
| `home-path` | Your home directory in paths | `/home/$USER`, `~$USER` or `$HOME`, up to the next space. Another user's is not (`/home/alexa` for `alex`). |
| `host` | This machine's hostname | Matched as a name (below), when it has 3 or more characters. |
| `private-key` | `-----BEGIN … PRIVATE KEY-----` blocks | The whole block, through its END line. One with no END line runs to the last line read. |
| `codes` | QR codes and barcodes | Every code zbar finds, whatever it holds (a code can hold a URL with a token, or WiFi credentials). Its label starts with the detector its decoded text matches: `code url-secret http…`. |

These are off unless `[redact] detectors` names them, since they give too
many false positives:

| Detector | Finds | Rule |
|---|---|---|
| `phone` | Phone numbers | 8 to 15 digits in groups split by spaces, `.` or `-`, with an optional `+country` and `(area)`. Not a date, and not run into letters, `.`, `:` or `/`. Ticked only when it starts with `+` or has brackets. |
| `uuid` | UUIDs | 8-4-4-4-12 hex digits. |

These come from the config, and run whenever they're set, whatever
`detectors` says:

| Key | Finds | Category |
|---|---|---|
| `literal` | Strings you list (your name, your employer), matched as names | `literal` |
| `domains` | Domains you list, with their subdomains, matched as names | `host` |
| `[[redact.custom]]` | A regex (Rust `regex` syntax) under a `name`; empty matches are ignored | its `name` |

**Names** (the hostname, `domains`, `literal`) match as whole words, with
both sides folded through OCR's usual confusions: any case, `rn` for `m`,
`0` for `o`, `1` and `|` for `l`.

Low-confidence matches are the only unticked ones: an email with OCR's
misreads, an `entropy` string under 4.0 bits, and a `phone` number without
`+` or brackets. The checksums (Luhn, mod-97) keep the number detectors
precise; `entropy` and the OCR-tolerant matches are there for recall.

## From matches to censors

- A match's character boxes are joined into one rectangle, then padded on
  every side by `pad` (default 0.2) times its line's height, rounded up,
  so ascenders, descenders and anti-aliasing are covered.
- Matches that overlap merge into one suggestion. Overlap is judged on the
  text before padding, so matches on neighbouring lines stay apart even
  when their padding meets. The surer, longer match gives the merged
  suggestion its category and label, and it is ticked if any of its
  matches is.
- A match inside a code or a private-key block joins that suggestion.
- Suggestions are cut to the box, and listed top to bottom, then left to
  right.

## Speed and caching

- Tesseract takes about 0.2–0.5 s on a typical box, and 1–2 s on a full
  1080p screen. In the overlay each scan runs on its own thread, so the
  picker stays live. With `--keys`, `--no-ui`, `shot` and
  `--redact auto`, each scan finishes before anything else happens.
- The last 8 scans are kept, by the committed picture's version and the
  scanned rect. A scan of a rect inside a kept one (same pixels; with
  codes only if the kept one read codes) is answered at once, with the
  kept scan's words and codes that reach into the rect: a word the box
  cuts still counts. A scan inside one still running waits for it. A box
  that grew, or pixels that changed, scan again.
- **Prescan** (`[redact] prescan = true`; `pick` and `edit` only): when
  the picker opens, each monitor of the committed picture is scanned on a
  background thread, with codes if `codes` is on, so the first `r` is
  usually instant. It spends CPU on every shot, so it is off by default.
  Its light-or-dark choice is the whole monitor's, so a dark window on a
  light screen may read less well than the box's own scan would.

## Output

- Accepted suggestions are censors like any other, in the image and the
  project.
- With `--json`, `"redacted": [{"what": "email", "rect": {…}}, …]` lists
  the accepted suggestions still in the picture (not undone, deleted, or
  in a hidden layer). `what` is the category (`code` for a code, a custom
  detector's `name`), and `rect` is in the shot's logical coordinates (a
  window's own pixels for `shot active`). The list is there whenever
  `--redact` was given, even if it's empty, or a suggestion was accepted.
  A script can check it.

## Config

```toml
[redact]
mode = "off"         # off | suggest | auto: --redact from the config
kind = "bar"         # bar | pixelate (10px blocks): what accepted suggestions become
detectors = ["email", "ip", "mac", "token", "entropy", "url-secret", "card",
             "security-code", "card-expiry", "iban", "home-path", "host",
             "private-key", "codes"]   # the default; add "phone", "uuid"
literal = ["Acme Corp"]
domains = ["corp.example.com"]
pad = 0.2            # of the line height, 0 to 2
prescan = false      # scan each monitor when the picker opens

[[redact.custom]]
name = "ticket"
regex = 'JIRA-\d+'
```

- `detectors` replaces the default list: what it doesn't name doesn't run,
  `codes` included. An unknown name is a config error.
- Only `mode` has a command-line option (`--redact`). The other keys are
  set in the config or a profile.

See [cli.md](cli.md) for `--redact`, `--json` and profiles, and
[picker.md](picker.md#censor) for Censor.

## Honesty

- It is **best effort**. OCR misses small, stylised, low-contrast and
  rotated text, and text in images. `--redact auto` says so in its help.
  Check the result, or the `redacted` list.
- Detectors read one line at a time, so a secret that wraps across a line
  break (a long token at a terminal's edge) is caught only if one piece is
  long and random enough for `entropy` on its own. Censor it by hand.
- It finds only text it can read, and codes. Faces, avatars and
  handwriting are not detected.
- Pixelation obscures; it is not secure redaction. Keep `kind = "bar"` for
  secrets.
