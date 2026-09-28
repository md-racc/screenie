"""Loupe: z toggles it. A pixel cursor of its own, spawned dead centre in the
box; h j k l move it one pixel per press, Shift+h j k l one loupe-width
(n pixels for an n x n loupe); hold either to repeat. It never leaves the
box; a click puts it under the pointer and a drag drags it. + / - resize
the loupe window, p copies the colour under it, and x starts a censor
right there. z, Esc or right-click turns it off."""

from .. import draw
from ..app import Action
from ..geometry import VEC
from .base import Mode

ZOOM = 10                  # screen px per frame pixel
SIZE_MIN, SIZE_MAX = 9, 61  # loupe window, frame pixels across (kept odd)
RATES = (15, 200)          # pixels per second while held: start, top
PAGE_RATES = (4, 12)       # loupe-widths per second with Shift held


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
        dx, dy = VEC[d]
        self.move(dx * self.step(), dy * self.step())

    def rates(self):
        return PAGE_RATES if self.app.shift else RATES

    def on_repeat(self, dirs, n):
        dx = sum(VEC[d][0] for d in dirs)
        dy = sum(VEC[d][1] for d in dirs)
        self.move(dx * n * self.step(), dy * n * self.step())

    def on_key(self, name):
        a = self.app
        if name in ("plus", "equal", "KP_Add"):
            a.loupe_px = min(SIZE_MAX, a.loupe_px + 4)
        elif name in ("minus", "KP_Subtract"):
            a.loupe_px = max(SIZE_MIN, a.loupe_px - 4)
        elif name == "p":
            a.finish(Action("text", text="#%02x%02x%02x" % a.frame.rgb(self.px, self.py),
                            note="Colour copied"))
        elif name == "x" and self.sel is a.sel:  # not from inside a censor
            from .censor import Censor
            a.push(Censor(a, self.sel, at=a.frame.to_global(self.px, self.py)))
        elif name in ("z", "Z", "Escape"):
            a.pop()

    # Mouse: a click jumps the pixel cursor there, a drag drags it.
    def on_press(self, button, gx, gy):
        if button == 1:
            self.dragging = True
            self.move_to(gx, gy)
        elif button == 3:
            self.app.pop()

    def on_motion(self, gx, gy):
        if self.dragging:
            self.move_to(gx, gy)

    def on_release(self, button, gx, gy):
        self.dragging = False

    def move_to(self, gx, gy):
        px, py = self.app.frame.pixel_under(gx, gy)
        self.move(px - self.px, py - self.py)

    def draw(self, cr, mon):
        a, f = self.app, self.app.frame
        if a.mode is not self:  # e.g. a censor started here: don't cover it
            return
        gx, gy = f.to_global(self.px, self.py)

        # The pixel cursor in the box: a crosshair with a gap for the pixel.
        cr.set_source_rgba(1, 0.2, 0.2, 1)
        cr.set_line_width(1)
        for (x0, y0, x1, y1) in ((gx - 9, gy, gx - 3, gy), (gx + 3, gy, gx + 9, gy),
                                 (gx, gy - 9, gx, gy - 3), (gx, gy + 3, gx, gy + 9)):
            cr.move_to(x0, y0)
            cr.line_to(x1, y1)
        cr.stroke()

        n = a.loupe_px
        size = n * ZOOM
        W, H = a.layout.rect[2], a.layout.rect[3]
        ox, oy = a.layout.rect[0], a.layout.rect[1]
        # Beside the cursor, flipped to stay inside the layout.
        lx = gx + 24 if gx + 24 + size < ox + W else gx - 24 - size
        ly = gy + 24 if gy + 24 + size + 28 < oy + H else gy - 24 - size - 28
        cr.save()
        cr.rectangle(lx, ly, size, size)
        cr.clip()
        cr.set_source_rgb(0, 0, 0)
        cr.paint()
        cr.translate(lx, ly)
        cr.scale(ZOOM, ZOOM)
        cr.translate(-(self.px - n // 2), -(self.py - n // 2))  # frame pixel coords
        f.paint(cr, f.to_image(self.sel.rect), a.filter, a.contrast, sharp=True)
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
        cr.set_source_rgba(1, 0.2, 0.2, 1)  # the pixel under the cursor
        cr.rectangle(lx + c + 0.5, ly + c + 0.5, ZOOM - 1, ZOOM - 1)
        cr.stroke()
        draw.outline(cr, (lx, ly, size, size))

        r, g, b = f.rgb(self.px, self.py)
        cr.set_source_rgb(r / 255, g / 255, b / 255)  # swatch + hex
        cr.rectangle(lx, ly + size + 4, 20, 20)
        cr.fill()
        draw.label(cr, lx + 24, ly + size + 24,
                   f"#{r:02x}{g:02x}{b:02x}  {self.px},{self.py}  {n}px")

