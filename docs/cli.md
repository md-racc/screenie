# screenie(1)

## NAME

screenie: a keyboard-driven screenshot picker for Hyprland

## SYNOPSIS

```
screenie [pick] [--from SOURCE] [--region REGION] [PICK OPTIONS] [CAPTURE OPTIONS] [OUTPUT OPTIONS]
screenie shot [REGION] [--from SOURCE] [CAPTURE OPTIONS] [OUTPUT OPTIONS]
screenie edit [FILE|-|clipboard|recent|last[:N]] [PICK OPTIONS] [OUTPUT OPTIONS]
screenie history [-n N] [--json]
screenie pin [FILE|-|clipboard|recent|last[:N]] [--at X,Y] [--json]
screenie pin list [--json]
screenie pin close ID...|--all [--json]
screenie help [keys|regions|config]
screenie completions bash|zsh|fish
screenie --version
```

## DESCRIPTION

Bare `screenie` is `screenie pick`, so a hotkey bind stays one word.

The picker's keys, mouse and modes are specified in [picker.md](picker.md).
This page covers the command line: verbs, sources and regions, options,
JSON, exit status, the config file and the files screenie keeps.

## VERBS

### `pick`

The interactive picker: freeze the source, pick a box, and optionally
censor, annotate, filter or measure it, then deliver the result. Enter
gives an image by default (see `--result`); `y` gives OCR text, `p` a
colour, `Q` a QR or barcode's text, `P` a pin, and Measure (`m`) a
measurement.

### `shot [REGION]`

No picker: take REGION of the source (default `active`) and deliver it.
`shot --from last` delivers the last shot again.

### `edit [FILE|-|clipboard|recent|last[:N]]`

The picker over an existing image, for annotating, censoring, OCR or
pinning an old shot. It is `pick --from SOURCE --region all`, and takes
`--region` too, but not `--from`. With no argument it opens `recent`.
- `last` (or `last:2` for the one before it) restores the shot's project
  (see FILES), so its layers are still editable: a censor can be moved
  and a filter deleted. If the monitors have changed since, it opens
  with its layers drawn in, like a project PNG.
- A plain image, or a shot that kept no project, opens flattened.
- A file whose name starts with `-` goes after `--`:
  `screenie edit -- -photo.png`.

### `history [-n N]`

Recent shots, newest first, one per line: local time
(`%Y-%m-%d %H:%M:%S`), size (`WxH`) and path. `-n` caps the count
(default 20). With `--json`, each line is an object with `time`, `w`,
`h`, `path`, and `project` when the shot kept one.

### `pin [FILE|-|clipboard|recent|last[:N]] [--at X,Y]`

Float an image above every workspace (default `last`, the shot's box
with its layers drawn in). It goes to `--at` (its top-left corner, in
logical coordinates) if given, else, for a past shot that kept a project,
where its box was, else the centre of the focused monitor. Anything but
such a shot is shown at one pixel per logical pixel. Prints the
pin's ID, or with `--json` a report of `kind: pin`.
- A pin is screenie's own window, with app id `screenie-pin` and title
  `screenie pin ID`. It needs a window rule that floats and pins that
  class (see the [README](../README.md#hyprland-setup)).
- Dragging a pin moves it; Esc, `q` or the middle button close it.
- Each pin is a small screenie process of its own; there is no daemon.

### `pin list`

The open pins, one per line: ID, size (`WxH`), `at X,Y` when it was
placed, and the path of its image. With `--json`, each line is an
object with `id`, `size` (`[w, h]`), `at` (`[x, y]` or `null`) and
`path`.

### `pin close ID...|--all`

Close pins. An ID that isn't open is exit 2; the others still close.
With `--json`, it prints a report of `kind: pins` with the IDs it
`closed`.

### `help [keys|regions|config]`

With no topic (or `-h`, `--help`), the usage summary. `keys` prints the
picker's keys for every mode, `regions` the REGION grammar, and
`config` the config keys (`keys` and `config` are generated from the
tables the picker and parser use). In the picker, `?` shows only the
current mode's legend.

### `completions bash|zsh|fish`

Print a completion script for the shell on stdout.

### `--version`

Print `screenie VERSION`.

`history`, `pin`, `help` and `completions` take only the global options
(below); any other option is exit 2.

## SOURCES AND REGIONS

The **source** is the picture, and the **region** is the part of it. They
are separate flags, so `-` (stdin) is never ambiguous: `--from -` is an
image on stdin, and `--region -` is a geometry on stdin. Passing both is
exit 2, as is `--keys-file -` with either.

### `--from SOURCE`

| Source | Meaning |
|---|---|
| `screen` | the frozen screen (the default) |
| `FILE` | an image file (PNG, JPEG, WebP, PPM) of up to 16384² pixels; a JPEG is turned upright by its EXIF orientation |
| `last`, `last:N` | a past shot, with its project (layers, layout); one that kept none (a window shot, an opened image) is a plain image, as `FILE` is |
| `clipboard` | the image on the clipboard; a screenie shot there (its copies say so, as `application/x-screenie-shot`) reopens with its project, like `last` |
| `recent` | the clipboard's image if it holds one, else `last` |
| `-` | an image on stdin |

### `--region REGION` (and `shot`'s positional REGION)

| Region | Meaning |
|---|---|
| `active` | the focused window, from its own pixels |
| `window:class:REGEX` | a window by class |
| `window:title:REGEX` | a window by title |
| `window:address:0x…` | an exact window |
| `window:pid:N` | a window by its process |
| `monitor`, `monitor:NAME` | the focused monitor, or one by name (`DP-1`) |
| `all` | everything in the source: the whole layout, or the whole image |
| `X,Y WxH` | a rectangle in slurp's format, so `slurp` output pastes in |
| `last-region`, `last-region:N` | the box of the last (Nth last) accepted selection: the picker's `.` ring |
| `-` | one of the above, read as a line from stdin (`slurp \| screenie shot -`) |

`shot`'s REGION may be given as separate words (`shot 10,20 300x200`), or
with `--region`, but not both (exit 2).

On the screen:
- Several window matches resolve by focus history (the most recently
  focused wins), not by stacking order. `--json` reports which window was
  taken.
- A window is captured from its own pixels (ext-image-copy-capture), so a covered window, or one on another workspace, still captures whole.
- No match, `active` with nothing focused, or a monitor or `last-region:N`
  that isn't there is exit 3. A malformed REGION is exit 2.
- An `X,Y WxH` on no monitor, or with no area, is exit 3. One partly off
  the monitors keeps its size, with the part off them black.
- `last-region` is anchored to its monitor by name, so it follows a
  rearranged layout. When that monitor is gone, it falls back to the
  global rectangle, clamped.

Over an image (`--from FILE`, `-` or `clipboard`, or `edit` of one):
- Only `all` and `X,Y WxH` apply, in the image's pixels. Window, monitor
  and `last-region` regions are exit 2.
- A rectangle must lie within the image: one partly off it is exit 2, and
  one with no area, or entirely off it, is exit 3.
- The picker shows the image on the focused monitor, centred, at the
  monitor's scale or more, so a big image is shown smaller but every
  pixel is kept. The box starts on all of it.
- A screenie project PNG (from `--save-project`, or one beside a shot)
  reopens with its layers when its monitors are still there, and
  otherwise opens with them drawn in, never as the bare capture. One this
  screenie can't read (a newer screenie's) is exit 3.
- An opened image doesn't join the `.` ring or keep a project of its own.

Over a past shot (`--from last`):
- The shot keeps the layout it was taken with, so `all`, its monitors,
  rectangles, `last-region` and `active` (the box of the window that was
  focused then) work. `window:` regions need the live screen: exit 2.
- `shot --from last` delivers the shot again with its layers drawn in,
  over its own box unless a region is given, and keeps its project again.

## COORDINATES

- Regions are in **logical** layout coordinates (Hyprland's) and may be
  fractional: the picker doesn't round a drag. Output is in device
  pixels.
- Logical coordinates become device pixels by rounding half up at each
  edge, so the crop, the edge pixels and the cursor pixel always agree
  (see [picker.md](picker.md), Coordinates and scaling).
- Printed integer geometries (`--region-format slurp` and `hypr`) round
  **outward**, so pasting one back never loses a pixel.
  `--region-format json` prints the exact values.
- A box across monitors with different scales is captured at the
  **highest** scale, and the lower-scale monitor is upsampled. `--scale`
  changes that.

## OPTIONS

Options may come before or after the verb, except `pin`'s `--at` and
`--all` and `history`'s `-n`, which follow it. `--` ends the options.

### Global options (every verb)

| Option | Meaning |
|---|---|
| `-p NAME`, `--profile NAME` | Apply the config profile `[profile.NAME]`. Repeatable, applied left to right. |
| `--config PATH` | Read this config file instead (it must exist). |
| `--no-config` | Read no config file. Exit 2 with `--config` or `-p`. |
| `--json`, `--no-json` | One JSON line on stdout describing what happened (see JSON). Exit 2 with `--stdout`. |
| `--quiet`, `--no-quiet` | Silence warnings on stderr, such as a config section skipped. Errors are still printed. |
| `--no-history` | Keep no history copy or project (see FILES). The box still joins the `.` ring. |
| `-h`, `--help`, `--version` | See VERBS. |

### Pick options (pick, edit)

| Option | Meaning |
|---|---|
| `--region REGION` | The initial box. Default for `pick`: the focused window, else the middle of the focused monitor; for `edit`: `all`. |
| `--mode MODE` | Open straight into `select`, `insert`, `censor`, `annotate`, `filter` or `measure` (a "shoot and censor" bind). The build modes open a layer as if their key had been pressed. |
| `--lock-box`, `--no-lock-box` | Lock the box: it can't be changed, only worked inside. |
| `--result RESULT` | What Enter produces: `image` (default), `text`, `colour`, `code`, `pin` or `measure`. The result keys still work. |
| `--select-only` | Pick a box and print its geometry, capturing and delivering nothing else: screenie as slurp. |
| `--region-format FORMAT` | How `--select-only` prints the box: `slurp` (`X,Y WxH`, the default), `json` (`{"x":…,"y":…,"w":…,"h":…}`, unrounded) or `hypr` (`X Y W H`). Only with `--select-only`. |
| `--measure-scale PX=LENGTH` | Start Measure calibrated: `--measure-scale '84.2px=1m'`, for a plan opened at the same zoom every time (see [measure.md](measure.md)). |
| `--keys SEQUENCE` | Play keys into the picker before the user gets it. |
| `--keys-file FILE\|-` | The same, read from a file, or from stdin for `-`. Exit 2 with `--keys`. |
| `--no-ui` | Never show the picker: keys that don't reach a result are exit 2. |

**`--select-only`**

- Layers are ignored. A finish that has no box (`p`'s colour, Measure's
  measurements) is a cancel.
- The geometry goes to stdout (or into the JSON line's `text` with
  `--json`), and to the clipboard only with `--copy`.
- It can't be combined with `--result` (exit 2).

**`--keys` and `--keys-file`**

- A sequence is whitespace-separated tokens: key names (`x`, `L`,
  `Return`, `space`, `dollar`), keys with vim-style modifiers (`<C-l>`,
  `<S-space>`, `<A-S-h>`; `M-` is Alt too), and mouse events in logical
  coordinates: `click:X,Y`, `drag:X,Y:X,Y`, `rclick` and `wheel:±N`. An
  unknown token is exit 2.
- If the sequence reaches a result, the picker never appears. Otherwise
  the picker opens in the state the keys left it, or with `--no-ui` it is
  exit 2.
- Example: `screenie --region active --keys 'x <S-l> Return Return'
  --no-ui` censors a small square near the window's centre, commits it,
  and delivers the image.

### Auto-redact (pick, shot, edit)

**`--redact suggest|auto`**

Run auto-redact on the box (see [auto-redact.md](auto-redact.md)).

- `suggest` opens Censor at once and proposes the suggestions when the
  scan is in. It needs the picker: with `shot` or `--no-ui` it is
  exit 2.
- `auto` applies the suggestions as they are, without asking. It is the
  only form for `shot` and `--no-ui`, and is best effort: OCR misses
  small, stylised, low-contrast and rotated text, and text in images.
- Either runs before `--keys`, so scripted keys can review what it
  found.
- `--redact` opens Censor itself, so it can't be combined with `--mode`
  (exit 2).
- `--json` lists what was redacted whenever `--redact` is given or a
  suggestion was accepted, with rects in the shot's logical coordinates
  (a window's own pixels for a window shot).

### Capture options

| Option | Verbs | Meaning |
|---|---|---|
| `--delay SECONDS` | pick, shot | Wait before capturing (0 to 3600). |
| `--cursor`, `--no-cursor` | pick, shot | Include the mouse pointer in the capture. |
| `--scale native\|min\|FACTOR` | pick, shot, edit | The output resolution. |

`--delay` and `--cursor` are for capturing the screen: with a `--from`
other than `screen`, or with `edit`, they are exit 2.

**`--delay`**

- When notifications are on, a notification says how long, and closes
  half a second before the capture. A delay under a second has none.
  There is no countdown on screen, since it would be in the capture.
- The window, monitor or region is chosen after the wait.

**`--scale`**

- `native`, the default, is the highest scale of the monitors the box is
  on (not the whole layout).
- `min` is the lowest, so no monitor is upsampled.
- `FACTOR` (0.05 to 8) is output pixels per logical pixel: `1` is
  logical size, `2` for docs, `0.5` for chat.
- An opened image's native size is its own pixels, and a window shot's is
  the window's.

### Output options (pick, shot, edit)

Output options compose: one result can be copied, saved, written to
stdout and pinned at once.

| Option | Meaning |
|---|---|
| `--copy`, `--no-copy` | Put the result on the clipboard. On by default, except when stdout carries the data. |
| `--copy-uri`, `--no-copy-uri` | Offer a saved image's file as `text/uri-list` too (default on). |
| `--save [DIR\|TEMPLATE]`, `--no-save` | Also save the image to a file. |
| `--save-project`, `--no-save-project` | Also save the project beside the saved image. |
| `--stdout` | Write the result to stdout. Exit 2 with `--json`: both claim stdout. |
| `--pin`, `--no-pin` | Also float the result as a pin. |
| `--format png\|jpeg\|webp\|ppm` | The image encoding (default `png`; `jpg` is `jpeg`). |
| `--quality N` | JPEG quality, 1 to 100 (default 90). Other formats ignore it. |
| `--png-level 0-9` | The PNG zlib level (default 1: fast, a few percent bigger). |
| `--colour-format hex\|rgb\|hsl` | For colour results: `#rrggbb` (default), `rgb(r, g, b)` or `hsl(h, s%, l%)`. `--color-format` is the same. |
| `--ocr-lang LANG` | The tesseract language for OCR and auto-redact (default `eng`). |
| `--notify`, `--no-notify` | A desktop notification per result. On by default when stdout isn't a terminal. |

**`--copy`**

- `--stdout` and `--select-only` imply `--no-copy`, unless `--copy` is
  given explicitly. `--json` doesn't, because it only describes the
  result. A config's `copy = true` is the default, not an explicit
  `--copy`.
- Text results are copied as text.
- An image is offered in `--format`, plus `image/png` when that's
  another format, plus `application/x-screenie-shot` (the history copy's
  path, so `--from clipboard` can reopen its project), plus
  `text/uri-list` when it was saved, so file-drop targets get the file.
  `--no-copy-uri` drops the URI, for an app that pastes the path over
  the picture.
- Wayland's clipboard lives only as long as its owner, so screenie starts
  a small detached copy of itself that serves the selection until
  something else is copied (as wl-copy does).

**`--save`**

- With no argument, the image goes to `$XDG_PICTURES_DIR/Screenshots`,
  named by the local time (`%Y-%m-%d_%H-%M-%S`).
- A DIR saves there under the same name. `~` is the home directory, and
  a missing directory is created.
- A TEMPLATE names the file. It may use strftime fields plus:
  - `{class}`, `{title}`: the window, for window shots;
  - `{monitor}`: the monitor at the box's centre;
  - `{w}`, `{h}`: the size in output pixels;
  - `{kind}`: the result's kind (`image`);
  - `{n}`: a counter: the number of files already in its directory, plus
    one.

  A field with no value, or an empty one, is `unknown`. A field's text is
  cut to 100 bytes, and a `/` in it becomes `-`.
- A template ending in `/` names a directory (`~/shots/%Y-%m/`), and the
  file in it gets the usual time stamp.
- A plain file name (`out.png`, `~/shots/today.webp`: an image extension
  and no fields) is a template naming one file, unless a directory of
  that name exists.
- The extension follows `--format`, replacing one the template gives, so
  a name always says what's in the file.
- A name that already exists gets `-1`, `-2`…, so two shots in one second
  never overwrite each other.
- `--save VALUE` takes the next word as its value only when it has a
  `/`, starts with `~` or `.`, or ends in an image extension (and isn't
  an option), so `shot --save monitor` saves the focused monitor. For any
  other value, write `--save=VALUE`.
- Saved files are mode 0644.

**`--save-project`**

Writes `NAME.screenie` beside the saved image (see FILES), so it can be
reopened later with its layers. **It holds the uncensored capture.**
Window shots and opened images have no project to save.

**`--stdout`**

Image bytes in `--format`, or the text, colour, code or measurement
followed by a newline.

**`--format`**

Applies to `--save`, `--stdout` and the clipboard. The history copy that
`last` reads stays PNG. WebP is lossless. PPM is fastest for piping to
another tool.

## JSON

With `--json`, screenie prints one line on stdout when it finishes,
including on cancel and on failure, so scripts can tell them apart:

```json
{"version":1,"status":"done","kind":"image",
 "rect":{"x":1920,"y":0,"w":800,"h":600},"px":{"w":800,"h":600},
 "monitor":"DP-1","layers":["censor ×2","filter"],
 "redacted":[{"what":"email","rect":{"x":2010,"y":120,"w":180,"h":16}}],
 "path":"/home/you/Pictures/Screenshots/2026-09-29_14-02-11.png",
 "project":"…","copied":true}
```

| Field | Meaning |
|---|---|
| `version` | The schema version, `1`. It bumps on any breaking change. |
| `status` | `done`, `cancelled` or `error`. |
| `kind` | `image`, `text`, `colour`, `code`, `pin`, `measure`, `region` (`--select-only`), or `pins` (`pin close`). |
| `rect` | The box, in logical coordinates, unrounded. Not for window shots, colours or measurements. Over an opened image, `shot` gives it in the image's pixels, and the picker where it showed the image on screen. |
| `monitor` | The monitor at the box's centre, when there is one. |
| `px` | The delivered image's size in pixels (`image`). |
| `window` | `class`, `title` and `address` of the window taken (window shots). |
| `layers` | The visible layers, described (images from the picker). |
| `redacted` | What auto-redact censored: `what` and `rect` for each (see `--redact`). |
| `path` | `image`: the saved file, else the history copy. `pin`: the pin's image. |
| `project` | The project file written, if one was: the `--save-project` one, else the history copy's (none for a window shot, an opened image, or `--no-history` alone). |
| `copied` | Whether the result went to the clipboard. |
| `text` | The text of a `text`, `colour`, `code`, `measure` or `region` result; the ID for the `pin` verb. |
| `rgb` | A colour's `[r, g, b]`. |
| `measurements`, `scale` | A measurement's data (see [measure.md](measure.md)). |
| `id` | A pin's ID. |
| `closed` | The IDs `pin close` closed. |
| `code`, `message` | An error's exit status and message. |

- A cancel is `{"version":1,"status":"cancelled"}`. A failure is
  `{"version":1,"status":"error","code":3,"message":"…"}`, and the message
  goes to stderr too.
- OCR that finds no text is done, with an empty `text` and
  `"copied":false` (and a "No text found" notification).
- A command line that doesn't parse prints the usage on stderr and no
  JSON line.
- `history` and `pin list` print one object per line instead (see VERBS).

## EXIT STATUS

| Code | Meaning |
|---|---|
| 0 | done |
| 1 | cancelled: Esc or `q`, a `--select-only` finish with no box, or the picker closed without a result |
| 2 | usage error: bad or conflicting options, an option the verb doesn't take, a malformed REGION or region the source can't have, `--no-ui` keys that don't finish, a bad config file, a `pin close` ID that isn't open |
| 3 | nothing to capture, or capture failed: no window, monitor, remembered box or past shot matched; a region that's empty, on no monitor, or off the image; an unreadable image or project; no image on the clipboard or stdin; no screencopy; no monospace font for the picker |
| 4 | delivery failed: clipboard, file, encoding, OCR, a missing zbar or tesseract |

## CONFIG

The config file is `$XDG_CONFIG_HOME/screenie/config.toml`
(`~/.config/screenie/config.toml` by default). It's optional; a file
named with `--config` must exist. `help`, `completions` and `--version`
don't read it.

Keys are the long options of the same name, grouped by section. A flag
takes `true` or `false` (its `--no-` form); an option with a value takes
a string or a number, as on the command line. Each key applies only to
the verbs listed, and the others ignore it.

| Section | Key | Option | Verbs |
|---|---|---|---|
| `[output]` | `copy`, `notify`, `quiet`, `json`, `save-project`, `pin`, `copy-uri` | the flag | pick, shot, edit |
| | `save` | `--save`: a directory or template, `true` (the default directory) or `false` | pick, shot, edit |
| | `format`, `quality`, `png-level`, `colour-format` | the option | pick, shot, edit |
| `[ocr]` | `lang` | `--ocr-lang` | pick, shot, edit |
| `[pick]` | `region` | `--region` | pick |
| | `mode`, `result`, `measure-scale`, `lock-box` | the option | pick, edit |
| `[capture]` | `delay`, `cursor` | the option | pick, shot |
| | `scale` | `--scale` | pick, shot, edit |
| `[history]` | `keep` | `0` is `--no-history`; any other count keeps the usual 20 | pick, shot, edit |
| `[redact]` | `mode` | `off`, `suggest` or `auto`: `--redact` | pick, shot, edit |
| | `kind`, `detectors`, `literal`, `domains`, `pad`, `custom` | auto-redact's rules (see [auto-redact.md](auto-redact.md)) | pick, shot, edit |
| | `prescan` | `true`: scan each monitor when the picker opens | pick, edit |

Profiles are named overrides: `[profile.NAME]` holds any of the keys
above (as dotted keys or nested tables), and `-p NAME` applies it over the
base config. Several `-p` apply left to right, later ones winning. `-p`
with no config file, or a profile that isn't there, is exit 2.

Precedence and clashes:
- The command line wins over profiles, and profiles win over the base
  config. Every flag has a `--no-` form, so the command line can turn a
  config's flag off for one run.
- A config option that would clash with the command line gives way to
  it: a region, delay or cursor when the picture isn't the screen
  (`--from`, `edit`); `json` under `--stdout`; `[pick] mode` under
  `--redact`, and `[redact] mode` under `--mode`; `[redact] mode =
  "suggest"` under `shot` or `--no-ui`; `[pick] result` under
  `--select-only`.
- In the same way, a profile's `[pick] mode` or `[redact] mode` replaces
  the other one from below it. Both set in one layer is exit 2.
- `[redact]`'s lists (`detectors`, `literal`, `domains`, `custom`) and
  `kind`, `pad` and `prescan` have no options, so only profiles override
  them.

Errors: malformed TOML, an unknown key or a value of the wrong type is
exit 2, naming the key. `[look]` and `[keys.*]` are reserved: screenie
warns about them (unless `--quiet`) and skips them.

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

[history]
keep = 20           # 0 is --no-history

[redact]            # see docs/auto-redact.md
detectors = ["email", "ip", "token", "card", "iban", "home-path"]
literal = ["my-hostname"]

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

### `$XDG_CONFIG_HOME/screenie/config.toml`

The config file (see CONFIG).

### `$XDG_RUNTIME_DIR/screenie/`

The runtime directory: private (0700) and, on tmpfs, cleared at logout.
Without an absolute `XDG_RUNTIME_DIR`, it's `screenie-UID` in the temp
directory, and screenie refuses one that isn't a directory of its own
user's.

### `$XDG_RUNTIME_DIR/screenie/last`

The newest shot's path.

### `$XDG_RUNTIME_DIR/screenie/shots/NAME.png`, `NAME.screenie`

Each delivered image's history copy (PNG) and, for a shot of the screen,
its **project**, which `edit last`, `--from last`, `pin last` and
`history` read. Window shots and opened images keep no project. The
newest 20 shots are kept; `--no-history` (or `[history] keep = 0`)
writes neither file.
- A project file is a PNG of the uncensored capture, with the versioned
  project as JSON in a `screenie-project` text chunk. What a project holds
  is in [picker.md](picker.md#projects-and-history).
- History copies and projects are 0600, and a project never leaves the
  runtime directory unless `--save-project` writes one beside a saved
  image.

### `$XDG_RUNTIME_DIR/screenie/pin-NAME.png`, `pins/ID.json`

Each open pin's image and record, for `pin list` and `pin close`.
Removed when the pin closes.

### `$XDG_STATE_HOME/screenie/regions.json`

The last 10 accepted boxes (for `.` and `last-region`), anchored to
their monitors. Only coordinates, so it lives in the state directory
(`~/.local/state` by default) and outlives reboots. `--no-history`
doesn't cover it.

### `$XDG_PICTURES_DIR/Screenshots/`

`--save`'s default directory (`~/Pictures/Screenshots` when the
variable is unset; screenie reads the environment, not
`user-dirs.dirs`).

## ENVIRONMENT

| Variable | Use |
|---|---|
| `WAYLAND_DISPLAY`, `HYPRLAND_INSTANCE_SIGNATURE` | The session to capture and the Hyprland instance to query. |
| `XDG_CONFIG_HOME`, `XDG_RUNTIME_DIR`, `XDG_STATE_HOME`, `XDG_PICTURES_DIR` | See FILES. |
| `HOME` | `~` in paths, the XDG fallbacks, and auto-redact's home-path detector. |
| `TMPDIR` | The runtime directory's fallback location. |
| `USER`, `HOSTNAME` | Auto-redact's home-path and host detectors (the host name comes from `/etc/hostname` first). |
| `SCREENIE_DEADLINE=SECS` | Close the picker after SECS seconds, as a cancel: a safety net for scripted and test runs. |

## EXAMPLES

```sh
screenie                                        # the picker
screenie shot --save                            # the focused window, saved and copied
screenie shot monitor:DP-1 --stdout | swappy -f -   # no clipboard clobbering
slurp | screenie shot - --json                  # another picker, screenie's delivery
screenie --select-only                          # screenie as a region picker
screenie --result colour --stdout               # a colour picker for scripts
screenie --mode censor --region last-region     # redo the last box, censoring
screenie edit last                              # reopen the last shot, layers intact
screenie shot --from last --save                # save the last shot after all
screenie shot --redact auto -p share            # a window, auto-redacted, as webp
screenie --result measure --json | jq .measurements
screenie pin close --all
screenie completions zsh > ~/.zfunc/_screenie
```

Hyprland binds and the pin window rule are in the
[README](../README.md#hyprland-setup).

## NON-GOALS

- **Recording.** A region for wf-recorder is `screenie --select-only`, and
  that's the whole integration.
- **Scrolling or long captures.** They're unreliable on Wayland.
- **Uploading.** `--json` or `--stdout` into a script covers it.
- **Beautifying** (shadows, padding, rounded corners, backgrounds). That's
  another tool's job, via `--stdout`.
- **Live (unfrozen) picking.** It conflicts with the frozen-frame model.
- **An `--exec` hook.** `--json` is the machine interface.

## SEE ALSO

[picker.md](picker.md), [auto-redact.md](auto-redact.md),
[measure.md](measure.md), slurp(1), tesseract(1), zbarimg(1),
notify-send(1)
