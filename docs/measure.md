# Measure

Measure measures lengths, angles and areas over the frozen picture: in
capture pixels, or in real units once calibrated. In pixels it serves UI
work: checking alignment, measuring gaps. On a floor plan or a map,
measure the scale bar once, say what it represents (or let screenie read
it), and every line, arc and area after it reads in metres.

## Entering and leaving

- `m` in Select, or `--mode measure`. With `--result measure`, Enter in
  Select opens Measure.
- Measure is a mode over the frozen picture, not a build tool. It opens no
  layer, and its drawings are overlay only, never in the image. To put a
  measurement in a shot, mark it up with Annotate.
- It starts with the line tool.
- `y`, or Enter with nothing in progress, delivers the measurements (see
  [Output](#output)); with the rect tool, Enter with no corners measures
  the box instead. `y` with nothing measured says so and stays; Enter then
  does nothing.
- Esc drops the measurement in progress. With nothing in progress it goes
  back to Select. If there are measurements, the first Esc warns
  (`Esc again drops N measurements`) and the second drops them.
- The scale is kept for the rest of the session and saved in the project,
  so `edit` keeps it.

## The cursor

Points are placed with Insert's pixel cursor, on capture pixels. It starts
at the box's cursor corner and stays on that corner's monitor. The mouse
moves it too, once the mouse moves (the overlay appearing under the
pointer doesn't count).

Each modifier means one thing, across keys and the mouse:

| | Keys | Mouse |
|---|---|---|
| plain | `h j k l` step; `Space` drops a point | move; click drops a point |
| **Shift** | `h j k l` ×4; `Shift+Space` drops a path point | `Shift+click` drops a path point, reaching 12px for another measurement's point or line |
| **Ctrl** | `Ctrl+h j k l` jumps to the next edge in the picture | hold: the cursor locks to 0°, 45° or 90° from the last point |
| **Alt** | `Alt+h j k l` goes halfway to the monitor's edge; `Alt+Space` drops a point, unsnapped | `Alt+click` selects a measurement; `Alt+drag` on its point moves it |

So **Shift extends** (a path; a further reach), **Ctrl constrains** (to
the picture's edges, to angles), and **Alt is about measurements, not
points** (select, move), except `Alt+Space`, the keyboard's unsnapped
point.

- `0 $ g G` go to the monitor's far edges.
- `+` / `-` and the wheel adjust the step (1–20px, default 5), or, with a
  line's first point down, its bend.
- An edge for `Ctrl+h j k l` is a change in grey of 32 or more from one
  pixel to the next (48 from two before, for anti-aliased edges), averaged
  over three pixels across the way it moves. With none, the status says
  so.
- A click with Ctrl held lands on the locked angle too. The status shows
  `45° lock` while Ctrl is held.

## Tools

The palette shows them all the time:

| Key | Tool | Placed by | Reports |
|---|---|---|---|
| `s` | line | two points; Shift'd points make it a path | length (a path's total), direction |
| `c` | circle | three points on it | diameter, circumference, area |
| `A` | area | one point inside | area, perimeter |
| `r` | rect | two corners; Enter with none measures the box | width × height, area, diagonal |
| `^` | angle | arm, vertex, arm; or one point on a vertex of lines | degrees |

- **Finishing.** A line ends at its second point, unless that point is
  Shift'd: then it stays open as a path, which Enter or a plain point
  ends. A rect finishes at its second corner, a circle and an angle at
  their third point. Enter ends the one in progress at the cursor (adding
  a point there if the cursor moved on); a line or rect needs 2 points,
  the others 3.
- Switching tools drops the points in progress. A point on top of the
  last one is refused.
- **Direction.** A line's direction is in degrees counter-clockwise from
  rightwards: 0° points right, 90° up.
- **Arcs.** With a line's first point down, `+` / `-` or the wheel bend it,
  2px a step, into an arc over the chord to the cursor: positive bends to
  the left of the line from the first point (up, for a line drawn
  rightwards), negative the other way. The status shows `arc 10px`, and
  the live length runs along the arc. A bent line finishes as an arc,
  which reports its length, radius and the angle it sweeps.
- **Areas by flood.** From the point, the region spreads over pixels
  (4-connected) whose grey is within 40 levels of the starting pixel's. It
  is stopped by the picture's edges and by every drawn measurement: draw a
  line across a doorway to close a room. It measures the outer outline,
  with anything inside included, to the drawn lines and to the last pixel
  inside a picture's edge, simplified to within half a pixel. If it would
  reach the monitor's edge it isn't closed in, and the status says
  `not closed in: draw lines to close it`. A point on a drawn line is
  refused. A shape outlined with a closed path is measured the same way.
- **Angles at a vertex.** A first `^` point on any line or path point, or
  where two lines cross, measures every angle round it at once. Each line
  ending there is one arm, each passing through is two, and every gap
  under 180° between neighbouring arms is an angle. A T gives its two
  right angles but not the straight back; a corner gives its inside, not
  its outside. Anywhere else, the angle takes its three points as usual.
  A crossing is rounded to its pixel, so opposite angles there can differ
  by a fraction of a degree. Only straight lines and paths count; arcs
  don't.
- **Snapping.** A point snaps to any point placed within 4px, and, for
  `^`'s first point, to a crossing. A `Shift+click`, or `^`'s first click,
  reaches 12px for another measurement's point (or a crossing, for `^`);
  failing that, a `Shift+click` lands on the nearest straight line or path
  within 12px. `Alt+Space` and the loupe never snap.

## Loupe and Enhance

- `z` opens the loupe at the cursor, on the cursor's monitor. In it,
  `Space` or `Alt+Space` drops a point at the loupe's pixel exactly (never
  snapped) and `Shift+Space` a path point; the loupe stays up. Closing it
  leaves the cursor on the loupe's pixel.
- `e` opens Enhance on a 480×270 region round the cursor, kept inside its
  monitor.

## Selecting and editing

- The measurement just made is selected. `[` / `]` select the next one
  down / up the list, and `Alt+click` selects the one under the pointer
  (within 12px of its points or drawing).
- `<` / `>` move it down / up the list, `d` deletes it, and `x` leaves it
  out of the totals, or puts it back (Space drops points here, so not
  Space).
- `o` edits the selected measurement's points, with nothing in progress.
  The cursor goes to its first point; `o` goes on to the next. The cursor
  keys or the mouse move the point; Space or Enter keep it, Esc or `u` put
  it back, and any other key keeps it first. An `Alt+drag` on a point does
  the same with the mouse, and letting go keeps it. An edit that moved
  something is one undo step.
- Editing the reference rescales: its real length stays, its pixels
  change.
- Deleting the reference keeps the scale.

## Undo

Measure has its own undo. `u` takes back the last point of the one in
progress, else the last action: a measurement (a vertex's angles as one),
a delete (back where it was), a calibration (to the scale before), an `x`,
a move in the list, or an edit.

## Calibration

- Everything is in pixels until `C` on a selected line, path or arc, which
  asks for its real length. It becomes the reference, drawn dashed and
  labelled `ref`. Esc in the prompt leaves the scale as it was.
- **Reading a scale bar.** `C` also reads round the line with OCR (its
  bounds plus 48px, within the monitor), in the background. If it reads a
  bar before anything is typed, it fills the prompt; typing replaces the
  reading. It reads:
  - the numbers nearest the line's two ends, within 24px or 15% of the
    line's length, whichever is more: `0 … 100 m` reads `100 m`;
  - one number at one end, the other end taken as an unread or
    unlabelled 0;
  - a lone number, when it's the only one in view: a `5 km` label.

  The unit is one glued to a number (`100m`), else the standard unit word
  nearest the line's middle, in any case (`KM`, `Ft`).
- **Typing the length.** The standard units are `mm cm m km in ft yd mi`,
  with `'` and `"` for feet and inches.
  - Several parts add up, in the first part's unit: `12'6"`, `12ft 6in`.
    Each part needs its unit, so `12'6` and `1 m 50 bricks` are refused.
  - Numbers take arithmetic, `+ - * /`, with the usual precedence:
    `2*2.5 m`.
  - Any other name after the number is a unit of its own (`4.2 ly`,
    `3 bricks`, `2 light years`; cut at 24 bytes), and values read as
    multiples of it. It has nothing to convert to.
  - A number alone is in `u`, and Enter with nothing typed makes the line
    1 u.
  - The length must be more than zero.
- **Showing values.** Each value is shown to three significant figures, in
  the unit of the reference's system that suits its size: the largest of
  km, m, cm, mm (or mi, ft, in) it is at least one of, as shown, so 999.8 m
  shows as `1.00 km`. An area is fitted by its side: 0.09 m² is `900 cm²`.
  `t` switches system (metric ↔ imperial; a unit of its own can't), and
  `T` shows pixels or real units. Pixel lengths have one decimal
  (`352.1 px`), pixel areas none (`1234 px²`), angles one decimal
  (`37.5°`).
- **Precision.** Each point is good to about 1px. A reference of `n`
  points, `L` px long, is good to `√n / L` (80px: ±1.8%). A length
  combines that with its own `√n` px; an area combines twice it with half
  its perimeter in pixels. Values show their error, to their own decimals
  or more: `4.20 m ±0.08`. While the reference's error is over
  2%, the status says `short reference: measure a longer one, or zoom the
  source`.
- `--measure-scale PX=LENGTH` starts calibrated, for a plan opened at the
  same zoom every time: `--measure-scale '84.2px=1m'`. LENGTH is read as
  typed lengths are. A scale given this way has no error of its own.

## On screen

- Measurements are drawn in cyan, the selected one in amber and on top,
  the reference dashed. Areas are tinted, and angles have an arc between
  their arms.
- Each has a label chip: beside a line's middle, at an area's or circle's
  centroid, out from an angle's arc. A circle's label is its diameter
  (`Ø 1.20 m`). Labels keep clear of each other, the arcs and the loupe,
  and stay on their own monitor.
- The status shows the tool, the cursor's pixel, the live length from the
  last point (along the arc for a bent line), the scale
  (`1 m = 84.2 px ±1.7%`, `(showing px)` under `T`), and the point being
  edited.
- The side list names every measurement (`L1`, `∩1` arcs, `○1` circles,
  `A1` areas, `R1` rects, `∠1` angles; names are never reused), marks
  those left out (`(not in total)`), and ends with the total of the
  lengths (lines, paths, arcs) and of the areas (areas, rects, circles)
  it counts, when there are two or more of a kind.

## Output

`y`, or Enter with nothing in progress (except with the rect tool),
delivers a text result (copied to the clipboard by default;
see [cli.md](cli.md)), one line a measurement, the reference first:

```text
ref   5.00 m  (411.0 px)
L2    4.20 m ±0.08     at 37.5°
A1    12.6 m² ±0.5     perimeter 14.3 m
∠1    37.5°
```

A path's line gives its segment count, an arc its radius and angle, a
circle its circumference and area, a rect its width × height and
diagonal.

With `--json`, `kind` is `measure`, with:
- `scale`: `px`, `real`, `unit`, `error` (a fraction), or `null` when
  uncalibrated;
- `measurements`: each with `kind` (`line`, `arc`, `circle`, `area`,
  `rect`, `angle`), `name`, `points` (`[x, y]` in capture pixels), `px`
  for a length or `px2` for an area, `real` and `unit` in the unit shown
  (`m²` for areas; left out while showing pixels or uncalibrated),
  `degrees` for an angle, and `reference: true` on the reference.

With `--select-only`, finishing in Measure is a cancel: there's no box to
give.

## Not supported

- Perspective photos (four-point calibration), and different x and y
  scales for stretched scans.
- Snapping points to edges in the picture: `Ctrl+h j k l` jumps to them,
  and the area flood stops at them.
- Measurements in the image itself: use Annotate.
- Counting things by clicking them.
