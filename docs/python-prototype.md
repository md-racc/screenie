# Python prototype

This is the reference implementation. It is **not** a design for Rust:
`README.md` specifies behaviour, and `docs/architecture-rust.md` specifies
the rewrite. This page is for running, testing and comparing against the
prototype during the port.

The prototype is frozen: it gets no new features and no CLI growth. It is
still in daily use until the Rust port reaches parity.

## Running

```
bin/screenie [--save-dir DIR] [--window] [--lang LANG]
```

| Flag | What it does |
|---|---|
| no flags | the picker |
| `--save-dir DIR` | also save images to `DIR/<timestamp>.png` (`-1`, `-2`… on collision) |
| `--window` | capture the active window's own pixels, even if covered (`grim -T`), with no picker |
| `--lang` | the tesseract language (default `eng`, or `$SCREENIE_LANG`) |

`bin/screenie` runs the package straight from the checkout. `pip install
-e .` also provides a `screenie` script.

The user's Hyprland Print binds call
`bin/screenie` (with `--window`, and with
`--save-dir ~/Pictures/Screenshots`). Pins are swayimg windows
with app id `screenie-pin`, which a Hyprland window rule pins.

**Dependencies (Arch):**
- required: `python-gobject python-cairo gtk3 gtk-layer-shell grim
  wl-clipboard libnotify`, plus Hyprland (`hyprctl`);
- optional: `tesseract tesseract-data-eng` (OCR), `zbar` (QR and
  barcodes), `swayimg` (pins).
- Python is 3.11 or newer; it's developed on 3.14.

**Files it writes:**

| Path | What |
|---|---|
| `$XDG_RUNTIME_DIR/screenie/` | every image (unless `--save-dir`), plus `last`, the newest path |
| `$XDG_STATE_HOME/screenie/regions.json` | the `.` ring, 10 accepted boxes |
| `/tmp/screenie-*.png` | grim's capture, deleted after loading |

## Tests

```
python -m unittest discover -s tests
```

At `bc247ad` there are **105 tests, all passing**, in about 0.7 s.
- **67 need a Wayland session** (`WAYLAND_DISPLAY` set): `test_modes.py`,
  `test_audit.py`, and `test_recall.Dot`. They build the real `App`, whose
  layer-shell windows are created but never shown, and drive it with
  synthetic key and mouse events on a synthetic 3840×1080 frame with two
  fake monitors.
- Without Wayland those 67 are **skipped**, not failed, and the other 38
  run: geometry, tag placement, Hyprland visibility rules, and the `.`
  ring.
- Nothing reads machine state: `test_recall` uses a temporary
  `$XDG_STATE_HOME`, and no test touches the real runtime directory.
- No flakiness was seen over repeated runs.

`test_modes.py` and `test_audit.py` are the **executable behaviour
reference** for the port:
- `test_audit.py` holds one regression test per finding of the multi-agent
  audit (commit `d6b5385`).
- `Harness` in `test_modes.py` is the event-driving fixture.

`pyproject.toml` configures pyright (`extraPaths`), but nothing gates on
it. It currently reports about 165 errors, nearly all type-inference noise
from duck typing (`app.mode` inferred as `Select`, base methods returning
`None`, optional attributes). Treat it as informational.

## Layout of the code

| Module | What |
|---|---|
| `cli.py` | arguments; carrying out the finished `Action` (image / ocr / text / pin); remembering the box for `.` |
| `hypr.py` | `Layout` (hyprctl monitors, clients, activewindow, and which windows are visible); `Capture` (grim, started before anything else) |
| `frame.py` | `Frame`: base, committed, and the lazily rebuilt view (committed plus the open layer); prefix snapshots; preview cache; view filters; crops; the OCR crop |
| `layers.py` | ops (`CensorOp`, `StrokeOp`, `ShapeOp`, `FilterOp`, each with `key()` and `bbox()`); `Layer`; Select-history undo entries |
| `geometry.py` | `Selection`: the box, its visual-block cursor, snapping, scope, jumps, history. No GTK |
| `app.py` | `Overlay` (one per monitor); `App`: input plumbing, key repeat, the mode stack, the open layer and its undo log, tag layout |
| `modes/` | `base` (the Mode interface), `box` (shared box editing), `select`, `insert`, `loupe`, `enhance`, `censor`, `filter`, `annotate` (and Shape), `layer` (the build tools' shared keys), `layers_view` |
| `draw.py` | drawing helpers and tag placement (`place`, `block`, the spot lists) |
| `keys.py` | key-name sets; the mouse arrives as keys |
| `output.py` | the PNG encoder, clipboard, saving, notify, OCR, QR, pins |
| `recall.py` | the `.` ring |

A run goes like this:

```
cli: Capture() starts grim → Layout() → Frame(capture)
   → App.run() (Gtk.main) → App.finish(Action)
   → cli delivers the result → recall.remember(box)
```

## Numbers (for parity)

The README deliberately leaves out most constants. These are the
prototype's, by module:

| What | Value | Where |
|---|---|---|
| box step (a tap) | 5 logical px | `geometry.STEP` |
| repeat: delay before it starts / ramp to top speed | 0.08 s / 0.6 s | `app.DELAY`, `app.RAMP` |
| box repeat rate | 20 → 300 steps/s | `box.RATES` |
| window-gap skip for edge jumps | 12 px | `geometry.GAP` |
| Insert step | 5 px, range 1–20; Shift ×4 | `insert.STEP`, `BIG` |
| Insert repeat | starts at 40 px/s; top speed crosses the box in 1 s | `insert.START_PX`, `CROSS` |
| Loupe window | 21 frame px, range 9–61, steps of 4, odd; 10 screen px per pixel | `app.loupe_px`, `loupe.SIZE_*`, `ZOOM` |
| Loupe repeat | 15 → 200 px/s; Shift 4 → 12 loupe-widths/s | `loupe.RATES`, `PAGE_RATES` |
| Enhance | starts at 2×, taps snap to 0.5, range 1–64×; lens is half the box; Shift ×4 | `enhance.*` |
| Censor | a 20 px starting square; pixelate 10 px (4–40, step 2); bar shade 0 (0–1, step 0.1) | `censor.SIZE`, `AMOUNTS`, `layers.PIXELATE_BLOCK` |
| Filter defaults (kind, start, step, min–max) | grey 1.0 (0.25, 0.25–1); invert; brightness 1 (1, −5–5); contrast 1 (1, −4–4); blur 4 (2, 2–20) | `filter.KINDS` |
| Annotate | red, 3 px, range 1–40 | `app.ink`, `annotate.WIDTH_*` |
| Tags | row 20 px, margin 4 px; an inside block may cover at most 35% of the box | `draw.ROW`, `MARGIN`, `INSIDE_MAX` |
| View filter contrast | 2 (one S-curve pass) | `frame.VIEW_CONTRAST` |
| Open-layer snapshots kept | 12 | `frame.SNAPSHOTS` |
| OCR upscale | 2× when the frame scale is under 1.5 | `output.OCR_UPSCALE` |
| `.` ring | 10 | `recall.RING` |

## Details worth knowing before comparing

**Input**
- **The mouse is fed in as keys.** The wheel is `plus`/`minus`, right
  click is `Return`, and middle click is `Escape`. Modes never see those
  buttons as buttons.
- **GTK key names vary:** `plus`/`equal`/`KP_Add`, `braceright`, `dollar`,
  `period`. See `keys.py`. Both `z` and `Z` are the loupe.
- **Key repeat is screenie's own.** GTK's autorepeat is ignored.
  - Held direction keys are tracked by keycode, so `k`+`l` is diagonal.
  - Each mode gives a start and top rate, ramped over `RAMP`.
  - Keys held across a mode change are `stale` until they're released.

**Overlays**
- **Every overlay is keyboard-exclusive.** While an exclusive layer is up,
  Hyprland routes the pointer only to exclusive layers, so an overlay
  without the keyboard would never see a click. The focused monitor's
  overlay is shown last so that it gets the focus.
- GTK3 can't name monitor connectors, so overlays are matched to monitors
  by **logical position**.

**Pixels**
- **One capture at the highest scale.** grim renders the whole layout;
  `frame.k = capture width / layout width`.
- The rounding rule is `rnd = floor(v + 0.5)` at each edge (`to_image`).
  `pixel_under` (the pixel under a point) uses `floor`. `edge_pixel` picks
  the pixel inside a box corner, so the cursor pixel is always inside the
  crop.
- **View filters are cairo blend modes,** not per-pixel maths:
  - greyscale is a saturation blend with grey;
  - contrast is overlaying the picture on itself (an S-curve);
  - invert is a DIFFERENCE paint with white.

  The contrast view also flips dark backgrounds: the mean of a 40×20
  sample grid below 128. The OCR crop is greyscale, flipped the same way.
- **PNG encoding** uses screenie's own encoder (RGB, zlib level 1), about
  2× faster than cairo's. grim writes an uncompressed temporary PNG.

**Model**
- **`FilterOp.key()` includes `id(self)`.** Op identity is the Python
  object, not a stable ID; the port must not copy this
  (`architecture-rust.md` §7).
- **Select's history mixes plain rects with undo entries** (`LayerEntry`,
  `LayerDeleted`, `LayerMoved`, each with `.undo()`).
- The `.` ring is written only by `cli.py`, after delivery, for image,
  OCR and pin results.

**Performance, measured on two 1080p monitors at scale 1**

| What | Time |
|---|---|
| hotkey to overlay on screen | ~270 ms |
| of which Python start | 40 ms |
| of which imports | 10 ms |
| of which hyprctl | 27 ms |
| of which waiting for grim | 48 ms |
| of which GTK import | 94 ms |
| of which building the windows | 32 ms |
| of which the first draw | 12 ms |
| filter stack of 6, a held key | 11 ms per step |
| filter `+`/`-` | 4 ms |
| big pixelate censor | 4 ms |
| cold contrast view | ~61 ms |

## Where the prototype differs from README.md

The README is authoritative; the Rust port should follow it. These
differences are known, and are **not** fixed in the prototype, which is
frozen.

1. **Deleting from a list.** The README says the next surviving item is
   selected, else the previous one. The prototype's two lists disagree:
   - Filter selects the item *below* the deleted one (earlier in the
     stack), else the new bottom.
   - Layers keeps the same index, which selects the item *above* (later),
     else the new top.

   The README doesn't define which direction "next" is.
2. **Hiding a layer** (`Space` in Layers) isn't an undo step in the
   prototype. It's a view toggle. The README lists visibility changes in
   project history.
3. **`u` in Layers** undoes the last entry of Select's history, whatever
   it is. If the last thing done was a box change, that's what gets
   undone. The README says "the last layer-management action".
4. **OCR returns plain text** (tesseract stdout). The README asks for
   structured results internally (words, boxes, confidence), which is a
   port requirement.

## The previous, key-by-key README

Before the transition, `README.md` was a detailed key reference written
against this prototype. It's useful when a behaviour detail is missing
from the new README:

```
git show e7c7c5d:README.md
```

Where the two disagree, the current `README.md` wins.
