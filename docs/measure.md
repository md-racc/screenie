# Measure: lengths, angles and areas, with a scale

**Status:** a design scaffold. It is not in the prototype. `README.md`
(Measure) and `docs/architecture-rust.md` (§58) take precedence over this
page. The architecture models a measure path as a `Trace`; the "keep
adding points" polyline below is the older wording, and which one is
meant is still open.

The box is already a measurement (its tag shows its size), and the loupe
and enhance are made for precise placement. Measure turns that into a
tool:
- **in pixels** by default: UI work, checking alignment, measuring gaps;
- **in real units** after calibrating. On a floor plan or map with a scale
  bar, measure the bar once, type what it represents ("5 m"), and every
  line, arc and area after it reads in metres.

## Entering

- In Select, press `m`, which is free there. It opens Measure over the
  frozen picture, with the cursor at the box's cursor corner.
- From the command line: `--mode measure`, or `--result measure` (Enter
  delivers the measurements).
- Measure doesn't change the picture: it isn't a build tool and opens no
  layer. Its drawings are overlay only, until you bake them (see Output).

## Primitives

| Key | Primitive | Points | Reports |
|---|---|---|---|
| `l` | line, or a path when you keep adding points | 2+ | length, angle (a path: total and each segment) |
| `(` | arc (start, a point on it, end) | 3 | arc length, radius, angle swept |
| `c` | circle (three points on it, as Annotate's `c`) | 3 | diameter, circumference, area |
| `A` | area (polygon) | 3+ | area, perimeter |
| `r` | rectangle (the box, or a new box) | 2 corners | w, h, area, diagonal |
| `^` | angle (arm, vertex, arm) | 3 | degrees |

These keep clear of the grammar's meanings elsewhere: `o` corners, `p`
colour, `w b` edges, `u` undo, `a` annotate.

Placing points uses the same grammar as elsewhere:
- **Cursor:** Insert's pixel cursor. hjkl move, Shift is ×4, `+`/`-`
  change the step. The mouse moves it too.
- **Ctrl+hjkl** jumps to the next **edge in the picture** that way: a
  contrast edge along the ray, such as a wall's line on a plan. Measuring
  wall to wall is then two keystrokes, and exact to the pixel. This is the
  one new piece of image analysis Measure needs.
- **Space** or a left click drops a point.
- **Enter** finishes the primitive (a path or area), and **Esc** abandons
  the one in progress.
- **u** takes back the last point, or the last primitive.
- **Snapping:** points within a few pixels of an existing point, endpoint
  or edge snap to it; Alt disables snapping while held. Shift while
  *clicking* locks to 0/45/90° (Shift on hjkl is still the bigger step).
- `z` / `e` open the loupe and enhance at the cursor, for sub-pixel care.
  Points are placed at pixel centres of the capture (device pixels), not
  logical ones.

## Calibration

The first **line, path or arc** you finish asks for its real length, in a
small input chip at the end of it:

```
= 5 m_          (Enter to set the scale, Esc to stay in pixels)
```

- Input: a number and a unit. Supported units:
  - metric: `mm`, `cm`, `m`, `km`;
  - imperial: `in`, `ft`, `yd`, `mi`;
  - feet and inches: `12'6"`, or `12ft 6in`.

  Arithmetic like `2*2.5 m` is nice to have.
- The scale is then `real length / pixel length`. That measurement becomes
  the **reference**, drawn in a distinct style (dashed, with a ruler
  glyph).
- Everything else, drawn before or after, reads in real units: lengths in
  the reference's unit, areas in its square. `t` toggles to the other
  system (m ↔ ft), and `T` shows pixels again.
- `C` re-calibrates against the selected measurement, or asks for a new
  reference.
- A scale can come in from the command line with `--measure-scale '84.2px=1m'`,
  for a plan you open at the same zoom every time. It can also be saved in
  the shot's project (`edit last` keeps it).

**Why an arc can be the reference:** some plans draw their scale on a curve
(a circular site boundary of known length), and a 3-point arc's length is
as exact as a line's.

**Precision.**
- A reference 80 px long has about ±1 px of error at each end, so the
  scale is only good to about ±1.8%. Measurements inherit that, plus their
  own ±1 px per point.
- The status tag shows it: `1 m = 84.2 px ±1.7%`, and a measurement shows
  `4.20 m ±0.08`.
- When the reference is short (error > 2%), the chip suggests measuring a
  longer bar, or zooming the source first.

**Assumptions.**
- The plan is orthographic and isotropic: the same scale in x and y, which
  holds for plans, maps and CAD exports. Rotation doesn't matter.
- **Perspective photos are a non-goal.** They would need four-point
  (homography) calibration; see Later.

## On screen

- Each measurement is drawn in the overlay: lines with end ticks, arcs,
  and area polygons with a light fill. Each has a chip at its midpoint or
  centroid, like `4.20 m`, `12.6 m²` or `37.5°`.
  - Chips follow the tag rules: never overlapping each other or the box's
    tags, and never off a monitor.
  - The reference's chip shows `ref 5 m`.
- The side list shows every measurement, top first, and totals: the sum of
  the selected lengths and areas. `[` `]` select, `d` deletes, and Space
  hides one from the total.
- The status tag shows the tool, the cursor pixel, the live length from
  the last point, and the scale.

## Output

- **Enter**, with `--result measure` or `y`: the measurements as text,
  copied, like this:

  ```
  ref   5 m        (411.0 px)
  L1    4.20 m
  A1   12.6 m²     perimeter 14.3 m
  ∠1   37.5°
  ```

  With `--json`:

  ```json
  "scale": {"px": 411.0, "real": 5, "unit": "m", "error": 0.0034},
  "measurements": [{"kind":"line","points":[[x,y],[x,y]],"px":352.1,"real":4.2,"unit":"m"}]
  ```

  Points are in capture pixels.
- **`a` (bake, as in annotate):** turns the measurements into an annotate layer:
  - dimension lines with arrowheads and their labels;
  - area outlines;
  - the reference drawn as a scale bar.

  Then Enter captures an image with them in. This reuses the ops from
  `docs/annotate.md` (arrows, text).
- **Esc** back to Select drops the measurements, unless they were baked.

## Later

- **Auto-calibrate from the scale bar.** When the reference line runs
  along a drawn scale bar, OCR the text next to it ("0  5  10 m") and
  pre-fill the input. This reuses auto-redact's OCR with word boxes.
- **Four-point calibration** for photos taken at an angle (a homography):
  mark a known rectangle, and measure on the plane.
- **Different x/y scales**, for stretched scans.
- **Counting:** click things to count them (doors, outlets), with the
  totals in the list.

## Open questions

- Should `m` open Measure with the line tool, or with a picker for the
  primitive?
- Should measurements survive going back to Select (a measure layer in the
  stack, off by default in output), or be dropped?
- Should Ctrl+hjkl edge detection be a threshold on luminance change along
  the ray, or use the frame's already-computed grey image with a small
  gradient kernel? The second is more robust on anti-aliased plans.
