"""Enhance: entered with e. A lens (a sub-window) appears inside the box and
shows what's under it magnified, with the filters and sharp / smooth pixels
applied. Everything stays inside the box: the lens can't leave it, and the
lens never shows anything from outside it.

Shift+h j k l move the lens, j / k zoom in / out (hold either to keep
going), a mouse drag draws a new lens. Esc returns."""

import math

from .. import draw
from ..geometry import STEP, VEC
from .base import Mode

ZOOM_START = 2.0
ZOOM_TAP = 0.5         # a tap moves to the next multiple of this
ZOOM_STEP = 1.03       # factor per repeat step while held
ZOOM_DELAY = 0.25      # hold this long before zoom starts repeating
ZOOM_MIN, ZOOM_MAX = 1.0, 64
MOVE_RATES, ZOOM_RATES = (20, 300), (20, 60)
LENS_RGB = (0.3, 0.8, 1)


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

    def zoom_by(self, factor):
        self.zoom = max(ZOOM_MIN, min(self.zoom * factor, ZOOM_MAX))

    def zoom_tap(self, up):
        """To the next ZOOM_TAP mark: 2 -> 2.5 -> 3, and 2.73 -> 3."""
        marks = self.zoom / ZOOM_TAP
        mark = math.floor(marks + 1e-6) + 1 if up else math.ceil(marks - 1e-6) - 1
        self.zoom = max(ZOOM_MIN, min(mark * ZOOM_TAP, ZOOM_MAX))

    def on_dir(self, d):
        if self.app.shift:
            dx, dy = VEC[d]
            self.lens.move(dx * STEP, dy * STEP)
        elif d in "BT":    # j in, k out
            self.zoom_tap(up=d == "B")

    def rates(self):
        return MOVE_RATES if self.app.shift else ZOOM_RATES

    def repeat_delay(self):
        # A tap must stay one clean 0.5 step, so zoom waits longer to repeat.
        return super().repeat_delay() if self.app.shift else ZOOM_DELAY

    def on_repeat(self, dirs, n):
        if self.app.shift:
            dx = sum(VEC[d][0] for d in dirs)
            dy = sum(VEC[d][1] for d in dirs)
            self.lens.move(dx * n * STEP, dy * n * STEP)
        else:
            if "B" in dirs:
                self.zoom_by(ZOOM_STEP ** n)
            if "T" in dirs:
                self.zoom_by(ZOOM_STEP ** -n)

    def on_key(self, name):
        if self.app.view_key(name):
            return
        if name == "Escape":
            self.app.pop()

    def on_press(self, button, gx, gy):
        if button == 1:
            self.drag = (gx, gy)
        elif button == 3:
            self.app.pop()

    def on_motion(self, gx, gy):
        if self.drag and abs(gx - self.drag[0]) + abs(gy - self.drag[1]) > 3:
            self.lens.drag_to(*self.drag, gx, gy)

    def on_release(self, button, gx, gy):
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
        f.paint(cr, f.to_image(self.main.rect), a.filter, a.contrast, sharp=a.sharp)
        cr.restore()
        draw.outline(cr, self.lens.rect, (*LENS_RGB, 0.9))
        draw.label(cr, x, y + h + 24,
                   f"enhance {self.zoom:.1f}x  {'sharp' if a.sharp else 'smooth'}{a.tags()}")
