"""Select: the base mode. Edit the box, look at it, finish."""

from .. import draw
from ..app import Action
from ..keys import AMOUNT, CONFIRM, INC, hex_colour
from ..output import decode_code, notify
from .box import BoxMode


class Select(BoxMode):
    name = "select"

    # --- keys -------------------------------------------------------------------

    def on_key(self, name):
        a, sel = self.app, self.sel
        if self.edit_key(name):
            return
        needs_box = ("i", "e", "x", "a", "f", "y", "P", "Q", *CONFIRM)
        if name in needs_box and not self.capturable():
            a.note = "empty box"
            return
        if name in AMOUNT:
            # the box's scope: window <-> monitor <-> everything (the wheel too)
            sel.remember()
            sel.zoom(1 if name in INC else -1)
        elif len(name) == 1 and name in "123456789":  # the layer view, at that layer
            if a.frame.layers:
                a.push(a.make("layers", sel, index=int(name) - 1))
            else:
                a.note = "no layers yet"
        elif name == "i":
            a.push(a.make("insert", sel))
        elif name in ("x", "a", "f"):  # a build tool: opens a layer
            a.start_tool(a.tool(name, sel))
        elif name in (*CONFIRM, "y", "P"):
            kind = {"y": "ocr", "P": "pin"}.get(name, "image")
            a.finish(Action(kind, list(sel.rect)))
        elif name == "p":
            a.finish(Action("text", text=hex_colour(a.frame.rgb(*self.cursor_px())),
                            note="Colour copied"))
        elif name == "Q":
            code = decode_code(a.frame.crop(sel.rect))
            if code:
                a.finish(Action("text", text=code, note="Code copied"))
            else:
                notify("No QR / barcode found", "Adjust the box and try again")
        elif name == "Escape":
            a.finish(None)

    def capturable(self):
        """At least one output pixel each way (a box can be under a pixel
        wide, from halving or a tiny drag)."""
        _, _, pw, ph = self.app.frame.to_image(self.sel.rect)
        return pw >= 1 and ph >= 1

    # --- mouse ------------------------------------------------------------------

    def click(self, gx, gy):
        """A click snaps to the window under the pointer, or the monitor if
        there's no window there."""
        r = self.sel.window_at(gx, gy) or self.sel.nearest(self.sel.monitors, gx, gy)
        self.sel.set_rect(r)

    # --- drawing ------------------------------------------------------------------

    def draw(self, cr, mon):
        a, sel = self.app, self.sel
        draw.dim_outside(cr, a.layout.rect, sel.rect)
        active = a.mode is self
        draw.outline(cr, sel.rect, (1, 1, 1, 0.9 if active else 0.5))
        if active:
            draw.marker(cr, *sel.cursor_point())

    def draw_status(self, cr, avoid=()):
        """The one status tag, on the bottom of the main box (see
        draw.place). The App draws it after every mode, so previews never
        cover it. Returns its rect, so the tags placed after it keep clear."""
        a = self.app
        _, _, pw, ph = a.frame.to_image(self.sel.rect)
        ink = a.mode.shows_ink  # the ink, as a swatch too
        rect, ends = draw.block(cr, [self.label(pw, ph)], self.sel.rect, draw.STATUS_SPOTS,
                                a.monitor_rects, avoid=avoid, extra=18 if ink else 0)
        if ink:
            draw.swatch(cr, ends[0] + 5, rect[1] + draw.ROW - 2, a.ink[1])
        return rect

    def label(self, pw, ph):
        """The status tag: the current mode (vim-style), the box size, the
        active cursor's pixel, and whatever the mode is working with."""
        a = self.app
        text = f"{a.mode.name.upper()}  {pw}×{ph}"
        pos = a.mode.cursor_px()  # whichever cursor is active
        text += f"  ({pos[0]},{pos[1]})" if pos else ""
        text += "  centre" if self.sel.center else ""
        if a.mode.shows_ink:
            text += f"  {a.ink[0]} {a.ink_width}px"
        if a.layer is not None:
            n = len(a.layer.ops)
            text += f"  layer +{n}" if n else "  layer"
        extra = a.mode.status() if a.mode is not self else ""
        text += f"  {extra}" if extra else ""
        n = len(a.layer.ops) if a.layer is not None else 0
        if a.discard_armed:
            text += f"  Esc again to discard {n}"
        if a.quit_armed:
            text += f"  q again to quit, discarding {n}"
        if a.note:
            text += f"  ({a.note})"
        return text

    def legend(self):
        return ["o/Space O/S-Space corner  w b } { edges  0 $ g G far  c centre  +/- scope  u undo",
                "i insert  z loupe  e enhance  x censor  a annotate  f filter  1-9 layers",
                "Enter capture  y text  p colour  Q code  P pin  Esc cancel  q quit"]
