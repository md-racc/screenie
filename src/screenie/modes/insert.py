"""Insert: entered with i, like vim. The box stays put and a free cursor (an
inverted 3x3 block on one frame pixel, in a 15x15 square) appears at its
centre. Move it anywhere inside
the box, then open the tools right there: z loupe, e enhance, x censor (its box
starts with the cursor as its top-left), p colour. Every tool comes back
here, cursor where it was. Coming back from the loupe, the cursor follows where the loupe
went. Esc returns to the box.

    h j k l          5 pixels (hold to speed up); the loupe is for precision
    Shift+h j k l    20 pixels
    Ctrl+h j k l     to the next window / monitor edge (or the box edge)
    Alt+h j k l      halfway to the box edge (keynav-style)
    w b } {          next edge right / left / down / up (same as Ctrl)
    0 $ g G          the box's left / right / top / bottom edge
    m                the box's centre
    click            put the cursor there
"""

import cairo

from .. import draw
from ..app import Action
from ..geometry import VEC
from .base import Mode

STEP = 5                   # a tap, frame pixels: ballpark; the loupe does precision
BIG_STEP = 20              # Shift step, frame pixels
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

    # --- moving -----------------------------------------------------------------

    def put(self, px, py):
        bx, by, bw, bh = self.box
        self.px = max(bx, min(round(px), bx + bw - 1))
        self.py = max(by, min(round(py), by + bh - 1))

    def step(self):
        return BIG_STEP if self.app.shift else STEP

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
        if a.ctrl:
            self.jump(d)
        elif a.alt:
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
        step = self.step()  # rates are in steps/s: STEP, or BIG_STEP with Shift
        return start / step, top / step

    def on_repeat(self, dirs, n):
        dx = sum(VEC[d][0] for d in dirs)
        dy = sum(VEC[d][1] for d in dirs)
        s = n * self.step()
        self.put(self.px + dx * s, self.py + dy * s)

    def on_resume(self, child):
        if child.name == "loupe":  # the cursor follows the loupe
            self.put(child.px, child.py)

    # --- keys -------------------------------------------------------------------

    def on_key(self, name):
        a, sel = self.app, self.sel
        at = a.frame.to_global(self.px, self.py)
        if a.view_key(name):
            return
        if name in ("w", "b", "braceright", "braceleft"):
            self.jump({"w": "R", "b": "L", "braceright": "B", "braceleft": "T"}[name])
        elif name in ("0", "dollar", "g", "G"):
            d = {"0": "L", "dollar": "R", "g": "T", "G": "B"}[name]
            if d in "LR":
                self.put(self.box_edge(d), self.py)
            else:
                self.put(self.px, self.box_edge(d))
        elif name == "m":
            bx, by, bw, bh = self.box
            self.put(bx + bw // 2, by + bh // 2)
        elif name in ("z", "Z"):
            from .loupe import Loupe
            a.push(Loupe(a, sel, at_px=(self.px, self.py)))
        elif name == "e":
            from .enhance import Enhance
            a.push(Enhance(a, sel, at=at))
        elif name == "x":
            from .censor import Censor
            f = a.frame  # the cursor pixel's top-left corner, in global coords
            a.push(Censor(a, sel, corner=(f.ox + self.px / f.k, f.oy + self.py / f.k)))
        elif name == "p":
            a.finish(Action("text", text="#%02x%02x%02x" % a.frame.rgb(self.px, self.py),
                            note="Colour copied"))
        elif name == "Escape":
            a.pop()

    # --- mouse ------------------------------------------------------------------

    def on_press(self, button, gx, gy):
        if button == 1:
            self.put(*self.app.frame.pixel_under(gx, gy))
        elif button == 3:
            self.app.pop()

    # --- drawing ----------------------------------------------------------------

    def draw(self, cr, mon):
        a, f = self.app, self.app.frame
        if a.mode is not self:  # a tool opened here draws its own cursor
            return
        x, y, w, h = self.sel.rect
        # In frame pixels, so it sits exactly on the pixel grid at any scale,
        # and inverted (DIFFERENCE with white) so it shows on any colour:
        # a solid 3x3 on the cursor pixel, and a hollow 15x15 around it.
        cr.save()
        draw.in_frame(cr, f)
        cr.set_operator(cairo.OPERATOR_DIFFERENCE)
        cr.set_source_rgb(1, 1, 1)
        cr.rectangle(self.px - 1, self.py - 1, 3, 3)
        cr.fill()
        cr.set_line_width(1)
        cr.rectangle(self.px - 7 + 0.5, self.py - 7 + 0.5, 14, 14)
        cr.stroke()
        cr.restore()
        bx, by, _, _ = self.box
        draw.label(cr, x, y + h + 24, f"insert  {self.px - bx},{self.py - by}{a.tags()}")
