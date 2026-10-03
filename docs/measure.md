# Measure: lengths, angles and areas, with a scale

**Status:** built in the Rust port. `README.md` (Measure) has the keys and
takes precedence; `docs/architecture-rust.md` §58 has how it's built. This
page is the design as it stands, and why. It was rewritten on 2026-10-03,
after a round of changes made one at a time; the old design's differences
are at the end.

The box is already a measurement (its tag shows its size), and the loupe
and enhance are made for precise placement. Measure turns that into a
tool:
- **in pixels** by default: UI work, checking alignment, measuring gaps;
- **in real units** once calibrated: on a floor plan or map, measure the
  scale bar once, say what it represents (or let screenie read it), and
  every line, arc and area after it reads in metres.

## Entering and leaving

- `m` in Select, `--mode measure`, or `--result measure` (Enter then
  delivers the measurements).
- Measure is a mode over the frozen picture, not a build tool: it opens
  no layer, and its drawings are overlay only, never in the image.
- `y`, or Enter with nothing in progress, delivers the measurements as
  text (and `--json` data). Esc with nothing in progress goes back to
  Select, warning first if there are measurements, which it drops. The
  scale is kept for the session, and saved in the project.

## The cursor, and what the modifiers mean

Points are placed with Insert's pixel cursor, on capture pixels, kept to
the box's monitor. The mouse moves it too, once it moves.

Each modifier means one thing, across keys and the mouse:

| | Keys | Mouse |
|---|---|---|
| plain | `hjkl` step; `Space` drops a point | move; click drops a point |
| **Shift** | `hjkl` ×4; `Shift+Space`: a path point | `Shift+click`: a path point, reaching 12px to land on another measurement's point or line |
| **Ctrl** | `Ctrl+hjkl`: jump to the next edge in the picture | hold: lock to 0/45/90° from the last point |
| **Alt** | `Alt+hjkl`: halfway to the monitor's edge; `Alt+Space`: a point, unsnapped | `Alt+click`: select a measurement; `Alt+drag` on its point: move it |

So: **Shift extends** (a path; further reach), **Ctrl constrains** (to
the picture's edges, to angles), **Alt is about measurements, not
points** (select, move), except `Alt+Space`, the keyboard's unsnapped
point. The loupe's points (`Space` inside it) are always exact, never
snapped: it's the bypass.

`+`/`-` and the wheel are the one adjustable amount: the step size, or,
with a line open, its bend.

## Tools

The palette shows them, always:

| Key | Tool | Placed by | Reports |
|---|---|---|---|
| `s` | line | two points; Shift'd points make it a path, which Enter or a plain point ends; bent with `+`/`-` while open, it's an arc | length (a path's total), direction; an arc's length, radius, angle |
| `c` | circle | three points on it | diameter, circumference, area |
| `A` | area | one point inside: flooded out to the picture's edges and the drawn lines | area, perimeter |
| `r` | rect | two corners; Enter with none measures the box | w × h, area, diagonal |
| `^` | angle | arm, vertex, arm; or one point on a vertex of lines, for every angle round it | degrees |

- **Angles at a vertex.** A first `^` point on any line or path point, or
  where two lines cross, makes every angle round it at once: each line
  ending there is one arm, each passing through is two, and every gap
  under 180° between neighbouring arms is an angle. A T gives its two
  right angles but not the straight back; a corner, not its outside.
- **Areas by flood.** From the point, the region spreads over pixels
  within 40 grey levels of it, stopped by the picture's edges and by every
  drawn measurement (draw a line across a doorway to close a room). It
  measures the outer outline, things inside included: to drawn lines, and
  to the last pixel inside a picture's edge. Reaching the monitor's edge
  means it isn't closed in, and it says so. A shape outlined with a
  closed path is measured the same way.
- **Snapping.** A point snaps to another point within 4px. A Shift+click
  reaches 12px, for another measurement's point first, then its line. The
  loupe and `Alt+Space` don't snap.

## Selecting and editing

- The last measurement made is selected. `[`/`]` or Alt+click select
  another; `<`/`>` move it in the list; `d` deletes it; `x` leaves it out
  of the total (Space drops points here, so not Space).
- `o` edits the selected measurement's points: the cursor goes to its
  first point, `o` to the next, the cursor keys (or the mouse) move it,
  Space or Enter keep it, Esc or `u` put it back. An Alt+drag on a point
  does the same with the mouse.
- Editing the reference rescales: its real length stays, its pixels
  change.

## Undo

Measure has its own scope. `u` takes back a point of the one in
progress, else the last action: a measurement (a vertex's angles as one),
a delete (back where it was), a calibration (to the scale before), `x`,
a move in the list, or an edit.

## Calibration

- Everything is in pixels until `C` on a selected line or arc, which asks
  for its real length.
- **Reading a scale bar.** `C` also OCRs round the line, in the
  background, and fills the prompt if it reads a bar before anything's
  typed: the numbers nearest the line's ends (`0 … 100 m` reads `100 m`;
  an unread 0, as OCR often drops a lone one, counts as 0), or a lone
  label (`5 km`). Typing replaces the reading.
- **Units.** Standard ones (`mm cm m km in ft yd mi`, `12'6"`,
  `12ft 6in`, and arithmetic like `2*2.5 m`) convert; any other name
  (`4.2 ly`, `3 bricks`) is a unit of its own, read as multiples; a
  number alone is `u`, and Enter alone makes the line 1 u.
- **Showing values.** Each is shown in the unit of the reference's system
  that suits its size (km, m, cm, mm; mi, ft, in); `t` switches system
  and `T` shows pixels.
- **Precision.** A reference `n` points and `L` px long is good to about
  `√n / L` (80px: ±1.8%); a measurement inherits that, plus its own ±1px a
  point, and shows it: `4.20 m ±0.08`. While the reference's error is
  over 2%, the status says to measure a longer one.
- `--measure-scale 84.2px=1m` starts calibrated, for a plan opened at the
  same zoom every time.

## On screen

- Measurements draw in cyan, the selected one in amber and on top, the
  reference dashed; areas tinted; angles as an arc between their arms.
- Each has a label chip: beside a line's middle, at an area's centroid,
  out from an angle's arc. Labels keep clear of each other, the arcs and
  the loupe, and stay on their own monitor.
- The status shows the tool, the cursor pixel, the live length (along the
  arc for a bent line), the scale, and what's being edited.
- The side list shows every measurement and the totals of the lengths and
  of the areas it counts.

## Output

`y` (or Enter with nothing in progress) delivers, and copies:

```
ref   5.00 m  (411.0 px)
L2    4.20 m ±0.08     at 37.5°
A1    12.6 m² ±0.5     perimeter 14.3 m
∠1    37.5°
```

With `--json`, `kind` is `measure`, with `scale` (`px`, `real`, `unit`,
`error`) and `measurements` (`kind`, `name`, `points` in capture pixels,
`px` or `px2`, `real` and `unit` in the unit shown, `degrees`).

## Not in it

- **Perspective photos** (four-point calibration, a homography), and
  different x/y scales for stretched scans.
- **Snapping points to picture edges**: dropped on 2026-10-03; Ctrl+hjkl
  jumps to them, and the area flood stops at them.
- **Baking measurements into the image**: built, then dropped
  (2026-10-03). Mark one up with Annotate if it needs to be in the shot.
- **Counting** (click things to count them): an idea, not designed.

## How it got here

Where this differs from the first design, and why:

- **Line is `s`, not `l`** (`l` is hjkl's). There's no arc tool (`(`): a
  line bends into one. No polygon tool: areas flood from a point inside.
- **Lines end at their second point**, with Shift for paths, rather than
  waiting for Enter.
- **No calibration until asked**: the first line used to ask for its
  length.
- **The 45° lock is holding Ctrl** (it was Shift+click), since Shift
  extends.
- **`x`, not Space, leaves one out of the total**, since Space drops
  points; otherwise the list follows the shared grammar.
- **A path is a polyline** of dropped points, not a `Trace`.
- **The cursor stays on the box's monitor.**
