"""Insert: entered with i, like vim. The box stays put and a free cursor (an
inverted 3x3 block on one frame pixel, in a 15x15 square) appears at its
centre. Move it anywhere inside the box, then open the tools right there:
z loupe, e enhance, x censor (its box starts with the cursor as its
top-left), a annotate, f filter, p colour. Every tool comes back here,
cursor where it was; coming back from the loupe, the cursor follows where
the loupe went. Enter or Esc returns to the box.

    h j k l          one step, 5 pixels (hold to speed up); the loupe is for precision
    + / -            the step, 1-20 pixels (the wheel too)
    Shift+h j k l    four steps
    Alt+h j k l      halfway to the box edge (keynav-style)
    u                back to where the cursor was
    w b } {          next window / monitor edge right / left / down / up (or the box edge)
    0 $ g G          the box's left / right / top / bottom edge
    m                the box's centre
    click            put the cursor there
"""

from .. import draw
from ..app import Action
from ..geometry import VEC
from ..keys import AMOUNT, CONFIRM, EDGE_JUMPS, FAR_EDGES, INC, LOUPE, held_vector, hex_colour
from .base import Mode

STEP, STEP_MIN, STEP_MAX = 5, 1, 20  # a tap, frame pixels (+ / - change it)
BIG = 4                              # Shift: this many steps
# Held-key speed. The top speed scales with the box, along the axis you're
# moving on: at full speed, crossing the box takes about CROSS seconds.
START_PX = 40              # px/s when repeat kicks in
CROSS = 1.0                # s to cross the box at top speed
TOP_MIN, TOP_MAX = 40, 800  # px/s bounds on the top speed


class Insert(Mode):
    name = "insert"

    def __init__(self, app, sel):
        super().__init__(app)
        self.sel = sel
        bx, by, bw, bh = app.frame.to_image(sel.rect)
        self.box = (bx, by, max(1, bw), max(1, bh))
        self.px, self.py = bx + self.box[2] // 2, by + self.box[3] // 2
        self.stepsize = STEP
        self.trail = []  # where the cursor was, for u

    # --- moving -----------------------------------------------------------------

    def put(self, px, py, remember=True):
        if remember:
            self.trail.append((self.px, self.py))
        bx, by, bw, bh = self.box
        self.px = max(bx, min(round(px), bx + bw - 1))
        self.py = max(by, min(round(py), by + bh - 1))

    def step(self):
        return self.stepsize * (BIG if self.app.shift else 1)

    def box_edge(self, d):
        bx, by, bw, bh = self.box
        return {"L": bx, "R": bx + bw - 1, "T": by, "B": by + bh - 1}[d]

    def jump(self, d):
        """To the next window / monitor edge that way, or the box edge."""
        f = self.app.frame
        gx, gy = f.to_global(self.px, self.py)
        axis = 0 if d in "LR" else 1
        direction = sum(VEC[d])
        t = self.sel.next_target((gx, gy)[axis], "xy"[axis], direction)
        if t is None:
            target = self.box_edge(d)
        else:
            origin = (f.ox, f.oy)[axis]
            # the pixel just inside the edge we're heading for
            target = int((t - origin) * f.k) - (1 if direction > 0 else 0)
            if (target - self.box_edge(d)) * direction > 0:
                target = self.box_edge(d)
        if axis == 0:
            self.put(target, self.py)
        else:
            self.put(self.px, target)

    def halve(self, d):
        """Halfway to the box edge that way."""
        edge = self.box_edge(d)
        if d in "LR":
            self.put((self.px + edge) / 2, self.py)
        else:
            self.put(self.px, (self.py + edge) / 2)

    def on_dir(self, d):
        a = self.app
        if a.ctrl or (a.alt and a.shift):
            return  # nothing for these here (edge jumps are w b } {)
        if a.alt:
            self.halve(d)
        else:
            dx, dy = VEC[d]
            self.put(self.px + dx * self.step(), self.py + dy * self.step())

    def rates(self):
        a = self.app
        if a.ctrl or a.alt:
            return None
        _, _, bw, bh = self.box
        dirs = {d for d, _ in a.held.values()}
        extent = max(bw if dirs & {"L", "R"} else 0, bh if dirs & {"T", "B"} else 0)
        top = max(TOP_MIN, min(extent / CROSS, TOP_MAX))  # px/s
        start = min(START_PX, top)
        step = self.step()  # rates are in steps/s
        return start / step, top / step

    def on_repeat(self, dirs, n):
        dx, dy = held_vector(dirs)
        s = n * self.step()
        self.put(self.px + dx * s, self.py + dy * s, remember=False)

    def on_resume(self, child):
        if child.name == "loupe":  # the cursor follows the loupe
            self.put(child.px, child.py)

    # --- keys -------------------------------------------------------------------

    def repeatable(self, name):
        return name in AMOUNT

    def on_key(self, name):
        a, sel = self.app, self.sel
        at = a.frame.to_global(self.px, self.py)
        if name in EDGE_JUMPS:
            self.jump(EDGE_JUMPS[name])
        elif name in FAR_EDGES:
            d = FAR_EDGES[name]
            if d in "LR":
                self.put(self.box_edge(d), self.py)
            else:
                self.put(self.px, self.box_edge(d))
        elif name == "m":
            bx, by, bw, bh = self.box
            self.put(bx + bw // 2, by + bh // 2)
        elif name in AMOUNT:  # the step size
            self.stepsize = max(STEP_MIN, min(STEP_MAX, self.stepsize + (1 if name in INC else -1)))
        elif name == "u":
            if self.trail:
                self.px, self.py = self.trail.pop()
        elif name in LOUPE or name == "e":
            a.push(a.lens(name, sel, at=at))
        elif name in ("x", "a", "f"):  # a build tool (opens a layer if none is open)
            f = a.frame  # the cursor pixel's top-left corner, in global coords
            corner = (f.ox + self.px / f.k, f.oy + self.py / f.k)
            a.start_tool(a.tool(name, sel, corner=corner), main=sel)
        elif name == "p":
            a.finish(Action("text", text=hex_colour(a.frame.rgb(self.px, self.py)),
                            note="Colour copied"))
        elif name == "Escape" or name in CONFIRM:  # Enter (right click) too
            a.pop()

    # --- mouse ------------------------------------------------------------------

    def on_press(self, button, gx, gy):
        if button == 1:
            self.put(*self.app.frame.pixel_under(gx, gy))

    # --- drawing ----------------------------------------------------------------

    def draw(self, cr, mon):
        a, f = self.app, self.app.frame
        if a.mode is not self:  # a tool opened here draws its own cursor
            return
        draw.pixel_cursor(cr, f, self.px, self.py)

    def cursor_px(self):
        return self.px, self.py

    def status(self):
        return f"step {self.stepsize}px"

    def legend(self):
        return ["hjkl move  +/- step  Shift ×4  Alt half to edge  w b } { edges",
                "0 $ g G box sides  m middle  u back  click place",
                "z loupe  e enhance  x censor  a annotate  f filter  p colour  Enter/Esc back"]
