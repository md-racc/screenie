"""Selection geometry: the box, its visual-block cursor, snapping, undo.

Pure logic, no GTK or cairo, so it can be unit-tested (and ported) on its
own. Rects are [x, y, w, h] in Hyprland's global logical coordinates.
"""

STEP = 5  # px per keyboard jump (logical)

# Snap targets closer than this to where an edge already is are skipped, so a
# jump never stops on the other side of a window gap.
GAP = 12

SIDES = {"h": "L", "j": "B", "k": "T", "l": "R",
         "Left": "L", "Down": "B", "Up": "T", "Right": "R"}
VEC = {"L": (-1, 0), "R": (1, 0), "T": (0, -1), "B": (0, 1)}
OPPOSITE = {"L": "R", "R": "L", "T": "B", "B": "T"}


def contains(r, px, py):
    x, y, w, h = r
    return x <= px < x + w and y <= py < y + h


def dist(r, px, py):
    x, y, w, h = r
    dx = max(x - px, 0, px - (x + w))
    dy = max(y - py, 0, py - (y + h))
    return dx * dx + dy * dy


def same(a, b):
    return all(abs(p - q) < 0.5 for p, q in zip(a, b))


def snap_targets(rects):
    """Every left/right and top/bottom edge of rects, per axis."""
    return {
        "x": sorted({r[0] for r in rects} | {r[0] + r[2] for r in rects}),
        "y": sorted({r[1] for r in rects} | {r[1] + r[3] for r in rects}),
    }


class Selection:
    """A box as an anchor corner plus a cursor corner, like vim visual-block.

    bounds: the rect the box must stay inside (the whole layout for the main
    selection, the main box for a censor box). windows / monitors: rects for
    scope (+/-) and neighbour jumps. targets: snap targets (snap_targets()).
    """

    def __init__(self, rect, bounds, windows, monitors, targets):
        self.bounds = list(bounds)
        self.windows, self.monitors, self.targets = windows, monitors, targets
        self.rect = list(rect)
        self.cur = ["R", "B"]  # the corner h/j/k/l drive; anchor is opposite
        self.center = False
        self.history = []  # rects, or entries with .undo() -> rect
        self.clamp()

    # --- basics -------------------------------------------------------------

    def limits(self, axis):
        bx, by, bw, bh = self.bounds
        return (bx, bx + bw) if axis == 0 else (by, by + bh)

    def clamp(self):
        bx, by, bw, bh = self.bounds
        x, y, w, h = self.rect
        w = max(0, min(w, bw))
        h = max(0, min(h, bh))
        x = max(bx, min(x, bx + bw - w))
        y = max(by, min(y, by + bh - h))
        self.rect = [x, y, w, h]

    def set_rect(self, r):
        self.rect = list(r)
        self.clamp()

    def remember(self):
        last = self.history[-1] if self.history else None
        if not isinstance(last, list) or not same(last, self.rect):
            self.history.append(list(self.rect))

    def undo(self):
        if not self.history:
            return
        entry = self.history.pop()
        if not isinstance(entry, list):
            entry = entry.undo()
        self.set_rect(entry)

    def empty(self):
        return self.rect[2] < 0.5 or self.rect[3] < 0.5

    def edges(self):
        x, y, w, h = self.rect
        return {"L": x, "T": y, "R": x + w, "B": y + h}

    def set_edges(self, e):
        self.rect = [e["L"], e["T"], e["R"] - e["L"], e["B"] - e["T"]]
        self.clamp()

    def centre(self):
        x, y, w, h = self.rect
        return x + w / 2, y + h / 2

    def cursor_point(self):
        if self.center:
            return self.centre()
        e = self.edges()
        return e[self.cur[0]], e[self.cur[1]]

    # --- the cursor (visual-block) ------------------------------------------

    def cursor_to(self, axis, value):
        """Put the cursor's edge at value. Crossing the anchor just swaps which
        side the cursor is on. In centre mode the centre stays put and the
        box grows/shrinks symmetrically instead (bottoming out at zero)."""
        e = self.edges()
        lo_side, hi_side = ("L", "R") if axis == 0 else ("T", "B")
        lo_lim, hi_lim = self.limits(axis)
        cur = self.cur[axis]
        if self.center:
            c = (e[lo_side] + e[hi_side]) / 2
            delta = value - e[cur]
            half = (e[hi_side] - e[lo_side]) / 2 + (delta if cur == hi_side else -delta)
            half = max(0, min(half, c - lo_lim, hi_lim - c))
            e[lo_side], e[hi_side] = c - half, c + half
            self.set_edges(e)
            return
        anchor = e[OPPOSITE[cur]]
        value = max(lo_lim, min(value, hi_lim))
        if value != e[cur]:
            self.cur[axis] = hi_side if value > anchor or (value == anchor and value > e[cur]) else lo_side
        e[lo_side], e[hi_side] = min(value, anchor), max(value, anchor)
        self.set_edges(e)

    def cursor_by(self, d, px):
        axis = 0 if d in "LR" else 1
        dx, dy = VEC[d]
        self.cursor_to(axis, self.edges()[self.cur[axis]] + (dx or dy) * px)

    def cursor_jump(self, d, to_limit=False):
        """w/b/}/{: cursor to the next snap target; 0/$/g/G: to the bounds."""
        axis = 0 if d in "LR" else 1
        direction = sum(VEC[d])
        value = self.edges()[self.cur[axis]]
        t = None if to_limit else self.next_target(value, "xy"[axis], direction)
        if t is None:
            lo, hi = self.limits(axis)
            t = lo if direction < 0 else hi
        self.cursor_to(axis, t)

    def swap_corner(self, both):
        """o: to the opposite corner; O: to the corner sideways."""
        if self.center:
            return  # no corners in centre mode
        self.cur[0] = OPPOSITE[self.cur[0]]
        if both:
            self.cur[1] = OPPOSITE[self.cur[1]]

    def toggle_center(self):
        self.center = not self.center
        self.cur = ["L", "T"]  # so h/k grow and l/j shrink in centre mode

    # --- moving the whole box -------------------------------------------------

    def move(self, dx, dy):
        x, y, w, h = self.rect
        self.set_rect([x + dx, y + dy, w, h])

    def step(self, d, px, moving):
        """One keyboard step: move the box, or move the cursor."""
        if moving:
            dx, dy = VEC[d]
            self.move(dx * px, dy * px)
        else:
            self.cursor_by(d, px)

    def next_target(self, value, axis, direction, targets=None, gap=GAP):
        # Nearest snap target past value in direction (+1/-1), ignoring any
        # within gap: for window edges, those are the far side of the gap
        # we're sitting in.
        targets = self.targets if targets is None else targets
        ts = [t for t in targets[axis] if (t - value) * direction > gap]
        if not ts:
            return None
        return min(ts) if direction > 0 else max(ts)

    def snap_move(self, d, targets=None, gap=GAP):
        """Slide until the leading or trailing edge snaps to the next of
        targets (window / monitor edges by default); slide() adds the
        quarter lines."""
        e = self.edges()
        dx, dy = VEC[d]
        axis, direction = ("x", dx) if dx else ("y", dy)
        a, b = ("L", "R") if dx else ("T", "B")
        best = None
        for edge in (a, b):
            t = self.next_target(e[edge], axis, direction, targets, gap)
            if t is not None and (best is None or abs(t - e[edge]) < abs(best)):
                best = t - e[edge]
        if best is not None:
            self.move(best * abs(dx), best * abs(dy))

    def grid(self):
        """Each monitor's edges and its quarter, half and three-quarter lines."""
        xs, ys = set(), set()
        for mx, my, mw, mh in self.monitors:
            xs.update(mx + mw * i / 4 for i in range(5))
            ys.update(my + mh * i / 4 for i in range(5))
        return {"x": sorted(xs), "y": sorted(ys)}

    def slide(self, d):
        """Ctrl+Shift+hjkl: slide the box (same size) until its leading or
        trailing edge lands on the next window / monitor edge or quarter line
        of a monitor. Lines within GAP are skipped: a box from a window is
        inset by Hyprland's gaps, so once one edge is on a line the other
        sits just shy of the next, and every other press would otherwise be
        a 1-3px hop. For the same reason, the leading edge is then pulled
        onto a line it lands just shy of."""
        grid = self.grid()
        targets = {a: sorted(set(self.targets[a]) | set(grid[a])) for a in ("x", "y")}
        self.snap_move(d, targets)
        dx, dy = VEC[d]
        axis = "x" if dx else "y"
        lead = self.edges()[d]
        direction = dx or dy  # only onward: never back from where it stopped
        near = [t for t in targets[axis] if 0 < (t - lead) * direction <= GAP]
        if near:
            t = min(near, key=lambda t: abs(t - lead))
            self.move((t - lead) * abs(dx), (t - lead) * abs(dy))

    def double(self, d):
        """Alt+Shift+hjkl: grow toward d by the box's own size, the inverse
        of halve (Alt+h then Alt+Shift+l is back where it started); it stops
        at the bounds."""
        e = self.edges()
        size = (e["R"] - e["L"]) if d in "LR" else (e["B"] - e["T"])
        e[d] += size * sum(VEC[d])
        lo, hi = self.limits(0 if d in "LR" else 1)
        e[d] = max(lo, min(e[d], hi))
        self.set_edges(e)

    def halve(self, d):
        """Alt+hjkl: keep the half toward d (keynav)."""
        e = self.edges()
        mid_x, mid_y = (e["L"] + e["R"]) / 2, (e["T"] + e["B"]) / 2
        e[OPPOSITE[d]] = mid_x if d in "LR" else mid_y
        self.set_edges(e)

    # --- windows and monitors -------------------------------------------------

    def window_at(self, px, py):
        for r in self.windows:  # focus-history order: topmost first
            if contains(r, px, py):
                return r
        return None

    @staticmethod
    def nearest(rects, px, py):
        # Containing the point wins (first = topmost); else the closest edge.
        hit = next((r for r in rects if contains(r, px, py)), None)
        return hit or min(rects, key=lambda r: dist(r, px, py))

    def zoom(self, step):
        """+ / - (and the wheel): the box's scope, nearest window <-> its
        monitor <-> the bounds; it stops at either end."""
        cx, cy = self.centre()
        steps = []
        if self.windows:
            steps.append(self.nearest(self.windows, cx, cy))
        steps.append(self.nearest(self.monitors, cx, cy))
        steps.append(list(self.bounds))
        # A fullscreen window is its monitor; one monitor is everything.
        steps = [s for i, s in enumerate(steps) if i == 0 or not same(s, steps[i - 1])]
        cur = next((i for i, s in enumerate(steps) if same(self.rect, s)), None)
        if cur is None:  # a box of its own: + widens to its window; - leaves it
            if step > 0:
                self.set_rect(steps[0])
        else:
            self.set_rect(steps[max(0, min(cur + step, len(steps) - 1))])

    def neighbour(self, d):
        """Ctrl+hjkl: select the nearest window that way (like SUPER+hjkl).

        In order of preference: windows beside the box; then windows the box
        sits inside (e.g. from a floating window back to the maximised one
        under it: skipping those outright made that a one-way trip); then
        monitors."""
        cx, cy = self.centre()
        dx, dy = VEC[d]
        beside = [r for r in self.windows if not contains(r, cx, cy)]
        around = [r for r in self.windows if contains(r, cx, cy)]
        for rects in (beside, around, self.monitors):
            best, best_score = None, None
            for r in rects:
                rx, ry = r[0] + r[2] / 2, r[1] + r[3] / 2
                along = (rx - cx) * dx + (ry - cy) * dy
                if along < 1 or same(r, self.rect):  # not that way, or already on it
                    continue
                perp = abs((rx - cx) * dy) + abs((ry - cy) * dx)
                score = along + 2 * perp
                if best is None or score < best_score:
                    best, best_score = r, score
            if best:
                self.set_rect(best)
                return

    def jump(self, d, moving):
        """Ctrl + direction: one-shot, no repeat. Ctrl selects the
        neighbouring window; Ctrl+Shift slides the box to the next edge."""
        if moving:
            self.slide(d)
        else:
            self.neighbour(d)

    def drag_to(self, x0, y0, gx, gy):
        """Mouse drag from (x0, y0): that's the anchor, the pointer the cursor.
        Both are held inside the bounds, so dragging past an edge stops the
        box there instead of sliding the anchor."""
        (lx, hx), (ly, hy) = self.limits(0), self.limits(1)
        x0, gx = (max(lx, min(v, hx)) for v in (x0, gx))
        y0, gy = (max(ly, min(v, hy)) for v in (y0, gy))
        self.set_rect([min(x0, gx), min(y0, gy), abs(gx - x0), abs(gy - y0)])
        self.cur = ["R" if gx >= x0 else "L", "B" if gy >= y0 else "T"]
        self.center = False
