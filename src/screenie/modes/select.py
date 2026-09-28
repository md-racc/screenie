"""Select: the normal mode. Edit the box, look at it, finish."""

from .. import draw
from ..app import Action
from ..geometry import STEP, VEC
from ..output import decode_code, notify
from .base import Mode

# Box repeat speed, in 5px steps per second: start, and top after the ramp.
RATES = (20, 300)


class Select(Mode):
    name = "select"
    outline_rgb = (1, 1, 1)

    def __init__(self, app, sel):
        super().__init__(app)
        self.sel = sel
        self.drag = None  # (x0, y0, moved?)

    @property
    def moving(self):
        return self.app.shift or self.app.space

    # --- box editing ----------------------------------------------------------

    def on_dir(self, d):
        a, sel = self.app, self.sel
        sel.remember()
        if a.ctrl:  # one-shot jumps, no repeat
            sel.jump(d, self.moving)
        elif a.alt:
            sel.halve(d)
        else:
            sel.step(d, STEP, self.moving)  # a tap is exactly one step

    def rates(self):
        return None if self.app.ctrl or self.app.alt else RATES

    def on_repeat(self, dirs, n):
        if self.moving:  # one combined vector, opposite keys cancel
            dx = sum(VEC[d][0] for d in dirs)
            dy = sum(VEC[d][1] for d in dirs)
            self.sel.move(dx * n * STEP, dy * n * STEP)
        else:
            for d in dirs:
                self.sel.step(d, n * STEP, False)

    def edit_key(self, name):
        """Box keys shared with Censor. Returns True if handled."""
        sel = self.sel
        if name in ("o", "O"):
            sel.swap_corner(both=name == "o")
        elif name in ("w", "b", "braceright", "braceleft"):
            sel.remember()
            sel.cursor_jump({"w": "R", "b": "L", "braceright": "B", "braceleft": "T"}[name])
        elif name in ("0", "dollar", "g", "G"):
            sel.remember()
            sel.cursor_jump({"0": "L", "dollar": "R", "g": "T", "G": "B"}[name], to_limit=True)
        elif name in ("Tab", "ISO_Left_Tab"):
            sel.remember()
            sel.next_window(-1 if name == "ISO_Left_Tab" or self.app.shift else 1)
        elif name == "a":
            sel.remember()
            sel.zoom_out()
        elif name == "c":
            sel.toggle_center()
        elif name == "u":
            sel.undo()
        elif name in ("z", "Z"):
            from .loupe import Loupe
            self.app.push(Loupe(self.app, sel))
        else:
            return self.app.view_key(name)
        return True

    def on_key(self, name):
        a, sel = self.app, self.sel
        if self.edit_key(name):
            return
        if name in ("i", "e", "x") and sel.empty():
            return
        if name == "i":
            from .insert import Insert
            a.push(Insert(a, sel))
        elif name == "e":
            from .enhance import Enhance
            a.push(Enhance(a, sel))
        elif name == "x":
            from .censor import Censor
            a.push(Censor(a, sel))
        elif name in ("Return", "KP_Enter", "y", "P"):
            if not sel.empty():
                kind = {"y": "ocr", "P": "pin"}.get(name, "image")
                a.finish(Action(kind, list(sel.rect)))
        elif name == "p":
            gx, gy = sel.cursor_point()
            px, py = a.frame.pixel_under(gx, gy, sel.cur[0] == "R" and not sel.center,
                                         sel.cur[1] == "B" and not sel.center)
            a.finish(Action("text", text="#%02x%02x%02x" % a.frame.rgb(px, py),
                            note="Colour copied"))
        elif name == "Q":
            code = None if sel.empty() else decode_code(a.frame.crop(sel.rect))
            if code:
                a.finish(Action("text", text=code, note="Code copied"))
            else:
                notify("No QR / barcode found", "Adjust the box and try again")
        elif name in ("Escape", "q"):
            a.finish(None)

    # --- mouse ----------------------------------------------------------------

    def on_press(self, button, gx, gy):
        if button == 1:
            self.sel.remember()
            self.drag = (gx, gy, False)
        elif button == 3:
            self.cancel()

    def cancel(self):
        self.app.finish(None)

    def on_motion(self, gx, gy):
        if not self.drag:
            return
        x0, y0, moved = self.drag
        if moved or abs(gx - x0) + abs(gy - y0) > 3:
            self.drag = (x0, y0, True)
            # Unrounded: the crop snaps to physical pixels, nothing coarser.
            self.sel.drag_to(x0, y0, gx, gy)

    def on_release(self, button, gx, gy):
        drag, self.drag = self.drag, None
        if button == 1 and drag and not drag[2]:  # a click: snap to that window
            r = self.sel.window_at(gx, gy)
            if r:
                self.sel.set_rect(r)

    # --- drawing --------------------------------------------------------------

    def draw(self, cr, mon):
        a, f, sel = self.app, self.app.frame, self.sel
        x, y, w, h = sel.rect
        draw.dim_outside(cr, a.layout.rect, sel.rect)
        if a.filter != "off" and w >= 1 and h >= 1:  # live filter inside the box
            cr.save()
            cr.rectangle(x, y, w, h)
            cr.clip()
            draw.in_frame(cr, f)
            f.paint(cr, f.to_image(sel.rect), a.filter, a.contrast)
            cr.restore()
        active = a.mode is self
        draw.outline(cr, sel.rect, (*self.outline_rgb, 0.9 if active else 0.4))
        _, _, pw, ph = f.to_image(sel.rect)
        draw.label(cr, x + 4, y + h - 4, self.label(pw, ph))
        if active:
            draw.marker(cr, *sel.cursor_point(), self.outline_rgb)

    def label(self, pw, ph):
        return f"{pw}×{ph}" + ("  centre" if self.sel.center else "") + self.app.tags()
