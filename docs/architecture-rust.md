# Rust architecture

This document defines the architecture of the Rust rewrite of screenie.

It is subordinate to `README.md` for user-visible behavior.

If this document and `README.md` disagree about how the picker behaves, `README.md` wins.

The goal of the Rust implementation is not to port the Python modules one-for-one. The Python prototype exists to settle UX. The Rust implementation should preserve that behavior while using a model that is easier to test, serialize, extend and port across display systems.

---

# 1. Architectural goals

The implementation should make the following properties structural rather than conventional:

- the captured frame is immutable;
- project state is reproducible from source pixels plus durable operations;
- keyboard and mouse manipulate the same semantic model;
- interaction grammar is shared across modes;
- modes do not duplicate common movement/list behavior;
- undo operates on semantic actions, not raw input events;
- rendering caches are disposable;
- Wayland/Hyprland details do not leak into the interaction core;
- project files do not serialize arbitrary runtime state;
- core behavior can be tested without Wayland, X11 or a renderer;
- future X11 support should not require changing picker behavior.

The Rust implementation should prefer ordinary data structures, enums and functions over elaborate trait hierarchies.

Use Rust's type system primarily to prevent invalid state and accidental mixing of concepts, not to encode every runtime relationship at compile time.

---

# 2. Repository structure

The Rust implementation lives in the existing screenie repository alongside the Python behavioral reference.

Recommended workspace:

```text
screenie/
├── README.md
├── docs/
│   ├── architecture-rust.md
│   ├── cli.md
│   ├── auto-redact.md
│   ├── measure.md
│   └── annotate.md
├── src/screenie/              # Python prototype
├── tests/                     # Python behavioral reference
└── rust/
    ├── Cargo.toml
    └── crates/
        ├── screenie-core/
        ├── screenie-image/
        ├── screenie-platform/
        └── screenie/
```

The implementation should be developed on a separate branch/worktree until it is suitable for daily use.

The Python prototype remains available during the port for:

- behavioral comparison;
- test reference;
- UX validation;
- fallback daily use.

---

# 3. Crate boundaries

## 3.1 `screenie-core`

Contains product logic that does not require a display server or image-processing backend.

Expected areas:

```text
geometry/
interaction/
input/
modes/
model/
history/
session/
layout/
action/
```

This crate contains:

- logical coordinates;
- selection geometry;
- anchor/cursor behavior;
- point interaction;
- list interaction;
- choices and adjustable values;
- semantic input intents;
- keymap tables;
- mode state;
- mode stack;
- project model;
- layers and operations as plain data;
- undo records;
- project/session transitions;
- platform-neutral monitor/window layout;
- output actions.

It must not depend on:

- Wayland;
- X11;
- Hyprland IPC;
- rendering libraries;
- clipboard libraries;
- OCR engines.

A CI machine with no graphical environment should be able to run nearly all `screenie-core` tests.

---

## 3.2 `screenie-image`

Contains pixel manipulation and rendering.

Depends on `screenie-core`.

Contains:

- pixel buffers;
- logical-to-pixel mapping;
- project/layer rendering;
- operation rasterization;
- crops;
- filters;
- view-only transforms;
- committed-layer caches;
- open-layer prefix caches;
- image encoding.

It must not contain product input semantics.

For example:

```text
CensorOp
```

is defined as durable data in core.

The image crate knows how to render it.

The project model should not contain renderer-specific objects.

---

## 3.3 `screenie-platform`

Contains operating-system, compositor and display-server integration.

May depend on `screenie-core` and `screenie-image`.

Expected areas:

```text
capture/
desktop/
overlay/
clipboard/
notifications/
pins/
jobs/
```

Initial implementations:

```text
capture/wayland
capture/hyprland_window
desktop/hyprland
overlay/wayland
clipboard/wayland
```

Future implementations may add:

```text
capture/x11
desktop/x11
overlay/x11
```

No platform implementation should directly contain picker behavior such as:

```text
Ctrl+h means neighboring window
```

It should only provide the raw environmental information needed by core.

---

## 3.4 `screenie`

The executable/front end.

Contains:

- CLI;
- config;
- profiles;
- project I/O;
- history commands;
- output delivery;
- service construction;
- top-level command dispatch.

Expected commands follow `docs/cli.md`:

```text
pick
shot
edit
history
pin
help
completions
```

This crate assembles the other crates.

---

# 4. Dependency direction

The important rule is:

```text
core
  ↑
image
  ↑
platform

executable depends on all three
```

The exact Cargo graph may vary where practical, but one rule is strict:

> `screenie-core` must never depend on platform or renderer code.

---

# 5. Canonical durable state

The durable state of an editable screenshot is:

```text
Project
    source capture
    capture/layout metadata
    selection
    ordered layers
        stable layer ID
        clip
        visibility
        ordered operations
            stable op ID
            operation data
    Measure's scale, if calibrated (px, real, unit name, error)
```

The project model describes what exists now.

It is not an append-only history of every action used to create it.

For example, repeatedly resizing the same rectangle should serialize one final rectangle, not every intermediate resize command.

The project model should be sufficient to reproduce the edited image.

---

# 6. Non-canonical runtime state

The following are runtime-only:

- mode stack;
- currently held keys;
- key repeat state;
- undo history;
- temporary warnings;
- render caches;
- async job state;
- temporary child-mode state;
- active previews;
- in-progress unconfirmed geometry.

They may be preserved separately as optional convenience state for `last`, but they are not required for image reproducibility.

A project must remain valid without them.

---

# 7. Stable IDs

Layers and operations use stable IDs from the start.

Conceptually:

```rust
struct LayerId(...);
struct OpId(...);
struct WindowId(...);
struct MonitorId(...);
```

Avoid using container indexes as identity.

Reordering an item changes position, not identity.

Stable IDs are required for:

- undo;
- list selection;
- serialization;
- editing existing operations;
- project migration;
- deterministic cache keys.

---

# 8. Coordinate spaces

The implementation distinguishes logical desktop coordinates from image/device pixels.

Conceptually:

```rust
struct LogicalPoint { ... }
struct LogicalRect  { ... }

struct PixelPoint { ... }
struct PixelRect  { ... }
```

These should be distinct Rust types.

Picker interaction occurs in logical coordinates.

Image operations occur in pixel coordinates.

Conversion happens through one authoritative mapping object.

The settled edge conversion rule is:

```text
rnd(v) = floor(v + 0.5)
```

applied independently at each edge.

No subsystem should independently perform its own `x * scale` conversion.

---

# 9. Layout model

Core receives a platform-neutral desktop model:

```text
Layout
    monitors[]
    windows[]
    active_window
    desktop_bounds
```

A monitor contains logical geometry and enough identity to preserve recall.

A window contains:

- stable runtime identity;
- logical rectangle;
- associated monitor where useful;
- visibility/state metadata required by picker behavior.

Core types should not be named after a specific compositor.

For example:

```text
Window
```

not:

```text
HyprlandWindow
```

Hyprland-specific identifiers remain in the platform implementation or opaque ID metadata.

---

# 10. Interaction pipeline

Input flows through distinct levels:

```text
raw platform input
    ↓
physical input state
    ↓
keymap / pointer mapping
    ↓
semantic Intent
    ↓
top Mode
    ↓
common interaction capability
    ↓
Command / Transition / ChildResult
    ↓
Session
```

Physical inputs should never directly mutate project properties.

Example:

```text
mouse wheel
    ↓
Adjust(+1)
    ↓
active mode exposes current Adjustment
    ↓
value changes
```

---

# 11. Raw input and repeat

Platform input contains only physical events:

- key press;
- key release;
- pointer press;
- pointer release;
- pointer motion;
- wheel movement.

screenie owns its repeat behavior.

The platform repeat configuration must not define picker acceleration semantics.

The input subsystem tracks:

```text
held keys
held directions
modifier state
repeat phase
```

Keys held across a mode transition do not become fresh presses in the new mode.

This rule is enforced below the mode stack.

---

# 12. Semantic intents

The exact enum may evolve, but intents should represent user meaning rather than physical keys.

Examples:

```text
Steer(Direction)
Translate(Direction)
Jump(Direction)
Slide(Direction)
Scale(Direction, ScaleKind)
Adjust(i32)
Choose(ChoiceId)
Confirm
Back
Undo
Quit
Begin(Point)
Update(Point)
End(Point)
```

The keymap maps physical input to intents.

Mouse input maps to the same semantic layer where appropriate.

Keyboard and mouse parity means:

> both can reach the same user-level operations and model states.

They do not need identical event sequences.

---

# 13. Anchor/cursor geometry

The fundamental two-point geometry is:

```text
AnchorCursor
    anchor
    cursor
    bounds
```

It owns only universally true geometry behavior:

- move cursor;
- translate pair;
- crossing through anchor;
- opposite-corner swap;
- sideways-corner swap;
- scaling;
- derived rectangle;
- derived line.

It does not know about:

- windows;
- monitors;
- filters;
- censors;
- Hyprland;
- OCR;
- selection scope;
- layer semantics.

The criterion for common geometry is:

> can the behavior be described without mentioning the purpose of the mode?

If yes, it may belong here.

---

# 14. Box grammar

`BoxGrammar` maps shared geometric intents onto `AnchorCursor`.

Examples:

```text
Steer(left)        → cursor left
Translate(left)    → move anchor and cursor
Scale(left, half)  → halve toward left
SwapOpposite       → opposite corner
```

The grammar knows that Ctrl means a jump request, but does not know where the next meaningful target is.

Mode-specific code resolves targets.

Example:

```text
Select
Ctrl+h
→ Jump(Left)
→ neighboring window/layout target
```

Measure may later resolve the same intent as:

```text
Ctrl+h
→ Jump(Left)
→ detected picture edge
```

Same grammar, different environment.

---

# 15. Point capability

A point-like capability represents one active position inside bounds.

Used by:

- Insert;
- Loupe;
- future keyboard pen cursor;
- callout placement;
- text placement;
- other point-driven children.

It supports concepts such as:

```text
steer
jump
place
larger step
```

Mode-specific code defines meaningful jump targets.

---

# 16. Trace capability

Trace represents a continuous path.

Conceptually:

```text
Trace
    points
    drawing state
```

Semantic operations:

```text
begin
extend
end
```

Mouse:

```text
press
motion
release
```

Keyboard may eventually use:

```text
point cursor
begin drawing
steer
finish
```

Both produce the same path representation.

This supports:

- freehand pen;
- highlighter;
- measurement paths.

---

# 17. Choice

A `Choice` is a finite set of selectable options.

Examples:

- censor kind;
- annotation colour;
- shape kind;
- filter kind.

A choice contains:

```text
option ID
binding
display label
selected option
```

One declaration should drive:

- key dispatch;
- palette display;
- collision validation;
- generated reference/help data where possible;
- tests.

Do not separately hand-maintain a visible palette and keyboard logic.

---

# 18. Adjustable

At any point a mode exposes zero or one current adjustable value.

Examples:

- scope;
- cursor step;
- width;
- censor block size;
- bar shade;
- filter amount;
- zoom;
- loupe size;
- selected layer.

An adjustment contains its own:

```text
minimum
maximum
step
display formatting
```

The common input system emits:

```text
Adjust(+1)
Adjust(-1)
```

The active mode resolves which value is current.

A `Choice` may change the active adjustable, but does not inherently do so.

Examples:

- changing Censor kind changes block-size vs shade adjustment;
- changing annotation colour does not change width adjustment;
- changing selected Filter row changes which filter amount is adjusted.

---

# 19. Ordered lists

The common list data structure contains:

```text
ordered stable IDs
selected stable ID
```

It owns:

- select previous/next;
- move selected earlier/later;
- selection validity after structural changes.

It does not own:

- delete semantics;
- visibility semantics;
- duplication semantics;
- undo;
- filter amounts;
- layer rendering.

`ListGrammar` maps common navigation/reorder intents onto this structure.

Filter and Layers share the grammar.

Mode-specific behavior determines what actions such as mute/hide/delete actually mean.

---

# 20. Text input

Text input is a temporary mode that suspends normal command-letter interpretation.

While active:

```text
letters → literal text
digits  → literal text
Space   → literal text
Enter   → accept
Esc     → cancel
```

The child returns text.

It does not interpret that text.

Examples:

```text
Measure calibration
    TextInput → "5 m"
    Measure parses units

Text annotation
    TextInput → "hello"
    Annotate creates TextOp

Callout
    TextInput → "12"
    Callout parses number
```

One shared text-entry component should support all of these.

---

# 21. Mode representation

Modes form a closed product-defined set and should normally use a Rust enum rather than a trait-object hierarchy.

Conceptually:

```rust
enum Mode {
    Select(SelectMode),
    Insert(InsertMode),
    Loupe(LoupeMode),
    Enhance(EnhanceMode),
    Censor(CensorMode),
    Annotate(AnnotateMode),
    Shape(ShapeMode),
    Filter(FilterMode),
    Layers(LayersMode),
    Text(TextMode),
    Measure(MeasureMode),
}
```

This allows exhaustive matching when new modes are added.

Use traits where implementations are genuinely open-ended, especially platform services.

---

# 22. Mode stack

The session owns an explicit runtime stack.

Invariant:

```text
stack is never empty
stack[0] == Select
```

Only the top mode receives input.

Drawing happens bottom-to-top.

A mode does not directly manipulate the stack.

It returns a transition request.

Examples:

```text
Stay
Push(mode)
Pop(result)
Replace(mode)
Finish(action)
Quit
```

The session validates and performs the transition.

---

# 23. Mode categories

These categories are conceptual rather than necessarily encoded in the type system.

## Base

```text
Select
```

Stable bottom mode.

## Tools

Examples:

```text
Censor
Annotate
Filter
Layers
Insert
```

Represent larger interaction contexts.

## Temporary children

Examples:

```text
Loupe
Enhance
Shape
Text
```

Return control to their exact parent.

---

# 24. Child results

Temporary children return explicit data rather than exposing their internal object to the parent.

Conceptually:

```text
ChildResult
    None
    Position(LogicalPoint)
    Text(String)
    ShapeSpec(...)
    Color(...)
    LineSpec(...)
```

Examples:

```text
Loupe → Position
Text → Text
Shape → ShapeSpec
```

The parent interprets the result.

Temporary child state is speculative until confirmed.

`Esc` returns no committed value.

---

# 25. Session ownership

The session owns shared editing state that must survive tool changes.

Conceptually:

```text
Session
    project
    mode_stack
    open_layer
    project_history
    open_layer_history
    pending_confirmation
    runtime input state
```

Modes must not own durable project layers directly.

---

# 26. Open layer

The first build tool opens an implicit layer.

That layer belongs to `Session`, not the tool.

This allows:

```text
Censor
    add operation
Annotate
    add operation
Filter
    add operation
Enter
    commit one layer
```

All operations remain ordered.

The layer retains its clip from the selection that created it.

---

# 27. Locked work and warnings

Discard and quit warnings are session-level state.

Conceptually:

```text
PendingConfirmation
    None
    DiscardOpenLayer
    QuitWithOpenLayer
```

If locked open-layer work exists:

```text
Esc
→ warning

Esc again
→ discard
```

Any unrelated key clears the warning.

The same mechanism applies to `q`.

Modes should not implement separate copies of this logic.

---

# 28. Undo principles

Undo records semantic actions, not input events.

Examples that should not individually become history entries:

- every repeated `h`;
- every mouse-motion event;
- every intermediate drag position.

Examples that may become semantic undo entries:

- lock censor;
- add filter;
- reorder filter;
- delete filter;
- change amount;
- commit layer;
- delete layer;
- move layer;
- change selection.

---

# 29. Undo scopes

There are two principal histories.

## Open-layer history

Used while build tools are active.

Shared by:

- Censor;
- Annotate;
- Shape;
- Filter;
- other build operations.

Undo may cross tool boundaries inside the same open layer.

Example:

```text
censor
shape
filter

u → revert filter
u → remove shape
u → remove censor
```

## Project history

Used from Select/project-management contexts.

May contain:

- selection changes;
- committed layer addition;
- layer deletion;
- layer movement;
- layer visibility changes.

Undo never automatically falls through from an empty open-layer history into project history.

---

# 30. Commands and undo records

Commands and undo records do not need to be identical types.

Conceptually:

```text
execute(Command, Model)
    → UndoRecord
```

For example:

```text
DeleteLayer(id)
```

may return:

```text
UndoDeleteLayer
    complete layer data
    previous index
```

Undo records may contain whatever runtime state is needed to reverse the actual mutation.

They do not belong in the project file.

---

# 31. Coalescing

Repeated adjustments belonging to one physical interaction should coalesce where practical.

Example:

```text
press +
repeat
repeat
repeat
release

history:
ChangeAmount { before: 1.0, after: 1.8 }
```

Mouse drag is naturally one interaction transaction.

Avoid introducing time-based wheel heuristics unless measurements or UX show they are necessary.

---

# 32. Project operations

Operations are plain serializable data.

Examples:

```text
CensorOp
StrokeOp
ShapeOp
TextOp
FilterOp
LineOp
HighlightOp
CalloutOp
```

Each operation has:

- stable ID;
- operation-specific fields;
- enough information to reproduce output.

Operations should not own renderer objects.

---

# 33. Serialization

Project serialization must exist early in the rewrite.

A project has an explicit format version.

Conceptually:

```text
Project
    version
    source metadata
    layout metadata
    selection
    layers
```

Tests should verify:

```text
construct
serialize
deserialize
assert model equivalence
render both
assert identical pixels
```

Project migration should operate over durable data models.

Do not make opening an old project depend on replaying a historical command log.

---

# 34. Captured frame

The captured image is immutable.

Imaging derives all edited views from:

```text
CapturedFrame
    +
Project layers
    +
OpenLayer preview
```

The source pixels are never destructively edited.

---

# 35. Pixel representation

The image subsystem should choose one canonical internal pixel representation.

A likely initial representation is:

```text
8-bit premultiplied RGBA
```

Platform capture backends convert into it once.

The rest of imaging should not care whether pixels came from:

- Wayland screencopy;
- Hyprland toplevel export;
- X11;
- file input;
- stdin.

---

# 36. Rendering pipeline

Conceptually:

```text
CapturedFrame
    ↓
committed Project layers
    ↓
OpenLayer preview
    ↓
view-only transform
    ↓
UI overlays
```

Only project/open-layer operations affect output.

View transforms are display-only.

UI overlays such as:

- boxes;
- cursors;
- tags;
- lists;
- loupe frames

are never rendered into project output unless explicitly converted into an annotation operation.

---

# 37. Renderer separation

Project operations are data.

Rendering code lives in `screenie-image`.

Conceptually:

```text
render(CensorOp)
render(ShapeOp)
render(FilterOp)
```

Do not expose a renderer-specific surface type through core APIs.

For example, core should not return or contain:

```text
CairoSurface
SkiaPixmap
```

---

# 38. Bounding boxes

Every operation should have a conservative affected bounding box.

Used for:

- preview rendering;
- cache invalidation;
- future damage tracking.

Where needed, imaging may distinguish:

```text
output_bbox
input_dependency_bbox
```

For example, blur may read pixels beyond the exact output region.

Do not require full damage precision in the first implementation, but preserve the ability to add it.

---

# 39. Layer clipping

Layer clipping is enforced in imaging.

An operation may geometrically extend beyond the clip, but pixels outside the clip are not affected.

This is distinct from interaction bounds.

For example, a pen path may contain points outside the clip while rendering still clips output correctly.

---

# 40. Render caches

Caches are disposable accelerators.

They are never authoritative state.

Deleting every render cache must leave project behavior unchanged.

Two important cache classes:

```text
CommittedRenderCache
OpenLayerRenderCache
```

---

# 41. Committed-layer cache

A simple prefix model is sufficient initially.

Example:

```text
base
→ layer A
cache A
→ layer B
cache AB
→ layer C
cache ABC
```

If B changes:

```text
reuse A
rebuild B
rebuild C
```

Exact storage strategy may change after measurement.

---

# 42. Open-layer prefix cache

Open-layer interactive editing benefits strongly from cached prefixes.

Example:

```text
committed base for clip
→ censor
prefix 0
→ shape
prefix 1
→ current contrast preview
```

Changing only contrast reuses the prefix.

This preserves an optimization already proven important in the Python prototype.

---

# 43. View pipeline

View transforms are separate from build filters.

Examples:

- greyscale;
- invert;
- contrast/readability;
- future colour-vision simulations.

They consume the current edited view and produce display-only pixels.

They never alter the durable project model.

---

# 44. Loupe and Enhance

Lenses should operate on crops or borrowed regions where possible.

They should not require full-frame duplication for every movement.

Loupe:

```text
small crop
nearest-neighbor scale
pixel grid
cursor data
```

Enhance:

```text
lens crop
scale
optional display transform
```

---

# 45. Platform services

Do not create one giant `PlatformBackend`.

Use narrow services.

Primary runtime boundaries:

```text
Capture
DesktopInfo
OverlayHost
Clipboard
Notifications
Pins
```

These have different portability constraints and should not be forced into one interface.

---

# 46. Capture

Capture produces platform-neutral pixels and mappings.

Conceptually:

```text
capture_desktop()
capture_window(WindowId)
```

Initial Wayland implementation should support:

- whole-layout capture;
- window capture for covered/off-workspace windows where available.

Protocol selection remains an implementation detail inside the platform crate.

---

# 47. Desktop information

`DesktopInfo` produces core's `Layout`.

Initial implementation:

```text
Hyprland IPC
    ↓
DesktopInfo
    ↓
Layout
```

Future:

```text
X11/EWMH
    ↓
DesktopInfo
    ↓
same Layout
```

Select never calls Hyprland directly.

---

# 48. Overlay host

OverlayHost is responsible for:

- monitor overlay surfaces;
- redraw requests;
- raw keyboard events;
- raw pointer events;
- surface lifecycle.

Initial implementation:

```text
Wayland + layer-shell
```

Future:

```text
X11 windows
```

OverlayHost emits raw input.

It does not interpret screenie commands.

---

# 49. Runtime capabilities

Platform support is not one boolean.

Represent available functionality explicitly.

Examples:

```text
desktop_capture
window_capture
window_discovery
overlay
clipboard
notifications
pins
```

This allows useful partial support and precise startup errors.

Initial Hyprland target should support the full picker.

---

# 50. Event loop

Keep normal interaction single-threaded initially.

Conceptually:

```text
Wayland events
repeat timer
job completions
redraw requests
    ↓
event loop
    ↓
Session
    ↓
Renderer
```

Normal input path:

```text
event
→ state mutation
→ damage/cache update
→ redraw
```

Do not introduce a large async runtime unless measurements demonstrate a need.

---

# 51. Startup ordering

Preserve frozen-frame semantics.

Capture should begin as early as possible after invocation.

Conceptually:

```text
start capture immediately

in parallel:
    query desktop/layout
    construct runtime services
    prepare overlays

show frozen frame when required dependencies are ready
```

The exact startup sequence should be measured in Rust before optimizing further.

No daemon should be introduced unless measurements later justify it.

---

# 52. Clipboard and output delivery

The interactive session finishes with a platform-neutral action.

Conceptually:

```text
FinishedAction
    Image
    OCR
    Color
    Code
    Pin
```

The executable/front end delivers it through:

- clipboard;
- file output;
- stdout;
- notification;
- pin service.

The picker does not directly invoke platform clipboard mechanisms.

---

# 53. Pins

Pins are a separate subsystem.

Their lifetime differs from the picker.

Required interface conceptually:

```text
create
list
close
```

Built natively (2026-10-05): each pin is its own detached process,
`screenie __serve-pin ID PATH SCALE [X,Y]`, showing the image in an xdg
toplevel (app id `screenie-pin`, no decorations, fixed size, drawn at the
capture's scale). A window rule floats and pins it; once Hyprland lists the
window, the process moves it to X,Y (`hl.dsp.window.move` on 0.56's Lua
dispatch, `movewindowpixel exact` before).

Records are `$XDG_RUNTIME_DIR/screenie/pins/ID.json` (id, pid, image,
position, size). `pin list` drops records whose process isn't a pin server
any more; `pin close` sends SIGTERM, and the server removes its record and
image on the way out. One process a pin keeps a crash to one pin and needs
no daemon; the picker only spawns.

---

# 54. Background jobs

Expensive analysis runs through a job system.

Examples:

```text
OCR
auto-redact
edge detection
code detection
future scale-bar OCR
```

Each request should include:

```text
JobId
FrameVersion
Region
AnalysisKind
Parameters
```

Results include the same identity/version information.

Stale results are discarded.

Example:

```text
start OCR on region A
user changes relevant box to B
OCR A returns
→ ignore
```

Workers do not know about modes.

---

# 55. Analysis cache

Analysis may be cached by data such as:

```text
FrameVersion
PixelRect
AnalysisKind
Parameters
```

Caches are disposable.

This allows OCR output and auto-redact to share one OCR pass when applicable.

---

# 56. OCR representation

OCR returns structured data.

Conceptually:

```text
OCRResult
    full_text
    words[]
        text
        bbox
        confidence
```

Internal consumers include:

- OCR output;
- auto-redact;
- scale-bar reading;
- possible future OCR word selection.

CLI exposure of word boxes/confidence is a separate product decision.

---

# 57. Auto-redact

Auto-redact pipeline:

```text
OCR
    ↓
detectors
    ↓
RedactionProposal[]
    ↓
review
    ↓
accepted CensorOp[]
```

A proposal is temporary analysis state.

It is not part of the durable project until accepted.

Accepted proposals become ordinary censor operations.

This keeps project serialization independent of OCR engine behavior.

---

# 58. Measure

Measure is a mode over the frozen picture, not a build tool: it opens no
layer, and its measurements are mode state, overlay only.

It reuses capabilities, and keeps pixels out of core like the rest:

```text
points          PointCursor (Insert's), on capture pixels
paths           polylines of dropped points
calibration     TextInput child (Calibration purpose)
edges, areas    host EdgeFinder: an edge along a ray; a flooded region
scale bar       Analyzer job (OCR round the line), filling the prompt
```

Measurement state contains:

```text
measurements    kind, points, name, excluded from totals
calibration     Scale (px, real, unit, error): kept on the session, and
                saved in the project (Project::measure_scale)
undo            its own log (added, deleted, calibrated, excluded,
                swapped, edited)
editing         a point being moved (o, Alt+drag)
```

Values and errors are computed from the points and the scale when shown,
never stored. `docs/measure.md` has the design.

---

# 59. Text annotation

Text annotation is a first-class project operation.

Initial durable data should remain small:

```text
TextOp
    id
    position or rect
    text
    size
    color
    background/chip setting
    alignment if needed
```

Text editing uses the shared TextInput child.

Avoid rich-text/editor complexity in the first implementation.

---

# 60. Annotation extensions

Additional operations should reuse existing interaction vocabulary.

## Line

```text
AnchorCursor → LineOp
```

## Arrow

```text
AnchorCursor → LineOp with arrowhead
```

## Highlighter

```text
Trace → HighlightOp
```

## Callout

```text
Point → CalloutOp
optional TextInput for explicit numbering
```

Features that can be expressed as:

```text
existing capability + new Op
```

are preferred over features that introduce new interaction systems.

---

# 61. Keymap representation

Keymaps should be explicit data where practical.

Example conceptual structure:

```text
BoxGrammar
    h           Steer(Left)
    Shift+h     Translate(Left)
    Ctrl+h      Jump(Left)
    Alt+h       Scale(Left, Half)
```

Modes extend the common grammar with local bindings.

Example:

```text
Censor
    inherits BoxGrammar
    t           ChooseNextKind
    +           Adjust(+1)
    -           Adjust(-1)
    x           LockAndNext
```

The exact syntax may evolve.

The important properties are:

- common grammar has one implementation;
- collisions can be validated mechanically;
- palettes and legends derive from the same binding data where possible.

---

# 62. Key collision validation

Every mode should be testable for key collisions.

Future Measure, auto-redact and annotation extensions compete for a limited key budget.

Collisions should fail tests rather than rely on manual review.

---

# 63. Rendering/UI text generation

Where practical, UI legends and palettes should derive from mode metadata/keymaps rather than duplicate handwritten strings.

Do not over-generalize status text rendering; the goal is consistency, not building a UI framework.

---

# 64. Rust enum vs trait guidance

Use enums for closed sets owned by screenie:

```text
Mode
Op
Intent
Transition
ChildResult
UndoRecord
```

Use traits/interfaces where implementations are expected to vary independently:

```text
Capture
DesktopInfo
OverlayHost
Clipboard
PinService
```

Do not introduce a trait solely because two implementations currently share three methods.

Prefer direct code until repetition proves an abstraction useful.

---

# 65. Error handling

Distinguish major failure classes:

```text
user cancellation
usage/config error
capture unavailable/failure
delivery failure
unsupported platform capability
analysis failure
project format/migration failure
```

User cancellation is not an error.

CLI exit semantics are defined in `docs/cli.md`.

---

# 66. Testing strategy

The Rust test suite should treat current Python behavior as the executable reference during the port.

Most behavioral tests belong in `screenie-core`.

Tests should drive semantic or key-script input without creating display surfaces.

Important areas:

```text
geometry
anchor crossing
corner swaps
modifier semantics
selection scopes
window jumps
bounds
clipping
list selection/reorder
mode transitions
temporary child results
held keys across transitions
open-layer lifecycle
undo scopes
locked-work warnings
serialization
key collision detection
logical/pixel mapping
```

Image tests should cover:

```text
op rendering
layer order
clip behavior
filter results
cache equivalence
view/output separation
serialization → identical render
```

Platform tests should remain narrowly focused on adapters.

---

# 67. `--keys`

`--keys` should drive the same core interaction system as real input.

It is not a separate automation engine.

Conceptually:

```text
script
    ↓
synthetic input/intents
    ↓
same Session
```

Uses:

- integration tests;
- macros;
- debugging;
- reproducible bug reports.

---

# 68. Port strategy

Do not port module-by-module.

Port vertical behavior.

## Milestone 1: headless Select

Implement:

- workspace;
- logical geometry;
- AnchorCursor;
- intents;
- keymap;
- Select;
- layout abstraction;
- selection undo;
- `--keys`-style tests.

No Wayland or renderer required.

Acceptance:

```text
core Select behavioral tests pass headlessly
```

---

## Milestone 2: first native screenshot

Implement enough platform/image/frontend code for:

```text
capture
→ overlay
→ Select
→ crop
→ clipboard/file
```

Acceptance:

```text
Rust picker can replace Python for basic region screenshots
```

Do not remove Python fallback yet.

---

## Milestone 3: Censor and open layers

Implement:

- Project;
- Layer;
- CensorOp;
- open-layer lifecycle;
- open-layer undo;
- layer commit;
- censor renderer.

This proves the durable model and build-tool architecture.

---

## Milestone 4: current Annotate

Implement:

- Trace;
- pen;
- colour Choice;
- width Adjustable;
- Shape child;
- ShapeOp.

---

## Milestone 5: Filter

Implement:

- OrderedList;
- ListGrammar;
- FilterOps;
- amounts;
- mute;
- duplicate;
- reorder;
- render-prefix cache.

---

## Milestone 6: Layers

Implement:

- committed-layer management;
- project-history undo;
- hide/show;
- delete;
- reorder;
- inspection view.

---

## Milestone 7: lenses

Implement:

- Loupe;
- Enhance;
- view pipeline;
- child Position result;
- lens placement/tag collision.

---

## Milestone 8: CLI/output parity

Implement:

- OCR;
- colour output;
- QR/barcode;
- pins;
- history;
- project persistence;
- CLI verbs/profiles/JSON as required for parity.

---

## Milestone 9: daily-driver cutover

Only after:

- core Python workflows have Rust parity;
- behavioral tests cover regressions;
- startup/performance are acceptable;
- project recovery is reliable.

Then switch the normal Print binding to Rust.

Keep the Python implementation until the Rust version has survived daily use.

---

## Milestone 10: annotation expansion

Add:

- text;
- lines;
- arrows;
- highlighter;
- callouts;
- keyboard pen cursor.

These deliberately exercise the reusable interaction architecture.

---

## Milestone 11: analysis features

Add:

- shared OCR service;
- async job model;
- auto-redact;
- Measure;
- edge detection;
- future scale-bar OCR.

---

## Milestone 12: broader platforms

Add X11 and/or other compositor adapters only after the Wayland/Hyprland implementation is stable.

Do not compromise current architecture for hypothetical portability before a concrete adapter needs it.

---

# 69. Performance philosophy

Optimize measured bottlenecks.

Do not add:

- daemon architecture;
- renderer worker synchronization;
- GPU rendering;
- complex damage graphs;
- global async runtime

without measurements demonstrating a need.

Preserve known-effective optimizations:

- immutable capture;
- prefix rendering caches;
- local operation bounding boxes;
- crop-local view filtering.

Rust performance should first be obtained through simpler execution and reduced runtime overhead, not architectural complexity.

---

# 70. Design rule for new features

Before adding a new interaction concept, ask:

> Can this feature be represented as an existing capability plus a new operation or analysis result?

Examples:

```text
arrow      = AnchorCursor + LineOp
text       = Point/AnchorCursor + TextInput + TextOp
highlight  = Trace + HighlightOp
callout    = Point + CalloutOp
measure    = AnchorCursor/Trace + analysis
```

Prefer reuse.

Introduce a new interaction capability only when an existing one cannot describe the behavior cleanly.

---

# 71. Development contract

Behavioral changes update together:

1. `README.md`;
2. key/palette/legend metadata;
3. behavioral tests.

Architecture changes that affect persistent data or module boundaries update this document.

Project-format changes require:

- explicit version handling;
- migration tests;
- serialization round-trip tests.

The Rust port succeeds when screenie's interaction model remains familiar while the implementation becomes easier to reason about, test, serialize, extend and port.

---

# 72. Decisions

Choices made during the port that the sections above leave open. Each
says what was chosen and why. Smaller judgement calls live in
`rust/PROGRESS.md`.

## Workspace

- **Edition 2024, resolver 3, `unsafe_code = "deny"`** across the
  workspace. Nothing in core needs unsafe; platform code that does will
  opt out per module, visibly.
- **`screenie-core` depends only on `serde` and `serde_json`.** The
  durable model and the recall ring need serialization. Nothing else is
  needed for interaction logic.

## Coordinates

- **`PixelMap` lives in `screenie-core::geometry`**, not in
  `screenie-image` as §3.2 lists it. Select needs it headlessly: the
  empty-box check (a box under one output pixel doesn't capture), the
  status line's size in output pixels, and the cursor pixel. It is pure
  arithmetic with no pixel buffers. `screenie-image` uses the same type,
  so there is still exactly one conversion (§8).
- Logical coordinates are `f64`; pixel coordinates are `i32`.

## Interaction

- **`AnchorCursor` stores the rectangle plus which corner the cursor is
  on**, not two points. The two are equivalent, but this form makes the
  prototype's crossing rule exact, including its tie-break when the
  cursor lands on the anchor. The anchor and cursor points are derived.
- **Chords match exactly.** A chord is a normalised key plus Shift, Ctrl
  and Alt. An upper-case letter is Shift plus its lower-case letter; a
  symbol (`$`, `{`, `?`, `+`) has its Shift consumed; a named key
  (`Space`, `Return`, arrows) keeps it. Any chord not in the mode's table
  does nothing, so a meaningless modifier never falls through (README,
  Directions).
- **Keymaps are tables** (`keymap.rs`), built from shared grammars
  (directions, box keys, `+`/`-`) plus each mode's keys. The same tables
  drive dispatch, the `?` legend and collision tests.
- **Time is passed in** as `f64` seconds on a monotonic clock, with each
  event and tick. Core never reads a clock, so repeat is deterministic
  in tests.
- **Repeat steps are the exact integral of the ramp** (start rate to top
  rate over `RAMP`, after `DELAY`), counted from the last tick. The
  prototype summed rate × frame time, so its count depended slightly on
  the frame rate; this doesn't.
- **Selection undo restores the rectangle only.** The cursor corner and
  centre mode are transient, as in the prototype. One entry is recorded
  per tap, per held-key repeat (with its tap), per drag, per click, per
  scope step and per recall. `u` skips entries that would change
  nothing, so every `u` visibly does something.

- **The host provides what core can't**: pixels for the eyedropper
  (`Sampler`) and code reading for `Q` (`CodeReader`), as trait objects set
  on the session. Core still never sees a pixel buffer.
- **Repeat is per mode**: box modes 20→300 steps/s, Insert scaled to cross
  the box in about a second, the loupe 15→200 px/s (Shift 4→12 widths/s);
  amounts that explicitly repeat (`+`/`-` in tools) follow the keyboard's
  own repeat rate, which the compositor reports.
- **Analysis jobs go through an `Analyzer`** on the session (§54). The
  session gives each job an id; the analyzer answers at once (`--keys`,
  `--no-ui`, `--redact`) or later through `Session::job_done`, from a
  thread whose results the overlay collects on ticks. A result whose id
  no mode is waiting for is dropped, which covers rescans and leaving the
  review. Jobs carry a pixel rect of the committed picture; there's no
  frame version yet, because the frame never changes within a session.
  No analysis cache yet (§55): scans are on request.
- **Measure is a mode, not a tool** (§58): it opens no layer and its
  measurements are mode state, overlay only, dropped on leaving (baking
  them into an annotation layer was built, then dropped). Its edges come from
  a host `EdgeFinder` (the committed picture's grey along a ray), like the
  `Sampler`, so core still never sees pixels. The scale is kept on the
  session and saved in the project (`Project::measure_scale`, an optional
  field older projects don't have), so `edit last` keeps it;
  measurements themselves aren't saved.
- **Auto-redact's suggestions live in a `Redact` mode**, a child of
  Censor, never in the project (§57). Accepting adds black-bar censors to
  the open layer as one undo step (`LayerUndo::RemoveMany`). The session
  remembers which ops were accepted, so `--json` reports only the ones
  still shown.

- **The mouse's chips and handles** (docs/mouse.md, 2026-10-05): the Ui
  records where it drew each chip and palette item (`Ui::hit`), and the
  picker gives a left press on one to the chip (`Session::act` for an
  item) before the session sees it. Box handles are core (`boxes::Handle`,
  `Session::pointer_handle`); `OverlayApp::pointer` turns them into a
  cursor shape (wp-cursor-shape).

## Platform

- **One Wayland connection** (smithay-client-toolkit 0.21) for capture and
  overlays: wlr-screencopy for the frame, composed at the highest scale;
  ext-image-copy-capture with a foreign-toplevel source for a window's own
  pixels; layer-shell overlays drawn into shm. A capture that gets no
  frame fails after 2 s rather than hanging (monitors asleep).
- **The clipboard is served by a detached copy of screenie**
  (`__serve-clipboard`, wl-clipboard-rs in the foreground): Wayland's
  clipboard dies with its owner, and this needs no `unsafe` fork.
- **tiny-skia** draws strokes and shapes; **fontdue** draws the tags;
  filters and view filters are our own per-pixel maths.
- OCR (tesseract, TSV) and codes (zbarimg) are external programs, as in
  the prototype. Pins were swayimg too, until 2026-10-05; they're now
  screenie's own windows (§53).

## Files

- **Project files are PNGs**: the uncensored capture, with the versioned
  project and its layout as JSON in a `screenie-project` iTXt chunk. One
  self-contained, private (0600) file beside each shot; any viewer opens
  it as the plain capture.
- **The recall ring keeps the prototype's `regions.json` format** in the
  same place (`$XDG_STATE_HOME/screenie/regions.json`), so the Python and
  Rust pickers share one ring until the cutover. Core holds the data and
  its JSON; the front end does the file I/O.
