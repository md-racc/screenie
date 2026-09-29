"""Annotate: a build tool (a). Lines and shapes go into the open layer,
clipped to the box.

The pen is the default: drag with the mouse to draw a freehand line (it's
locked into the layer when you let go). 1-5 pick white / black / red /
green / blue, p picks up the colour under the pointer (an eyedropper; kept
until a number key), + / - change the line width (hold to keep going).
r and c open a rectangle / circle as a box of its own (Shape), edited with
all of Select's box keys and the mouse.

    x / f      switch to censor / filter (same layer)
    u          undo the last action on the layer
    Enter      commit the layer (a line being drawn is kept)
    Esc        throw the layer away (twice if there's locked work in it)
    z / e      the loupe / enhance, at the pointer
"""

from .. import draw
from ..keys import AMOUNT, DEC, INC, LOUPE, hex_colour
from ..layers import Layer, ShapeOp, StrokeOp
from .base import Mode
from .box import BoxMode
from .layer import BuildTool

COLOURS = {"1": ("white", (1.0, 1.0, 1.0)),
           "2": ("black", (0.0, 0.0, 0.0)),
           "3": ("red", (0.9, 0.12, 0.12)),
           "4": ("green", (0.1, 0.75, 0.25)),
           "5": ("blue", (0.15, 0.45, 1.0))}
WIDTH_MIN, WIDTH_MAX = 1, 40
SHAPES = ["rect", "circle"]


def ink_key(app, name):
    """Colour and width keys, shared by the pen and shapes."""
    if name in COLOURS:
        app.ink = COLOURS[name]
    elif name in INC:
        app.ink_width = min(WIDTH_MAX, app.ink_width + 1)
    elif name in DEC:
        app.ink_width = max(WIDTH_MIN, app.ink_width - 1)
    else:
        return False
    return True


def pick_ink(app, gx, gy):
    """p: the colour under a global point becomes the ink."""
    r, g, b = app.frame.rgb(*app.frame.pixel_under(gx, gy))
    app.ink = (hex_colour((r, g, b)), (r / 255, g / 255, b / 255))


class Annotate(BuildTool, Mode):
    name = "annotate"
    shows_ink = True

    def __init__(self, app, main):
        super().__init__(app)
        self.main = main
        self.points = None  # the stroke being drawn
        self.pointer = main.centre()  # last pointer position, for p / z / e

    def repeatable(self, name):
        return name in AMOUNT

    def stroke(self, points):
        return StrokeOp(points, self.app.ink[1], self.app.ink_width)

    def lock(self):
        """A line still being drawn (Enter / a tool key mid-stroke) is kept."""
        if self.points:
            self.app.lock(self.stroke(self.points))
            self.points = None

    def on_key(self, name):
        a = self.app
        if ink_key(a, name) or self.layer_key(name, "a", self.main):
            return
        if name == "p":
            pick_ink(a, *self.pointer)
        elif name in ("r", "c"):
            a.push(Shape(a, self.main, "rect" if name == "r" else "circle"))
        elif name in LOUPE or name == "e":
            a.push(a.lens(name, self.main, at=self.pointer))

    def on_press(self, button, gx, gy):
        if button == 1:
            self.points = [(gx, gy)]

    def on_motion(self, gx, gy):
        self.pointer = (gx, gy)
        if self.points is not None:
            self.points.append((gx, gy))

    def on_release(self, button, gx, gy):
        if button == 1 and self.points:
            if self.points[-1] != (gx, gy):
                self.points.append((gx, gy))  # right up to where it was let go
            self.lock()

    def reset_pointer(self):
        self.points = None

    def draw(self, cr, mon):
        if self.points:  # the stroke being drawn, over the layer so far
            self.app.frame.preview(cr, Layer(self.main.rect, [self.stroke(self.points)]))
        # (the mode, colour and width show in Select's label on the main box)

    def palette(self):
        return ["r rectangle  c circle"]

    def legend(self):
        return ["1 white  2 black  3 red  4 green  5 blue   +/- width  p pick colour",
                "drag draw  u undo  x censor  f filter",
                "z loupe  e enhance  Enter commit  Esc discard"]


class Shape(BuildTool, BoxMode):
    """A rectangle / circle as a box of its own inside the main box. t swaps
    rect / circle; a locks it and goes back to the pen (for another shape);
    x / f lock it and switch tool; Enter locks it and commits the layer;
    Esc drops just this shape; u undoes the last action on the layer; p
    picks up the colour under its cursor."""
    name = "shape"
    shows_ink = True

    def __init__(self, app, main, kind):
        x, y, w, h = main.rect
        super().__init__(app, app.selection([x + w / 4, y + h / 4, w / 2, h / 2], main.rect))
        self.main, self.kind = main, kind

    def repeatable(self, name):
        return name in AMOUNT

    def current(self):
        a = self.app
        return None if self.sel.empty() else ShapeOp(self.kind, self.sel.rect, a.ink[1], a.ink_width)

    def lock(self):
        self.app.lock(self.current())

    def on_key(self, name):
        a = self.app
        if name == "p":  # eyedropper at the shape's cursor
            pick_ink(a, *self.sel.cursor_point())
        if name == "p" or ink_key(a, name):
            return
        if name == "a":  # lock it, back to the pen for the next
            self.lock()
            a.pop()
        elif name == "Escape":  # drop just this shape, back to the pen
            a.pop()
        elif self.layer_key(name, "a", self.main) or self.edit_key(name):
            return
        elif name == "t":
            self.kind = SHAPES[(SHAPES.index(self.kind) + 1) % len(SHAPES)]

    def draw(self, cr, mon):
        op = self.current()
        if op:  # live preview, clipped to the main box like the real thing
            self.app.frame.preview(cr, Layer(self.main.rect, [op]))
        self.draw_box(cr)

    def status(self):
        return f"{self.kind}  {self.size_status()}"

    def palette(self):
        return ["t rectangle/circle"]

    def legend(self):
        return ["1-5 colour  +/- width  p pick colour",
                "a next shape  Esc drop this one  u undo  Enter commit",
                "c centre  (box keys as in Select)"]
