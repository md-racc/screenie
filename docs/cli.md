# screenie(1): the command-line interface (Rust port)

**Status:** the Rust port's CLI contract. The Python prototype keeps its
three flags (`--window`, `--save-dir`, `--lang`) and won't grow this
interface. Picker behaviour and keys are specified in `README.md` (which
wins over this document), and will become the man page's KEYS section.
How the port is built is `docs/architecture-rust.md`.

Decided so far:
- verbs, not flat flags;
- sources (`--from`) are separate from regions (`--region`);
- `shot` exists alongside `pick`, and `edit` is the one editing verb;
- `--select-only` (slurp-style output) is in scope;
- the clipboard is on by default, except when stdout carries data;
- `--json` is the machine interface (there is no `--exec` hook);
- `--keys` scripts the picker, and profiles keep binds short;
- shots keep an editable project file in the runtime directory.

Further design lives in its own documents:
- `docs/auto-redact.md`: censor suggestions from OCR;
- `docs/measure.md`: measuring, with a scale;
- `docs/annotate.md`: text, arrows, callouts, the highlighter.

The remaining open questions are at the end.

## SYNOPSIS

```
screenie [pick] [--from SOURCE] [--region REGION] [PICK OPTIONS] [OUTPUT OPTIONS]
screenie shot [REGION] [--from SOURCE] [CAPTURE OPTIONS] [OUTPUT OPTIONS]
screenie edit FILE|last[:N]|- [PICK OPTIONS] [OUTPUT OPTIONS]
screenie history [-n N] [--json]
screenie pin [FILE|last[:N]|-] [--at X,Y]
screenie pin list [--json]
screenie pin close [ID...|--all]
screenie help [keys|regions|config]
screenie completions bash|zsh|fish
screenie --version
screenie daemon                  (reserved, see DAEMON)
```

Bare `screenie` is `screenie pick`, so a hotkey bind stays one word.

Global options: `-p, --profile NAME` (repeatable), `--config PATH`,
`--no-config`, `--no-history`, `--json`, `--quiet`.

## VERBS

`pick`
: The interactive picker: freeze the source, pick a box, and optionally
  censor, annotate, filter or measure it. Then deliver the result. Enter's
  default result is an image; `y` gives text, `p` a colour, `Q` a code and `P`
  a pin; Measure (`m`) gives a measurement (see `--result`).

`shot [REGION]`
: No UI: take REGION of the source (default: `active`) and deliver it.
  `shot active` replaces the prototype's `--window`, and `shot --from last`
  delivers the last shot again.

`edit FILE|last[:N]|-`
: The picker over an existing image, for annotating or censoring an old
  shot. This is `pick --from … --region all`.
  - `last` (or `last:2` for the one before it) restores the shot's project
    (see FILES), so its layers are still editable: a censor can be moved,
    and a filter deleted.
  - A plain image opens flattened.

`history`
: Recent shots, newest first: time, size, where from, and path. With
  `--json`, it prints one line per shot.

`pin [FILE|last[:N]|-]`
: Float an image on every workspace (default: `last`). It goes where the
  image came from if screenie took it, or else to `--at`, or else the
  centre of the focused monitor. Prints the pin's ID.

`pin list` / `pin close [ID...|--all]`
: The open pins. Pins are screenie's own windows, not swayimg's, so it can
  list and close them.

`help keys|regions|config`
: Print a section of the man page (the picker's keys, the REGION grammar,
  the config keys). This is the full reference; in the picker, `?` shows
  only the current mode's legend.

## SOURCES AND REGIONS

The **source** is the picture, and the **region** is the part of it. The
two are separate flags, so `-` (stdin) is never ambiguous: `--from -` is an
image on stdin, and `--region -` is a geometry on stdin. Passing both is a
usage error (exit 2).

`--from SOURCE`

| Source | Meaning |
|---|---|
| `screen` | the frozen screen (the default) |
| `FILE` | an image file (PNG, JPEG, WebP, PPM) |
| `last`, `last:N` | a past shot, with its project (layers, layout) |
| `-` | an image on stdin |

`--region REGION` (and `shot`'s positional REGION)

| Region | Meaning |
|---|---|
| `active` | the focused window, from its own pixels |
| `window:class:REGEX` | a window by class |
| `window:title:REGEX` | a window by title |
| `window:address:0x…` / `window:pid:N` | an exact window |
| `monitor` / `monitor:NAME` | the focused monitor, or one by name (`DP-3`) |
| `all` | everything in the source (the whole layout, or the whole image) |
| `X,Y WxH` | a rectangle (slurp's format, so `slurp` output pastes in) |
| `last-region`, `last-region:N` | the box the previous (Nth previous) accepted selection used: the picker's `.` ring |
| `-` | read one of the above from stdin (`slurp \| screenie shot -`) |

Window regions:
- Several matches are resolved by focus history (the most recently focused
  wins), not by stacking order. `--json` reports which window was taken.
- A window is captured from its own pixels (Hyprland's toplevel-export),
  so a covered window, or one on another workspace, still captures whole.
- No match, or `active` with nothing focused, is exit 3.

Other sources:
- With `--from FILE` or `-`, only `all` and `X,Y WxH` apply (in image
  pixels). Window and monitor regions are exit 2.
- `--from last` keeps the layout it was taken with, so every region works.

`last-region` is anchored to its monitor by name (it follows a rearranged
layout), and falls back to the global rect, clamped, when that monitor is gone.

## COORDINATES

- Regions are in **logical** layout coordinates (Hyprland's), and may be
  fractional: the picker doesn't round a drag. Output is in device pixels.
- Logical to device pixels is round half up at each edge. That's the
  prototype's `rnd` rule: the crop, the edge pixels and the cursor pixel
  always agree.
- Printed integer geometries (`--select-only` as slurp, `hypr`) round
  **outward**, so pasting one back never loses a pixel. `--region-format
  json` prints the exact floats.
- A box across monitors with different scales is captured at the
  **highest** scale (the lower-scale monitor is upsampled). `--scale`
  changes that.

## PICK OPTIONS

`--region REGION`
: The initial box (default: the focused window, as today).

`--mode select|insert|censor|annotate|filter|measure`
: Open straight into a mode: for example, a "shoot and censor" bind. The
  build tools open a layer as if their key had been pressed.

`--lock-box`
: The box can't be changed, only worked inside.

`--result image|text|colour|code|pin|measure`
: What Enter produces. The result keys still work.
  `screenie --result colour --stdout` is a colour picker for scripts.

`--keys SEQUENCE`, `--keys-file FILE|-`
: Play keys into the picker before the user gets it. This drives the
  picker from scripts, macros and the port's integration tests.
  - The sequence is space-separated key names (`x l l Return`), with vim
    modifiers (`<C-l>`, `<S-space>`, `<A-S-h>`). Mouse events are
    `click:X,Y`, `drag:X,Y:X,Y`, `rclick` and `wheel:±N`.
  - If the sequence reaches a result, the picker never appears.
  - Otherwise the picker opens in the state the keys left it. With
    `--no-ui`, that's exit 2 instead.

  Example: `screenie --region active --keys 'x <S-l> Return' --no-ui`
  censors the window's left part, then saves.

`--redact suggest|auto`
: Run auto-redact on the box (see `docs/auto-redact.md`). `suggest` opens
  Censor with the suggestions ready to accept. `auto` applies them as they
  are, the only way to use it with `shot` or `--no-ui`, and is best effort.
  It runs before `--keys`, so scripted keys can review what it found.
  `--json` lists what was redacted whenever `--redact` is given or a
  suggestion was accepted, with rects in the shot's logical coordinates
  (a window's own pixels for `shot active`).

`--measure-scale PX=LENGTH`
: Start Measure calibrated: `--measure-scale '84.2px=1m'`, for a plan
  opened at the same zoom every time.

`--select-only`
: Pick a box and print its geometry, without capturing or delivering
  anything: screenie as a slurp. Layers are ignored.

`--region-format slurp|json|hypr`
: `slurp`: `X,Y WxH` (the default). `json`: `{"x":…,"y":…,"w":…,"h":…}`,
  unrounded. `hypr`: `X Y W H`.

## CAPTURE OPTIONS (pick and shot)

`--delay SECONDS`
: Wait before capturing. The picker shows a countdown chip.

`--cursor`
: Include the mouse pointer in the capture.

`--scale native|min|FACTOR`
: Output resolution.
  - `native`, the default, is the highest scale in the box.
  - `min` is the lowest, so no monitor is upsampled.
  - `FACTOR` is relative to logical size: `1` is logical, `2` for docs,
    `0.5` for chat.

## OUTPUT OPTIONS

Output options are composable and apply to any verb that produces a result.

`--copy` / `--no-copy`
: The clipboard. It's on by default, **except** when stdout carries the
  data: `--stdout` and `--select-only` imply `--no-copy` unless `--copy` is
  given explicitly. `--json` doesn't, because it only describes the result.
  - Text results are copied as text.
  - A saved image is offered as `image/png`, plus `text/uri-list` when it
    was saved, so file-drop targets get the file. `[output] copy-uri =
    false` turns that off, for an app that pastes the path over the picture.
  - Wayland's clipboard lives only as long as its owner, so screenie forks
    a small child that serves the selection until something else is copied
    (as wl-copy does).

`--save [DIR|TEMPLATE]`
: Also save to a file. With no argument, the configured directory is used,
  else `$XDG_PICTURES_DIR/Screenshots`.
  A TEMPLATE may use strftime fields plus:
  - `{class}` `{title}` `{monitor}`: where the box was;
  - `{w}` `{h}`: the size in output pixels;
  - `{kind}`: the result type;
  - `{n}`: a counter.

  A name that already exists gets `-1`, `-2`… (two shots in a second never
  overwrite).

`--save-project`
: Also save the project next to the image (`NAME.screenie`, see FILES), so
  it can be edited later with its layers. **It holds the uncensored
  capture.**

`--stdout`
: Write the result to stdout: image bytes in `--format`, or the text,
  colour, code or measurement.

`--json`
: Write one JSON line to stdout describing what happened (see JSON).
  `--stdout` and `--json` both claim stdout, so passing both is a usage
  error.

`--pin`
: Also float the result.

`--format png|jpeg|webp|ppm` and `--quality N`
: The image encoding (default png). ppm is fastest for piping to another
  tool.

`--png-level 0-9`
: The zlib level. The default is 1 (fast, a few percent bigger).

`--colour-format hex|rgb|hsl`
: For colour results (default hex: `#rrggbb`).

`--ocr-lang LANG`
: The tesseract language (default eng). This replaces the prototype's
  `--lang`.

`--notify` / `--quiet`
: A desktop notification per result (on by default when stdout isn't a
  terminal). `--quiet` also silences stderr notes.

## JSON

With `--json`, screenie prints one line on success, and also on cancel so
scripts can tell the two apart:

```json
{"version":1,"status":"done","kind":"image",
 "rect":{"x":1920,"y":0,"w":800,"h":600},"px":{"w":800,"h":600},
 "monitor":"DP-3","window":{"class":"firefox","title":"…","address":"0x…"},
 "layers":["censor ×2","filter"],"redacted":[{"what":"email","rect":{…}}],
 "path":"/home/…/2026-09-29_14-02-11.png","project":"…","copied":true}
```

What varies by result:
- `kind` is one of `image`, `text`, `colour`, `code`, `pin`, `measure`,
  `region`.
- Text-like results carry `"text"` instead of `"path"`. Colours also carry
  `"rgb":[r,g,b]`. Measurements carry `"measurements"` and `"scale"` (see
  `docs/measure.md`).
- `--select-only` gives `kind: region`.
- A cancel is `{"version":1,"status":"cancelled"}`. A failure is
  `{"version":1,"status":"error","code":3,"message":"…"}`.

`version` bumps on any breaking change to the schema.

## EXIT STATUS

| Code | Meaning |
|---|---|
| 0 | done |
| 1 | cancelled (Esc, q); only ever a user's decision |
| 2 | usage error (bad flags, `--stdout` with `--json`, a bad REGION, `--no-ui` keys that don't finish) |
| 3 | nothing to capture, or capture failed (no window matched, no screencopy) |
| 4 | delivery failed (clipboard, file, OCR, a missing zbar or tesseract) |

## CONFIG

The config file is `$XDG_CONFIG_HOME/screenie/config.toml` (TOML).
- Every long option has a key of the same name, grouped by section.
- The command line wins over profiles, and profiles win over the base
  config.
- `--config PATH` uses a different file; `--no-config` ignores it (useful
  for tests).

```toml
[output]
copy = true
copy-uri = true
save = "~/Pictures/Screenshots/%Y-%m-%d_%H-%M-%S.png"
format = "png"
png-level = 1
colour-format = "hex"

[ocr]
lang = "eng"

[pick]
region = "active"

[history]
keep = 20           # projects kept in the runtime dir; 0 is --no-history

[redact]            # see docs/auto-redact.md
detectors = ["email", "ip", "token", "card", "iban", "home-path"]
literal = ["my-hostname"]

[look]              # the overlay
dim = 0.45
tag-opacity = 0.92
ink = ["white", "black", "red", "green", "blue"]

[keys.select]       # explicit keymap tables
capture = ["Return", "y"]

# Profiles: named overrides, applied left to right with -p.
[profile.share]
output.format = "webp"
output.save = false
redact.mode = "suggest"

[profile.docs]
capture.scale = 2
pick.mode = "annotate"
```

## FILES

`$XDG_RUNTIME_DIR/screenie/`
: tmpfs: private (0700) and cleared at logout. `last` holds the newest
  shot's path.

`$XDG_RUNTIME_DIR/screenie/shots/NAME.png`, `NAME.screenie`
: Each shot's output and its **project**, which `edit last`, `--from last`
  and `history` read. The newest `[history] keep` shots are kept.
  - A project is the uncensored capture, the layout snapshot (monitors,
    windows, scales), the box, the layer stack as ops, and any measure
    calibration. The file is a PNG of the capture, with the versioned
    project as JSON in a `screenie-project` text chunk.
  - Projects are 0600 and never leave the runtime directory unless you
    pass `--save-project`.
  - `--no-history` writes neither the project nor the copy.

`$XDG_STATE_HOME/screenie/regions.json`
: The last 10 accepted boxes (for `.` and `last-region`), anchored to their
  monitors. Only coordinates, so it lives in the state dir and outlives
  reboots.

`$XDG_RUNTIME_DIR/screenie/sock`
: The daemon's socket (reserved).

## EXAMPLES

```sh
screenie                                        # the picker
screenie shot --save                            # the focused window, saved and copied
screenie shot monitor:DP-3 --stdout | swappy -f -   # no clipboard clobbering
slurp | screenie shot - --json                  # someone else's picker, screenie's delivery
screenie --select-only                          # screenie as a region picker
screenie --result colour --stdout               # a colour picker for scripts
screenie --mode censor --region last-region     # redo the last box, censoring
screenie edit last                              # reopen the last shot, layers intact
screenie shot --from last --save                # save the last shot after all
screenie shot --redact auto -p share            # a window, auto-redacted, as webp
screenie --result measure --json | jq .measurements
screenie pin close --all
```

Hyprland binds (these replace today's `--window` / `--save-dir` binds):

```
bind = , Print, exec, screenie --save
bind = SHIFT, Print, exec, screenie shot --save
bind = CTRL, Print, exec, screenie -p share
```

## MIGRATING FROM THE PROTOTYPE

| Prototype | Port |
|---|---|
| `screenie` | `screenie` |
| `--window` | `shot` (or `shot active`) |
| `--save-dir DIR` | `--save DIR` |
| `--lang LANG`, `$SCREENIE_LANG` | `--ocr-lang LANG`, `[ocr] lang` |
| `$XDG_RUNTIME_DIR/screenie/last` | the same, plus `shots/` and projects |

## NON-GOALS

- **Recording.** A region for wf-recorder is `screenie --select-only`, and
  that's the whole integration.
- **Scrolling / long captures.** They're unreliable on Wayland.
- **Uploading.** `--json` or `--stdout` into a script covers it.
- **Beautify** (shadows, padding, rounded corners, backgrounds). That's
  another tool's job, via `--stdout`.
- **Live (unfrozen) picking.** It conflicts with the frozen-frame model.

## DAEMON (reserved)

`screenie daemon` is reserved. Keep a daemon out until measurements show
it's needed.

The prototype, measured on two 1080p monitors:

| Phase | Time |
|---|---|
| hotkey to the overlay on screen | ~270 ms |
| of which Python start | ~40 ms |
| of which GTK import | ~94 ms |
| of which hyprctl | ~27 ms |
| of which waiting for grim | ~48 ms |
| of which building the windows | ~32 ms |

A Python daemon would save ~150 ms.

A fresh-launch Rust binary is estimated at 30–50 ms. That's a 1–2 ms start,
in-process screencopy instead of grim's 12 MB PPM, direct socket IPC with
Hyprland, and surfaces plus the first frame in ~10–20 ms. A daemon would
save only ~5–15 ms of that.

A daemon's costs:
- resident memory;
- following hotplugs and scale changes through the event socket;
- an old daemon surviving upgrades;
- a crashed or hung daemon eating the hotkey, or holding the keyboard.

If one is ever added:
- The socket protocol is the `--json` schema: a request is argv, and the
  reply is the JSON line.
- The client is a thin binary that falls back to running in-process when
  no daemon answers, so nothing about the interface changes.
- The clipboard-serving child (see `--copy`) is needed either way.

## OPEN QUESTIONS

- `--notify` default: on when stdout isn't a terminal (hotkeys), off in a
  shell?
- `edit -` from stdin plus `--stdout`: allowed (a filter in a pipeline), or
  too clever?
- Should `--result text` (OCR) put confidence or per-word boxes in
  `--json`? Auto-redact computes them anyway.
- Shell completions for REGION: complete window classes and monitor names
  live from Hyprland?
