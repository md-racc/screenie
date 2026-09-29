"""BoxMode: editing a box (a Selection) with the keyboard and the mouse.

The shared base of Select and the build tools' boxes (censor, filter,
shape). It knows only about the box: the modifier grammar on h j k l, the
box keys, drag to draw, click to place. What a box is *for* (capturing,
censoring...) belongs to the mode built on it.
"""

from .. import draw
from ..geometry import STEP
from ..keys import EDGE_JUMPS, FAR_EDGES, LOUPE, held_vector
from .base import Mode

# Box repeat speed, in 5px steps per second: start, and top after the ramp.
RATES = (20, 300)


class BoxMode(Mode):
    def __init__(self, app, sel):
        super().__init__(app)
        self.sel = sel
        self.drag = None  # (x0, y0, moved?)

    @property
    def moving(self):
        return self.app.shift

    def changed(self):
        """The box changed (by a key or the mouse). Tools hook this."""

    # --- keyboard -------------------------------------------------------------

    def on_dir(self, d):
        a, sel = self.app, self.sel
        sel.remember()
        # The modifier grammar, the same in every box: Shift moves the whole
        # box; Ctrl jumps (the neighbouring window, or with Shift, slides to
        # the next edge); Alt scales (halve, or with Shift, double).
        if a.ctrl:  # one-shot jumps, no repeat
            sel.jump(d, self.moving)
        elif a.alt and a.shift:
            sel.double(d)
        elif a.alt:
            sel.halve(d)
        else:
            sel.step(d, STEP, self.moving)  # a tap is exactly one step
        self.changed()

    def rates(self):
        return None if self.app.ctrl or self.app.alt else RATES

    def on_repeat(self, dirs, n):
        if self.moving:  # one combined vector, opposite keys cancel
            dx, dy = held_vector(dirs)
            self.sel.move(dx * n * STEP, dy * n * STEP)
        else:
            for d in dirs:
                self.sel.step(d, n * STEP, False)
        self.changed()

    def edit_key(self, name):
        """The box keys. Returns True if handled."""
        a, sel = self.app, self.sel
        if name in ("o", "O", "space"):  # Space is o, Shift+Space is O
            sel.swap_corner(both=name == "o" or (name == "space" and not a.shift))
        elif name in EDGE_JUMPS:
            sel.remember()
            sel.cursor_jump(EDGE_JUMPS[name])
        elif name in FAR_EDGES:
            sel.remember()
            sel.cursor_jump(FAR_EDGES[name], to_limit=True)
        elif name == "c":
            sel.toggle_center()
        elif name == "u":
            sel.undo()
        elif name in LOUPE or name == "e":  # the lenses, from any box
            a.push(a.lens(name, a.layer_main or a.sel, at=sel.centre()))
            return True
        else:
            return False
        self.changed()
        return True

    # --- mouse ------------------------------------------------------------------

    def on_press(self, button, gx, gy):
        if button == 1:
            self.sel.remember()
            self.drag = (gx, gy, False)

    def on_motion(self, gx, gy):
        if not self.drag:
            return
        x0, y0, moved = self.drag
        if moved or abs(gx - x0) + abs(gy - y0) > 3:
            self.drag = (x0, y0, True)
            # Unrounded: the crop snaps to physical pixels, nothing coarser.
            self.sel.drag_to(x0, y0, gx, gy)
            self.changed()

    def on_release(self, button, gx, gy):
        drag, self.drag = self.drag, None
        if button == 1 and drag and not drag[2]:
            self.click(gx, gy)
            self.changed()

    def click(self, gx, gy):
        """A left click (no drag) places: the box's cursor corner moves there
        (the anchor stays), resizing the box. Select snaps to windows
        instead."""
        sel = self.sel
        if sel.center:
            sel.toggle_center()
        sel.cursor_to(0, gx)
        sel.cursor_to(1, gy)

    def reset_pointer(self):
        self.drag = None

    # --- drawing ----------------------------------------------------------------

    def draw_box(self, cr):
        draw.outline(cr, self.sel.rect)
        if self.app.mode is self:
            draw.marker(cr, *self.sel.cursor_point())

    def cursor_px(self):
        """The pixel under the box's cursor corner (the one just inside the
        box on its far edges), or its centre in centre mode."""
        sel = self.sel
        if sel.center:
            return self.app.frame.pixel_under(*sel.centre())
        gx, gy = sel.cursor_point()
        return self.app.frame.edge_pixel(gx, gy, sel.cur[0] == "R", sel.cur[1] == "B")

    def size_status(self):
        _, _, pw, ph = self.app.frame.to_image(self.sel.rect)
        return f"{pw}×{ph}" + ("  centre" if self.sel.center else "")
