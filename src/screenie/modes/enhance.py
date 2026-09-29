"""Enhance: entered with e. A lens (a sub-window) appears inside the box and
shows what's under it magnified, with the view filter and sharp / smooth
pixels applied. Everything stays inside the box: the lens can't leave it,
and the lens never shows anything from outside it.

    h j k l          move the lens (Shift: bigger steps; hold to keep going)
    Alt / Alt+Shift  halve / double the lens toward that side
    click            centre the lens there
    z                the loupe, at the lens centre
    + / -            zoom in / out: a tap moves to the next 0.5 (2 -> 2.5 -> 3);
                     hold, or the wheel, to keep going
    v / V, s         view filter, sharp / smooth pixels
    drag             draw a new lens
    Enter / Esc      back
"""

import math

from .. import draw
from ..geometry import STEP, VEC
from ..keys import AMOUNT, CONFIRM, INC, LOUPE, held_vector
from .base import Mode

ZOOM_START = 2.0
ZOOM_TAP = 0.5         # a tap moves to the next multiple of this
ZOOM_MIN, ZOOM_MAX = 1.0, 64
MOVE_RATES = (20, 300)
BIG = 4  # Shift: this many steps at a time


class Enhance(Mode):
    name = "enhance"

    def __init__(self, app, main, at=None):
        super().__init__(app)
        self.main = main
        _, _, w, h = main.rect
        cx, cy = at or main.centre()
        self.lens = app.selection([cx - w / 4, cy - h / 4, w / 2, h / 2], main.rect)
        self.zoom = ZOOM_START
        self.drag = None

    def zoom_tap(self, up):
        """To the next ZOOM_TAP mark: 2 -> 2.5 -> 3, and 2.73 -> 3."""
        marks = self.zoom / ZOOM_TAP
        mark = math.floor(marks + 1e-6) + 1 if up else math.ceil(marks - 1e-6) - 1
        self.zoom = max(ZOOM_MIN, min(mark * ZOOM_TAP, ZOOM_MAX))

    def step(self):
        return STEP * (BIG if self.app.shift else 1)

    def on_dir(self, d):
        a = self.app
        if a.ctrl:
            return  # nothing to jump to
        if a.alt and a.shift:
            self.lens.double(d)
        elif a.alt:
            self.lens.halve(d)
        else:
            dx, dy = VEC[d]
            self.lens.move(dx * self.step(), dy * self.step())

    def rates(self):
        return None if self.app.ctrl or self.app.alt else MOVE_RATES

    def on_repeat(self, dirs, n):
        dx, dy = held_vector(dirs)
        self.lens.move(dx * n * self.step(), dy * n * self.step())

    def repeatable(self, name):
        return name in AMOUNT

    def on_key(self, name):
        a = self.app
        if a.view_key(name):
            return
        if name in AMOUNT:
            self.zoom_tap(up=name in INC)
        elif name in LOUPE:
            a.push(a.lens(name, self.main, at=self.lens.centre()))
        elif name == "Escape" or name in CONFIRM:
            a.pop()

    def on_press(self, button, gx, gy):
        if button == 1:
            self.drag = (gx, gy, False)

    def on_motion(self, gx, gy):
        if not self.drag:
            return
        x0, y0, moved = self.drag
        if moved or abs(gx - x0) + abs(gy - y0) > 3:
            self.drag = (x0, y0, True)
            self.lens.drag_to(x0, y0, gx, gy)

    def on_release(self, button, gx, gy):
        drag, self.drag = self.drag, None
        if button == 1 and drag and not drag[2]:  # a click centres the lens there
            x, y, w, h = self.lens.rect
            self.lens.move(gx - (x + w / 2), gy - (y + h / 2))

    def reset_pointer(self):
        self.drag = None

    def draw(self, cr, mon):
        a, f = self.app, self.app.frame
        x, y, w, h = self.lens.rect
        if w < 1 or h < 1:
            return
        cx, cy = x + w / 2, y + h / 2
        cr.save()
        cr.rectangle(x, y, w, h)
        cr.clip()
        cr.set_source_rgb(0, 0, 0)
        cr.paint()
        # Magnify around the lens centre, then clip to the box so nothing
        # from outside the selection shows.
        cr.translate(cx, cy)
        cr.scale(self.zoom, self.zoom)
        cr.translate(-cx, -cy)
        cr.rectangle(*self.main.rect)
        cr.clip()
        draw.in_frame(cr, f)
        f.paint(cr, f.to_image(self.main.rect), a.filter, sharp=a.sharp)
        cr.restore()
        draw.outline(cr, self.lens.rect)

    def status(self):
        a = self.app
        return f"{self.zoom:.1f}x {'sharp' if a.sharp else 'smooth'}{a.tags()}"

    def legend(self):
        return ["hjkl move lens  Shift faster  Alt / Alt+Shift halve / double  +/- zoom",
                "v view filter  s sharp/smooth  drag new lens  click centre  z loupe  Enter/Esc back"]
