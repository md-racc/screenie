"""Censor: a build tool (x). Censors go into the open layer.

The main box freezes and a censor box appears inside it: a text-height
square at the centre of the main box, at the loupe's pixel if the loupe is
on, or with its top-left on insert mode's cursor. It's edited with all the
usual box keys and the mouse; t swaps its type (pixelate / bar), and
+ / - adjust it: how pixelated (block size, 4-40px, default 10) or the
bar's shade (black to white, default black).

    x          lock this censor into the layer, start the next where it was
    a / f      lock it, switch to annotate / filter (same layer)
    u          undo the last action on the layer
    Enter      lock it and commit the layer
    Esc        throw the layer away (nothing's committed until Enter)
"""

from ..keys import AMOUNT, INC
from ..layers import PIXELATE_BLOCK, CensorOp, Layer
from .box import BoxMode
from .layer import BuildTool

SIZE = 20  # starting square, logical px: about a line of UI text
KINDS = ["pixelate", "bar"]
# + / - per type: (step, min, max); the starting amounts are the defaults.
AMOUNTS = {"pixelate": (2, 4, 40),   # block size, logical px (bigger: more pixelated)
           "bar": (0.1, 0.0, 1.0)}   # shade, 0 black .. 1 white


class Censor(BuildTool, BoxMode):
    name = "censor"

    def __init__(self, app, main, at=None, corner=None):
        """at: centre the square there (default: the box centre). corner:
        put its top-left there instead, from insert mode's cursor; either
        way the censor's own cursor starts bottom-right."""
        if corner:
            x, y = corner
        else:
            cx, cy = at or main.centre()
            x, y = cx - SIZE / 2, cy - SIZE / 2
        super().__init__(app, app.selection([x, y, SIZE, SIZE], main.rect))
        self.main, self.kind = main, "pixelate"
        # each type remembers its own amount (across x and t)
        self.amounts = {"pixelate": PIXELATE_BLOCK, "bar": 0.0}

    def repeatable(self, name):
        return name in AMOUNT

    def current(self):
        if self.sel.empty():
            return None
        return CensorOp(self.sel.rect, self.kind, self.amounts[self.kind])

    def describe(self):
        v = self.amounts[self.kind]
        if self.kind == "pixelate":
            return f"pixelate {v}px"
        return {0.0: "black bar", 1.0: "white bar"}.get(round(v, 1), f"grey bar {round(v * 100)}%")

    def lock(self):
        self.app.lock(self.current())

    def on_key(self, name):
        a = self.app
        if self.layer_key(name, "x", self.main) or self.edit_key(name):
            return
        if name in AMOUNT:  # + / -: how pixelated, or the bar's shade
            step, lo, hi = AMOUNTS[self.kind]
            v = self.amounts[self.kind] + (step if name in INC else -step)
            self.amounts[self.kind] = round(max(lo, min(hi, v)), 2)
        elif name == "t":  # swap this censor's type
            self.kind = KINDS[(KINDS.index(self.kind) + 1) % len(KINDS)]
        elif name == "x":  # lock this one, start the next where it was
            cx, cy = self.sel.centre()
            self.lock()
            self.sel = a.selection([cx - SIZE / 2, cy - SIZE / 2, SIZE, SIZE], self.main.rect)

    def draw(self, cr, mon):
        op = self.current()  # the censor being edited, over the layer so far
        if op:
            self.app.frame.preview(cr, Layer(self.main.rect, [op]))
        self.draw_box(cr)

    def status(self):
        return f"{self.describe()}  {self.size_status()}"

    def legend(self):
        return ["t pixelate/bar  +/- amount  x next  a annotate  f filter",
                "u undo  Enter commit  Esc discard   (box keys as in Select)"]
