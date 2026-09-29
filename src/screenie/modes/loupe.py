"""Loupe: z toggles it. A pixel cursor of its own, spawned dead centre in the
box; h j k l move it one pixel per press, Shift+h j k l one loupe-width
(n pixels for an n x n loupe); hold either to repeat. It never leaves the
box; a click puts it under the pointer and a drag drags it. + / - (and the
wheel) resize the loupe window, v / V cycle the view filter, p copies the
colour under it, e opens enhance there, and x / a / f start a build tool
right there. z, Enter or Esc turns it off.

The magnifier is placed like a tag (draw.place): beside the cursor, off the
box if it can be, and always wholly on one monitor."""

import cairo

from .. import draw
from ..app import Action
from ..geometry import VEC
from ..keys import AMOUNT, CONFIRM, INC, LOUPE, held_vector, hex_colour
from .base import Mode

ZOOM = 10                  # screen px per frame pixel
SIZE_MIN, SIZE_MAX = 9, 61  # loupe window, frame pixels across (kept odd)
RATES = (15, 200)          # pixels per second while held: start, top
PAGE_RATES = (4, 12)       # loupe-widths per second with Shift held
CHIP = 28                  # room under the magnifier for its colour chip
SPOTS = ("below-l", "right-t", "above-l", "left-t", "right-b", "left-b")


class Loupe(Mode):
    name = "loupe"

    def __init__(self, app, sel, at_px=None):
        super().__init__(app)
        self.sel = sel
        f = app.frame
        bx, by, bw, bh = f.to_image(sel.rect)
        self.box = (bx, by, max(1, bw), max(1, bh))
        self.px, self.py = at_px or (bx + self.box[2] // 2, by + self.box[3] // 2)
        self.move(0, 0)  # clamp into the box
        self.dragging = False

    def move(self, dx, dy):
        bx, by, bw, bh = self.box
        self.px = max(bx, min(self.px + dx, bx + bw - 1))
        self.py = max(by, min(self.py + dy, by + bh - 1))

    def step(self):
        """Pixels per step: 1, or a whole loupe-width with Shift."""
        return self.app.loupe_px if self.app.shift else 1

    def on_dir(self, d):
        if self.app.ctrl or self.app.alt:
            return  # no jumps or scaling for a pixel cursor
        dx, dy = VEC[d]
        self.move(dx * self.step(), dy * self.step())

    def rates(self):
        if self.app.ctrl or self.app.alt:
            return None
        return PAGE_RATES if self.app.shift else RATES

    def on_repeat(self, dirs, n):
        dx, dy = held_vector(dirs)
        self.move(dx * n * self.step(), dy * n * self.step())

    def repeatable(self, name):
        return name in AMOUNT

    def on_key(self, name):
        a = self.app
        at = a.frame.to_global(self.px, self.py)
        if name in AMOUNT:
            a.loupe_px = max(SIZE_MIN, min(SIZE_MAX, a.loupe_px + (4 if name in INC else -4)))
        elif name == "p":
            a.finish(Action("text", text=hex_colour(a.frame.rgb(self.px, self.py)),
                            note="Colour copied"))
        elif name in ("x", "a", "f") and self.sel is (a.layer_main or a.sel):
            a.start_tool(a.tool(name, self.sel, at=at))
        elif name == "e":
            a.push(a.lens("e", self.sel, at=at))
        elif name in ("v", "V"):  # view filter: only changes what you see here
            a.view_key(name)
        elif name in LOUPE or name == "Escape" or name in CONFIRM:
            a.pop()

    # Mouse: a click jumps the pixel cursor there, a drag drags it.
    def on_press(self, button, gx, gy):
        if button == 1:
            self.dragging = True
            self.move_to(gx, gy)

    def on_motion(self, gx, gy):
        if self.dragging:
            self.move_to(gx, gy)

    def on_release(self, button, gx, gy):
        self.dragging = False

    def reset_pointer(self):
        self.dragging = False

    def cursor_px(self):
        return self.px, self.py

    def move_to(self, gx, gy):
        px, py = self.app.frame.pixel_under(gx, gy)
        self.move(px - self.px, py - self.py)

    def status(self):
        return f"{self.app.loupe_px}px{self.app.tags()}"

    def legend(self):
        return ["hjkl 1px  Shift a loupe-width  +/- size  v view filter  click/drag place",
                "p colour  e enhance  x censor  a annotate  f filter  z/Enter/Esc off"]

    # --- drawing ------------------------------------------------------------------

    def occupied(self):
        """The magnifier and its chip (tags keep clear of them)."""
        if self.app.mode is not self:
            return []
        gx, gy = self.app.frame.to_global(self.px, self.py)
        size = self.app.loupe_px * ZOOM
        x, y = self.where(gx, gy, size)
        return [(x, y, size, size + CHIP)]

    def where(self, gx, gy, size):
        """The magnifier's top-left: beside the cursor, off the box if it can
        be, on the cursor's monitor."""
        a = self.app
        around = [gx - 12, gy - 12, 24, 24]
        bw, bh = size, size + CHIP
        x, y = draw.place(around, bw, bh, SPOTS, a.monitor_rects, avoid=[self.sel.rect])
        if draw._overlaps((x, y, bw, bh), self.sel.rect):  # no spot off the box
            x, y = draw.place(around, bw, bh, SPOTS, a.monitor_rects)
        return x, y

    def draw(self, cr, mon):
        a, f = self.app, self.app.frame
        if a.mode is not self:  # e.g. a censor started here: don't cover it
            return
        gx, gy = f.to_global(self.px, self.py)
        draw.pixel_cursor(cr, f, self.px, self.py)  # the same cursor as insert

        n = a.loupe_px
        size = n * ZOOM
        lx, ly = self.where(gx, gy, size)
        cr.save()
        cr.rectangle(lx, ly, size, size)
        cr.clip()
        cr.set_source_rgb(0, 0, 0)
        cr.paint()
        cr.translate(lx, ly)
        cr.scale(ZOOM, ZOOM)
        cr.translate(-(self.px - n // 2), -(self.py - n // 2))  # frame pixel coords
        f.paint(cr, f.to_image(self.sel.rect), a.filter, sharp=True)
        cr.restore()

        cr.set_line_width(1)
        cr.set_source_rgba(1, 1, 1, 0.12)  # pixel grid
        for i in range(1, n):
            cr.move_to(lx + i * ZOOM + 0.5, ly)
            cr.line_to(lx + i * ZOOM + 0.5, ly + size)
            cr.move_to(lx, ly + i * ZOOM + 0.5)
            cr.line_to(lx + size, ly + i * ZOOM + 0.5)
        cr.stroke()
        c = n // 2 * ZOOM
        cr.save()  # the pixel under the cursor, outlined inverted like the cursor
        cr.set_operator(cairo.OPERATOR_DIFFERENCE)
        cr.set_source_rgb(1, 1, 1)
        cr.rectangle(lx + c + 0.5, ly + c + 0.5, ZOOM - 1, ZOOM - 1)
        cr.stroke()
        cr.restore()
        draw.outline(cr, (lx, ly, size, size))

        r, g, b = f.rgb(self.px, self.py)
        end = draw.label(cr, lx, ly + size + CHIP, f"{hex_colour((r, g, b))}  ({self.px},{self.py})")
        draw.swatch(cr, end + 5, ly + size + CHIP - 2, (r / 255, g / 255, b / 255))
