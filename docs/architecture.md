# Architecture

How screenie's code is built, for contributors. What the picker does is
specified in [picker.md](picker.md), and the command line in
[cli.md](cli.md); where this document and those disagree about
behaviour, they win.

## Goals and invariants

These properties are structural, not conventions to remember:

- The captured frame is immutable. Every edited view derives from it.
- A project is reproducible from the source pixels plus durable
  operations. Runtime state never affects the output.
- Keyboard and mouse drive the same semantic model. They reach the same
  operations and states, though not by identical event sequences.
- The interaction grammar is shared across modes; modes don't duplicate
  movement, list or text-entry behaviour.
- Undo records semantic actions, not input events.
- Render caches are disposable.
- Wayland and Hyprland details stay in the platform crate. Core never
  sees a compositor, a pixel buffer, a clipboard or an OCR engine.
- Project files hold only durable data.
- Picker behaviour is testable without a display server or a renderer.

Prefer ordinary data, enums and functions over trait hierarchies. Use
the type system to prevent invalid states and the mixing of concepts
(logical vs pixel coordinates, layer vs op IDs), not to encode every
runtime relationship.

## Workspace

The Rust workspace is in `rust/`:

```text
rust/
├── Cargo.toml          workspace: edition 2024, resolver 3, rust-version 1.86
└── crates/
    ├── screenie-core/      interaction logic and the durable model
    ├── screenie-image/     pixels, rendering, overlay drawing
    ├── screenie-platform/  Wayland, Hyprland, clipboard, pins, OCR, codes
    └── screenie/           the binary: CLI, config, delivery
```

Workspace lints deny `unsafe_code` everywhere and warn on all of clippy's
default groups (the gate turns warnings into errors). Nothing needs
`unsafe`: the clipboard and pins run as detached copies of screenie
instead of forking.

### The dependency rule

```text
screenie-core
    ↑
screenie-image
    ↑
screenie-platform
    ↑
screenie (depends on all three)
```

`screenie-core` never depends on platform or renderer code. Its only
dependencies are `serde`, `serde_json` (the durable model and the recall
ring) and `regex` (auto-redact's detectors, Measure's scale-bar reading).
Core does no I/O: the host passes in what it needs of the machine (time,
the layout, auto-redact's user, home directory and hostname) and
provides services for what needs pixels (see Host services).

## Crate boundaries

### screenie-core

Product logic with no display server, renderer, clipboard or OCR engine.
Everything here runs headlessly.

| Module | What it holds |
|---|---|
| `geometry` | logical and pixel types, directions, `PixelMap` |
| `layout` | `Layout`: monitors, visible windows, active window, focused monitor, bounds; the start box |
| `anchor_cursor` | the two-point box |
| `boxes` | the box grammar: hjkl and its modifiers, the box keys, drag to draw, mouse handles |
| `targets` | where Select's jumps land: window and monitor edges, quarter lines, neighbours, scope |
| `insert` | `PointCursor` and Insert |
| `lists` | the list grammar over stable IDs |
| `keys`, `intent`, `keymap`, `input` | chords, intents, keymap tables, held keys and repeat |
| `legend` | each mode's palette and `?` legend, as data |
| `tags` | where the status line, palettes, legends and lists go |
| `session` | `Session`, `Mode`, `Transition`, the open layer, warnings, host service traits |
| `history` | the undo logs |
| `model` | `Project`, `Layer`, `Op`, `ProjectFile` |
| `select`, `censor`, `annotate`, `filter`, `layers`, `lens`, `text`, `review`, `measure` | the modes |
| `ocr`, `redact`, `units` | OCR results as data, auto-redact's analysis, Measure's units and scale |
| `recall` | the recall ring as data |
| `script` | `--keys` scripts |

Each mode's module holds its state type and an `impl Session` block with
its intent handler (`censor_intent` and so on), so a mode can reach the
open layer and the undo logs only through the session's methods.

### screenie-image

Pixels and rendering. It knows how operations look, but not what any key
means.

| Module | What it holds |
|---|---|
| `image` | `Image`: 8-bit premultiplied RGBA, row-major, unpadded; crops, blits, damage |
| `frame` | `Frame`: the immutable capture plus its `PixelMap` |
| `compose` | per-monitor captures into one frame: format conversion, output transforms, scaling, placement; placing an opened image on a monitor |
| `decode`, `encode` | PNG, JPEG, WebP, PPM/PGM in; PNG (with text chunks), JPEG, lossless WebP, PPM out |
| `render` | every operation's rasterization, clipped to its layer |
| `picture` | `Picture`: the committed picture and the open layer's view, with their caches; the eyedropper, edge finder and OCR crops |
| `view` | view filters for the lenses |
| `ui` | the overlay for one monitor: frame, dimming, boxes, tags, lenses, measurements, suggestions; hit testing |

tiny-skia draws strokes, shapes, lines and highlights; fontdue
rasterizes glyphs, for text annotations and callouts (the host registers
the tags' font with `render::set_font`) and for the tags. Censors and
filters are screenie's own per-pixel code.

### screenie-platform

Operating-system and compositor integration, as narrow services that are
each optional. Nothing here interprets picker input: a platform
implementation never knows that Ctrl+h means a neighbouring window.

| Module | What it holds |
|---|---|
| `wayland` | one connection: capture, window capture, overlays, pin windows, xkb keys |
| `desktop` | the `DesktopInfo` trait and its Hyprland implementation |
| `clipboard` | the clipboard server, and reading the clipboard |
| `pins` | pin records and processes |
| `notify` | notifications through `notify-send` |
| `ocr`, `codes` | tesseract (TSV) and `zbarimg`, as external programs |
| `fonts` | finding a monospace font |
| `error` | `Error`: `Unavailable`, `Protocol`, `Io` |

### screenie

The binary assembles the other crates: the command line (`cli`), the
config file and profiles (`config`), `help` and completions (`help`),
regions (`region`), opened images and project files (`open`), the
interactive picker (`picker`), background OCR (`analysis`), output
(`deliver`), file locations (`files`) and `--json` reports (`report`).
`main` dispatches the verbs and maps failures to exit codes.

## Durable and runtime state

The durable state of an edited screenshot is:

```text
ProjectFile
    format              "screenie-project"
    layout              monitors, windows, scales, bounds at capture time
    project
        version
        selection       the box
        layers[]        bottom first
            id, clip, hidden
            ops[]       bottom first
                id, operation data
        next_id         the ID counter
        measure_scale   Measure's calibration, if set
+ the source capture's pixels
```

The project describes what exists now, not how it got there: resizing a
rectangle ten times serializes one rectangle.

Runtime only, and never needed to reproduce the image: the mode stack,
held keys and repeat, both undo logs, warnings, render and analysis
caches, job state, child-mode state, previews, in-progress geometry,
Measure's measurements, and auto-redact's suggestions.

## Stable IDs

Layers and operations carry stable IDs (`LayerId`, `OpId`), drawn from
one counter in the project that is saved with it, so IDs are never
reused across reopenings. Container indexes are never identity:
reordering changes position, not ID. Undo, list selection, editing an
existing op, serialization and cache keys all go by ID.

Monitors and windows have `MonitorId` and `WindowId`, stable for one
run. Recall anchors boxes to a monitor by its connector name instead,
which outlives the run.

## Coordinate spaces

Picker interaction happens in logical desktop coordinates
(`LogicalPoint`, `LogicalRect`: `f64`, possibly fractional). Image
operations happen in pixels (`PixelPoint`, `PixelRect`: `i32`).

The only conversion is `PixelMap`, in `screenie-core::geometry`. It
lives in core because Select needs it headlessly: the empty-box check (a
box under one output pixel doesn't capture), the size in output pixels on
the status line, and the cursor pixel. It is pure arithmetic.

### The rounding rule

```text
rnd(v) = floor(v + 0.5)
```

applied independently at every edge. `to_pixels` maps a rect,
`pixel_under` a point, and `edge_pixel` the pixel at a box corner that
lies inside the box. They share the rule, so a crop, its edge pixels and
the cursor pixel always agree, whatever the scale. No other code
multiplies by a scale.

The frame is one capture of the whole layout at the highest monitor
scale; `PixelMap` maps the layout's bounds onto it.

## Layout model

Core's `Layout` is platform-neutral:

```text
Layout
    monitors[]   id, connector name, logical rect, scale
    windows[]    id, logical rect, monitor, class, title; visible ones, topmost first
    active       the active window, if visible
    focused      the focused monitor
    bounds       the bounding box of every monitor
```

Types are named for what they are (`Window`), never for a compositor.
Hyprland's identifiers stay in the platform crate. The layout is saved
in the project file, so a reopened project knows the monitors it was
taken on.

## Interaction pipeline

```text
raw platform event      key press/release, modifiers, button, motion, wheel, focus lost
    ↓
physical input state    held keys, stale keys, repeat          (input)
    ↓
chord → intent          session keys first, then the top mode's keymap
    ↓
top mode                its intent handler
    ↓
shared capability       box, point cursor, list, text entry
    ↓
Transition              the session carries it out
```

The host (the overlay, a test, a `--keys` script) feeds
`Session::handle(RawEvent, now)` and `Session::tick(now)`, and reads the
outcome, the project, the boxes, the status line, palettes and legend
back. Physical input never mutates project data directly: a wheel notch
becomes `Adjust(+1)`, and the top mode decides what that adjusts.

### Raw input and repeat

Time is passed in as `f64` seconds on a monotonic clock, with every event
and tick. Core never reads a clock, so repeat is deterministic in tests.

screenie owns its repeat; the platform's key repeat never defines picker
acceleration.

- A key fires once per physical press. A platform autorepeat of a key
  already down is dropped.
- Held directions repeat after `DELAY` (0.08 s), ramping from the top
  mode's start rate to its top rate over `RAMP` (0.6 s). Steps are the
  exact integral of the ramp counted from the last tick, so the count
  doesn't depend on the frame rate. Every held direction counts: `k` and
  `l` together are diagonal; opposite keys cancel.
- Rates are per mode: the box modes 20 to 300 steps/s; Insert and
  Measure scaled so the cursor crosses the box in about a second; the
  loupe 15 to 200 px/s (with Shift, 4 to 12 window widths/s). Ctrl, Alt
  and Super never repeat (jumps and scaling are one-shot), and neither
  does Layers.
- Keys whose action repeats (`+`/`-` on an amount, typed text) follow the
  keyboard's own repeat rate and delay, which the compositor reports
  (default 25/s after 0.6 s).
- Keys held across a mode transition go stale: until released, they are
  not fresh presses in the new mode, and they stop repeating. This is
  enforced in `InputState`, below the mode stack.
- A direction pressed with Super held never repeats, even after Super is
  released.
- Losing keyboard focus forgets every held key.

### Chords

A `Chord` is a normalised key plus Shift, Ctrl, Alt and Super, and
keymaps match chords exactly:

- an upper-case letter is Shift plus its lower-case letter (`O` is
  `Shift+o`);
- a symbol or digit has its Shift consumed (`$`, `{`, `?`, `+`, `1`;
  layouts like AZERTY need Shift to type digits);
- a named key keeps Shift (`Shift+Space` is not `Space`).

A chord not in the mode's table does nothing, so a meaningless modifier
never falls through to the unmodified action. No table binds Super, so
any key with Super held does nothing.

### Intents and keymaps

`Intent` is the closed set of user meanings: `Steer`, `Translate`,
`Jump`, `Slide`, `Scale`, `Adjust(±1)`, `Choose(n)`, `CycleKind`,
`Confirm`, `Back`, `Undo`, `Quit`, the list intents, Measure's and text
entry's own, and the pointer's `Press`, `Motion` and `Release` in
logical coordinates. The pointer maps onto the same set: the left button
presses and drags, the right is `Confirm`, the middle `Back`, and the
wheel `Adjust` (Ctrl+wheel in Annotate bends the pen's arc).

Keymaps are data (`keymap.rs`): one chord-to-intent table per mode,
built from shared pieces (the direction grammar, the box keys, `+`/`-`,
the digits, the list keys) plus the mode's own bindings. A mode may
`unbind` a shared chord to give it its own meaning. Aliases (the arrows,
`=` for `+`) are bound but left out of the legend. `SESSION` holds the
keys every mode shares (`q`, `?`); the session handles them first.

The same tables drive dispatch, the `?` legend, the palettes,
`screenie help keys`, and the collision tests: no mode binds a chord
twice or shadows a session key, and a test fails if one does.

### Legends and palettes

`legend.rs` lists each mode's palette (always shown) and legend (on `?`)
as lines of items: a label and the intents it stands for. Rendering
looks up the keys for those intents in the mode's keymap, so the
on-screen text can't drift from what the keys do; an item whose intents
have no key is left out, and a test checks every listed intent is bound.
A palette item clicked with the mouse sends its first intent through
`Session::act`, exactly as its key would. Generate text from this data
rather than hand-writing strings, but only for consistency: this is not
a UI framework.

## Shared capabilities

Modes are built from a few shared interaction pieces. Behaviour belongs
in one if it can be described without mentioning what the mode is for.

### The box

`AnchorCursor` is the two-point box, like vim's visual-block mode. It is
stored as the rectangle plus which corner the cursor is on (the anchor
is the opposite one); this form makes crossing exact, including the
tie-break when the cursor lands on the anchor. It owns only universal
geometry: moving the cursor (crossing the anchor flips it), opposite and
sideways corner swaps, translation, halving and doubling, centre mode,
all kept inside its bounds. It knows nothing of windows, monitors,
layers or OCR.

### The box grammar

`boxes` maps the shared intents onto the box for every mode that has one
(Select, Censor, Filter and Shape): hjkl and its modifiers, `o`/`O`,
edge keys, drag to draw, and the mouse handles (an edge or corner within
`GRAB` = 6 px, inside or out; Alt inside the box moves it whole).

The grammar knows that Ctrl means "jump", not where to. The mode
resolves targets: Select's `targets` finds window and monitor edges and
neighbours; Ctrl+Alt+hjkl asks the host's `EdgeFinder` for the next edge
in the picture.

### The point cursor

`PointCursor` (in `insert`) is one position in frame pixels inside
bounds: steer, larger steps, jumps, placement. Insert and Measure use it,
and the loupe hands its position back to whichever opened it.

### Traces

The pen and highlighter build a path of points: a drag adds points as
the pointer moves; the keyboard pen (Space, then hjkl) adds them by
steps. Both produce the same `StrokeOp` or `HighlightOp`. Points closer
than half a pixel are dropped.

### Choices and adjustments

A choice is a finite set picked by key (`Choose(n)` for inks, filters,
layers; `CycleKind` for censor kinds and shapes). An adjustment is the
one value `+`/`-` and the wheel change (`Adjust(±1)`); the top mode
decides which value is current, with its own range and step. A choice
may change the current adjustment (a censor's kind switches between
block size and shade) or not (an ink leaves the width alone).

### Lists

`lists` is selection and order over stable IDs: select previous or next,
move the selected item, and keep the selection sensible after a delete
(the next row down as displayed, else the one above). What delete,
mute or hide mean belongs to the mode. Filter and Layers share it.

### Text input

`Mode::Text` is a temporary child that suspends command letters. While
it is on top, the session bypasses the keymaps and the session keys:
plain characters and Space are literal (even `q` and `?`), Backspace
deletes, Shift+Enter is a new line, and everything else (Enter, Esc,
arrows, chords with Ctrl or Alt) goes to its own table. Enter accepts,
Esc cancels. The child returns the text; its parent says what it means (a text annotation, a
callout number, a calibration length). For a text annotation it also
previews the text in place, and its Ctrl keys place and style it.

## Modes

Modes are a closed set, so they are one enum and every match is
exhaustive:

```rust
enum Mode {
    Censor, Annotate, Shape, Filter, Layers, Insert,
    Loupe, Enhance, Text, Redact, Measure,
}
```

Select is not in the enum: it is the base, always present below the
stack. Conceptually, Select is the base; Censor, Annotate and Filter are
build tools that open a layer; Layers, Insert and Measure are views over
the picture; Loupe, Enhance, Shape, Text and Redact are temporary
children that return to their exact parent.

### The mode stack

The session owns the stack. Only the top mode receives input; drawing
goes bottom to top. A mode never touches the stack: its handler returns
a `Transition`, which the session validates and carries out.

| Transition | What the session does |
|---|---|
| `Stay` | nothing |
| `OpenTool(mode)` | opens the layer if none is open (recording the stack depth), then pushes the tool |
| `SwitchTool(mode)` | back to the depth the layer was opened at, then the new tool |
| `Push(mode)` | a temporary child on top |
| `Pop` | back to the mode below |
| `PopWith(result)` | back to the mode below, handing it a `ChildResult` |
| `PopTo(depth)` | back to that depth (0 is Select) |
| `Finish(outcome)` | the run ends |

Every transition marks held keys stale. Leaving a mode ends any drag in
progress there, so its release goes nowhere.

`Place` says where a build tool opens: at the box's centre, centred on a
lens's point, or with a censor's top-left on Insert's cursor.

### Child results

Children return data, never their internal state:

| Result | From | To |
|---|---|---|
| `Position(PixelPoint)` | Loupe | Insert or Measure adopts it |
| `Text(String, entry)` | Text | Annotate (text, callout number) or Measure (calibration) |

Child state is speculative until confirmed; Esc returns nothing.

### Host services

What core can't do, the host provides as trait objects set on the
session. Core still never sees a pixel buffer.

| Trait | Used for | Provided by |
|---|---|---|
| `Sampler` | the eyedropper (`p` in Annotate) | `PictureSampler`, over what's shown |
| `CodeReader` | `Q` | zbar over the committed pixels of the box |
| `EdgeFinder` | picture edges, Measure's areas | `PictureEdges`: grey steps along a ray; a flood fill |
| `Analyzer` | OCR jobs (auto-redact, scale bars) | `analysis::Ocr`, inline or threaded |

Traits are for implementations that vary independently (these, and
`DesktopInfo`, `OverlayApp`). Closed sets owned by screenie (`Mode`,
`OpData`, `Intent`, `Transition`, `ChildResult`, undo records) are
enums. Don't add a trait because two types share three methods.

## The session

`Session` is one picker run. It owns everything that must survive tool
changes: the layout, the pixel map, the box, the project, the open layer
and its log, project history, the ink (colour and width, shared by the
pen and shapes and kept across layers), the lens view, the warnings, the
outcome and the host services. Modes never own durable layers.

It finishes with an `Outcome`: `Cancelled` (Esc or q, the user's
decision, not an error), or `Done` with a `FinishedAction`: `Image`,
`Ocr`, `Pin` (each with a rect), `Colour` (a pixel), `Code` (already
read) or `Measure` (text and JSON).

The binary's `deliver` turns it into output: the history copy and its
project, `last`, the `--save` copy, stdout, the clipboard and a
notification (what each option does is in cli.md). Core never touches
the clipboard, files or notifications.

## The open layer

The first build tool opens an implicit layer, owned by the session, not
the tool. Its clip is the box at that moment. Every tool adds to the same
layer, in order, until Enter commits it (one layer, if it holds anything)
or Esc discards it; both return to the depth it was opened from.

### Locked work and warnings

Warnings are session state (`Pending`), not per-mode logic:

- Esc in a build tool with locked work in the open layer sets
  `Pending::Discard`; a second Esc discards.
- `q` with locked work sets `Pending::Quit`; a second `q` quits.
- Any other input takes the warning back, and so does new work on the
  layer.

## Undo

Undo records semantic actions. A repeated `h`, each motion event and
each intermediate drag position are not entries. There are two logs,
and undo never falls through from one into the other.

### Open-layer history

Shared by every build tool in the open layer, so undo crosses tools:
censor, shape, filter, then `u` reverts the filter, removes the shape,
removes the censor. Entries are `LayerUndo` records, holding what is
needed to reverse the mutation (they don't belong in the project):

| Entry | Reverses |
|---|---|
| `Remove(id)` | an op added |
| `Restore { index, op }` | an op deleted |
| `Revert { id, data }` | an op changed (amount, rect, mute) |
| `Swap(a, b)` | two ops reordered |
| `RemoveMany(ids)` | several ops added as one action (accepted suggestions) |

With the log empty, `u` does nothing.

### Project history

Used from Select and Layers: selection changes, and layers committed,
deleted, moved, hidden or shown. Selection undo restores the rectangle
only; the cursor corner and centre mode are transient. `u` in Select
skips entries that would change nothing, so every `u` visibly does
something. `u` in Layers undoes the most recent layer entry, wherever it
is in the log, and leaves the box alone.

Project history starts afresh when a project is reopened.

### Coalescing

One physical interaction is one entry:

- the selection: one per tap, per held-key repeat (with its tap), per
  drag, per click, per scope step, per recall;
- Filter: a run of the same edit to the same filter (holding `+`,
  repeated taps, a drag) folds into one `Revert`;
- a mouse drag is naturally one transaction.

There are no time-based heuristics (for the wheel or anything else).

Measure keeps its own log (measurements added, deleted, calibrated,
excluded, swapped, edited), since its measurements aren't layer ops.

## The project model

Operations are plain serializable data (`OpData`), each with its ID:

| Op | Fields | Built with |
|---|---|---|
| `Censor` | rect, kind (pixelate or bar), amount (block size, or shade 0 black to 1 white) | box |
| `Stroke` | points, colour, width | trace |
| `Shape` | kind (rect or circle), rect, colour, width | box |
| `Line` | a, b, colour, width, heads (none, end, both) | box |
| `Highlight` | points, colour, width, alpha | trace |
| `Text` | origin, text (lines split on `\n`), colour, size, chip | text input |
| `Callout` | centre, number, colour, size | point |
| `Filter` | rect, kind (grey, invert, brightness, contrast, blur), amount, muted | box |

Each op has a conservative `bbox` of what it can change (text is bounded
as monospace, so no font is needed). A layer's box is its ops' boxes
within its clip. The boxes bound preview rendering and cache
invalidation, so an op whose bbox is too small leaves stale pixels. Ops
never own renderer objects.

### Serialization and versioning

The project serializes to JSON through serde: ops are tagged by `"op"`
with their fields flattened beside it. `model::VERSION` is the format
version (currently 1). Reading a project newer than the binary fails
with a clear error.

- A new field that older files lack is optional with a serde default
  (`hidden`, `muted`, `measure_scale` are), and needs no version bump.
- A breaking change bumps the version and migrates the old data model.
  Never make opening a project depend on replaying commands.
- Tests construct a project, serialize, deserialize, assert equality,
  render both and assert identical pixels. A test pins the JSON's shape.

### Project files

A project file is a PNG of the uncensored capture, with the
`ProjectFile` JSON (format tag, project, layout) in an iTXt chunk named
`screenie-project`. It is one self-contained file, and any image viewer
opens it as the plain capture. Projects are written private (0600) and
never overwrite. Each shot's project sits beside its history copy in
the runtime directory (`NAME.screenie` next to `NAME.png`; see FILES in
cli.md).

Reopening restores the layers, still editable with their IDs, only when
the current monitors match the saved layout's (names, rects, scales).
Otherwise the layers are drawn in and the image opens flat.

### The recall ring

`recall` holds the last 10 accepted boxes, newest first, each relative
to the monitor its centre was on (by name), so a box follows its monitor
if the layout is rearranged; with the monitor gone, the global rect is
used and clamped. The front end reads and writes
`$XDG_STATE_HOME/screenie/regions.json`, a JSON list of
`{rect, monitor?, offset?}`. Existing files depend on that shape: keep
it stable.

## Rendering

### Pixels

`Image` is 8-bit premultiplied RGBA, row-major, without row padding.
Capture backends and decoders convert into it once, so the rest of the
crate doesn't care whether pixels came from screencopy, a window export
or a file. Opened images are flattened over a background (screenie's
pictures are screens); one read in may have at most 2^28 pixels.

### The pipeline

```text
Frame (the capture, immutable)
    ↓ committed layers, in order (hidden ones skipped)
committed picture                  output comes from here
    ↓ open layer over its clip (locked ops, then the item being edited)
shown picture
    ↓ view filters (lenses only)
    ↓ UI: dimming, boxes, cursors, tags, lists, lens frames
overlay buffer
```

Only layers affect the output. View filters and UI are display-only and
never reach the project or the output; turning a mark into output means
making an operation.

Rendering lives only in `screenie-image::render`. Core defines operations
as data and never sees a renderer type.

### Layer clipping

A layer never touches pixels outside its clip, and never reads them
either: a blur or a pixelate cell stops at the clip's edge. So a layer
looks the same rendered over a patch of just its clip (while it is open)
as over the whole frame (once committed). Operation geometry may extend
past the clip, and pen points may lie outside it; the clip still holds.

Rendering works on a `Patch`: an image plus its origin in frame pixels.
Pixelate blocks are exact means of the pixels under each block, aligned
to the censor's rect. Blur is a three-pass box blur with running sums in
fixed point. A highlighter is multiplied at its alpha, so it marks
without hiding.

### Render caches

Caches are disposable accelerators, never authoritative. Throwing every
one away changes nothing: `Picture::sync` rebuilds what's needed from
the capture and the layers.

- **Committed picture.** Rebuilt from the capture when the committed
  layers change. Its `version` bumps, which keys analysis caches.
- **Open-layer prefixes.** The clip patch after the first n locked ops,
  for the last `PREFIXES` (12) prefixes still true of the layer. Changing
  the top filter's amount re-runs only that filter, not the ones under it.
- **Shown patch.** The locked patch plus the item being edited. A pen
  trace that only grew draws just its new tail; a highlighter trace keeps
  its coverage and grows it. The full trace is rendered exactly once, when
  it locks.
- **Isolated layer.** The layer the Layers view inspects, alone over the
  capture in its clip.
- `generation` bumps whenever what's shown changes.

The picker marks the picture stale on input and syncs once per frame, so
a fast mouse costs one sync a frame, not one an event.

### Lenses

View filters (greyscale, invert, contrast) consume the shown picture and
produce display-only pixels. The loupe is a small crop scaled
nearest-neighbour with a pixel grid; Enhance is a lens crop, scaled
sharp or smooth, with an optional view filter. Both work on crops, never
a full-frame copy per movement.

## Overlay and UI

`ui::View::of(&session)` collects what to draw as plain data: the box,
other boxes, status, palette, legend, lists, lenses, measurements,
suggestions. `Ui::draw` paints one monitor into a buffer at that
monitor's device scale: the frozen picture dimmed outside the box, box
outlines and cursor marker, and tags as chips with a monospace font.
`tags` (in core) decides where each block goes; `ui` only draws it.

`Ui` records where it drew each chip and palette item. The picker gives
a left press on a chip to the chip (`Ui::hit`): an item does what its
key would (`Session::act`), and the picture under a chip never sees the
press, drag or release. Box handles are core (`boxes::Handle`,
`Session::pointer_handle`); the picker turns them into a cursor shape.

Frames are diffed: `Image::damage` compares the new frame with the last
in 64 px tiles (at most 32 rectangles, else their bounds). A frame that
didn't change isn't committed; a changed one damages and copies only
its tiles.

## Platform services

There is no single platform backend. Each service is narrow, separately
optional, and fails with a precise error when missing (`Unavailable`:
no Hyprland, no screencopy, no layer-shell).

### Capture

One Wayland connection (smithay-client-toolkit) serves capture,
overlays and pins.

- The frame: wlr-screencopy of every output, composed at the highest
  scale into one image of the layout. Output transforms are applied
  after capture.
- A window's own pixels, even covered or on another workspace:
  ext-image-copy-capture with an ext-foreign-toplevel-list source,
  identified by Hyprland's `stableId`.
- A capture waits at most 2 s. A monitor that sends no frame by then
  (asleep) is left blank; the capture fails only when no monitor sent
  one, and the error names the outputs it waited on.
- `--cursor` asks screencopy to include the pointer.

### Desktop information

`DesktopInfo::layout()` produces core's `Layout`. The Hyprland
implementation reads monitors, clients and the active window over its
IPC socket (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`),
converts monitor sizes from device to logical pixels, and keeps only
windows you can see, topmost first. Select never calls Hyprland.

### Overlay host

`Wayland::run_overlay` shows one layer-shell surface per monitor and
drives an `OverlayApp` (the binary's `Picker`), which it knows only as:
raw input in, `tick`, `wants_ticks`, `finished`, `draw(monitor)`, and a
pointer shape. It never interprets screenie commands.

- Every overlay is keyboard-exclusive: while an exclusive layer is up,
  Hyprland routes the pointer only to exclusive layers. The focused
  monitor's overlay is created last, so it gets the keyboard.
- Overlays are drawn into shm at the output's integer buffer scale. On
  a fractional output that is the next integer up, and the compositor
  scales it down.
- Pointer shapes go through wp-cursor-shape.
- Continuous scrolling becomes wheel steps every 15 logical px.

### Event loop

Interaction is single-threaded:

```text
Wayland events, repeat ticks, job results
    ↓
OverlayApp::input / tick      → Session
    ↓
draw due surfaces             → Picture::sync, Ui::draw, damage
```

While a key is held or work is pending, the loop ticks every 8 ms;
otherwise it sleeps until an event. It ends when the session finishes,
the compositor closes an overlay, or `SCREENIE_DEADLINE` passes (a
safety net for development runs), and takes the overlays down before
returning. There is no async runtime.

### Startup order

The picture must be of the moment screenie started, so capture starts
before the layout query and before any overlay exists:

```text
connect → start screencopy → query the layout → finish capture → overlays
```

### Clipboard

Wayland's clipboard lives only as long as the client offering it, so
screenie starts a detached copy of itself (`__serve-clipboard`, using
wl-clipboard-rs) that serves the selection until something else is
copied. The parent hands it the offers on stdin and waits until it has
taken the selection. An image is offered in the chosen format, as PNG,
as `application/x-screenie-shot` (the history copy's path, so a screenie
shot on the clipboard reopens with its project), and as
`text/uri-list` for a saved file.

### Pins

A pin is a detached process, `screenie __serve-pin ID PATH SCALE [X,Y]`,
showing the image in an xdg toplevel (app id `screenie-pin`, no
decorations, fixed size, drawn at the capture's scale). A window rule
floats and pins it; once Hyprland lists the window, the process moves it
to X,Y with Hyprland's dispatcher: `hl.dsp.window.move` (Lua dispatch,
Hyprland 0.56 and later), else `movewindowpixel exact X Y,pid:PID` for
older releases. A left drag moves it; Esc, `q` or the
middle button closes it.

Records are `$XDG_RUNTIME_DIR/screenie/pins/ID.json` (id, pid, image
path, position, size). `pin list` drops records whose process is no
longer a pin server; `pin close` sends SIGTERM, and the server removes
its record and image. One process per pin keeps a crash to one pin and
needs no daemon.

### Notifications, OCR and codes

`notify-send`, tesseract (TSV output, so words keep boxes and
confidences) and `zbarimg` are external programs, each optional.
Notifications are best effort: without `notify-send` they are silent.
Without tesseract or zbarimg, the action that needs it fails (exit 4)
and everything else works; in the picker, `Q` stays open with a note.

## Background jobs

Expensive analysis runs off the input path through the session's
`Analyzer`:

- the session gives each job an id (`JobKind::Ocr { rect, codes }`, a
  pixel rect of the committed picture);
- the analyzer answers at once (`--keys`, `--no-ui`, `--redact auto`,
  which need the answer now) or later through `Session::job_done`, from
  a thread whose results the picker collects on ticks;
- a result whose id no mode is waiting for is dropped. That covers
  rescans and leaving the review.

Workers know nothing of modes. Jobs carry no frame version, because the
frame never changes within a session; the analysis cache keys on the
committed picture's version instead.

### The analysis cache

`analysis` keeps the last 8 scans, keyed by the committed picture's
version, the rect, and whether codes were read; a scan inside a kept or
running one reuses it. The rules, and `[redact] prescan`, are in
[auto-redact.md](auto-redact.md#speed-and-caching).

## OCR

OCR returns structured data (`core::ocr`): the full text, and each word
with its box, confidence and line. The binary crops the committed
picture for it: greyscale, dark backgrounds inverted (tesseract wants
dark text on light), and doubled when the frame's scale is under 1.5.
Word boxes are mapped back to frame pixels. OCR output, auto-redact and
Measure's scale-bar reading share it.

## Auto-redact

```text
OCR words → lines, each character with a box → detectors → spans
codes (zbar) ──────────────────────────────────┘
spans → union of character boxes → pad → merge → Proposal[]
Proposal[] → review (Mode::Redact) → accepted → ordinary CensorOps
```

`redact` is pure analysis. Its configuration comes in as `Rules` (from
`[redact]`) and the machine as `Env` (user, home, hostname). Proposals
are never project data; accepting adds censors to the open layer as one
undo step (`RemoveMany`). The session remembers which ops came from
suggestions, so `--json` reports only the ones still there. The
pipeline, detectors and review are in [auto-redact.md](auto-redact.md).

## Measure

Measure is a mode over the frozen picture, not a build tool: it opens no
layer, and its measurements are mode state, drawn in the overlay and
dropped on leaving. It reuses the shared pieces and keeps pixels out of
core:

```text
points        PointCursor, on capture pixels
paths, arcs   polylines of dropped points
calibration   the Text child, parsed by `units`
edges, areas  the host's EdgeFinder: an edge along a ray; a flooded region
scale bar     an Analyzer job (OCR around the line) filling the prompt
```

Values and errors are computed from points and the scale when shown,
never stored. The scale (pixels, real length, unit, error) lives on the
session and is saved in the project (`measure_scale`), so reopening
keeps it; measurements are not saved. Behaviour is in
[measure.md](measure.md).

## Error handling

Failures are classes, not strings:

| Class | Exit |
|---|---|
| user cancellation (Esc, `q`) | 1, not an error |
| usage or config error | 2 |
| nothing to capture, or capture unavailable | 3 |
| delivery failure (clipboard, file, OCR, a missing tool) | 4 |

The platform crate reports `Unavailable`, `Protocol` or `Io`; `main`
maps every failure to an exit code in one place. The exact cases are in
cli.md (EXIT STATUS).

## Testing

Most behaviour is tested in `screenie-core`, headlessly, by driving a
`Session` with `--keys` scripts or raw events (where timing matters),
then asserting on the box, the project, the stack, the status line or
the outcome. Core tests cover anchor crossing and corner swaps, modifier
semantics, selection scopes and window jumps, bounds and clipping, key
repeat and keys held across mode changes, mode-stack transitions and
child results, the open layer's lifecycle, undo scopes, locked-work
warnings, list selection and reordering, serialization round trips, key
collisions and logical/pixel rounding. The behaviour tests are the
executable form of [picker.md](picker.md).

`screenie-image` tests cover op rendering, layer order, clipping,
filters, cache equivalence (a cached picture equals one rebuilt from
scratch), view/output separation, a serialized project rendering the
same pixels, and the overlay drawn headlessly. `screenie-platform` tests
stay narrow, on the adapters' parsing and conversion. The binary's tests
run it on its headless paths. [CONTRIBUTING.md](../CONTRIBUTING.md) says
where each lives and how to run them.

screenie began as a Python prototype, and the Rust code was tested
against its behaviour; the prototype is in the git history.

### --keys

`--keys` drives the same session as real input, not a separate
automation engine: `Session::play` parses the script and feeds the same
`RawEvent`s a platform would (modifiers on fixed keycodes, a fresh code
per key), all at one instant, so nothing repeats. Tests, macros,
debugging and reproducible bug reports all use it. The syntax is in
cli.md.

## Performance

Optimize measured bottlenecks. Don't add a daemon, renderer threads, GPU
rendering, damage graphs or an async runtime without measurements that
need them. Keep what is known to work: the immutable capture, prefix
caches, local operation bounds, crop-local lenses, one sync per frame
and tile damage.

The ignored test `picker::tests::frame_costs` measures a frame's costs
on two 1920x1080 monitors:

```
cargo test --release -p screenie frame_costs -- --ignored --nocapture
```

## Design rule for new features

Before adding an interaction concept, ask whether the feature is an
existing capability plus a new operation or analysis result:

```text
rectangle, circle  = box + ShapeOp
line, arrow        = box + LineOp
pen                = trace + StrokeOp
highlighter        = trace + HighlightOp
text               = point + text input + TextOp
callout            = point + CalloutOp (+ text input for its number)
measure            = point cursor + paths + analysis
auto-redact        = analysis + review + CensorOp
```

Prefer reuse. Add a new capability only when no existing one describes
the behaviour cleanly. Any new keys must pass the collision tests, and
the legend and palette come from the keymap.

Changes to persistent data or to crate boundaries update this document.
The rest of the development rule is in
[CONTRIBUTING.md](../CONTRIBUTING.md).
