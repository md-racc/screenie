"""Censor: entered with x. The main box freezes and a second box appears
inside it: a text-height square at the centre of the main box, at the
loupe's pixel if the loupe is on, or with its top-left on insert mode's
cursor. It's edited with all the usual box keys and the mouse, and t swaps
its type (pixelate / black bar).

Enter burns the censor into the frozen frame and returns to where you were
(Enter again in the main box captures); x burns it and starts the next one
right there, same type (multi-censor); Esc / right-click drops the current
one. Each censor is one undo step in the main box."""

from .. import draw
from .select import Select

SIZE = 20  # starting square, logical px: about a line of UI text
KINDS = ["pixelate", "black"]


class Censor(Select):
    name = "censor"
    outline_rgb = (1, 0.55, 0.1)

    def __init__(self, app, main, at=None, corner=None, kind="pixelate"):
        """at: centre the square there (default: the box centre). corner:
        put its top-left there instead, from insert mode's cursor; either
        way the censor's own cursor starts bottom-right."""
        if corner:
            x, y = corner
        else:
            cx, cy = at or main.centre()
            x, y = cx - SIZE / 2, cy - SIZE / 2
        sel = app.selection([x, y, SIZE, SIZE], main.rect)
        super().__init__(app, sel)
        self.main, self.kind = main, kind
        self.burned = 0  # censors locked in during this visit

    def burn(self):
        if not self.sel.empty():
            a = self.app
            self.main.history.append(a.frame.censor(self.sel.rect, self.kind, self.main.rect))
            self.burned += 1

    def on_key(self, name):
        a = self.app
        if self.edit_key(name):
            return
        if name in ("Return", "KP_Enter"):
            self.burn()
            a.pop()
        elif name == "Escape":
            a.pop()
        elif name == "t":  # swap this censor's type
            self.kind = KINDS[(KINDS.index(self.kind) + 1) % len(KINDS)]
        elif name == "x":
            # Multi-censor: lock this one in and start the next, same type,
            # where this one is, without leaving.
            cx, cy = self.sel.centre()
            self.burn()
            self.sel = a.selection([cx - SIZE / 2, cy - SIZE / 2, SIZE, SIZE], self.main.rect)

    def cancel(self):
        self.app.pop()

    def draw(self, cr, mon):
        a, f, sel = self.app, self.app.frame, self.sel
        x, y, w, h = sel.rect
        cr.save()  # live preview of the censor
        draw.in_frame(cr, f)
        f.paint_censor(cr, f.to_image(sel.rect), self.kind)
        cr.restore()
        draw.outline(cr, sel.rect, (*self.outline_rgb, 0.9))
        _, _, pw, ph = f.to_image(sel.rect)
        # Below the box: inside, it would cover what's being censored.
        draw.label(cr, x, y + h + 24, f"{self.kind}  {pw}×{ph}"
                   + ("  centre" if sel.center else "")
                   + (f"  +{self.burned} done" if self.burned else ""))
        if a.mode is self:
            draw.marker(cr, *sel.cursor_point(), self.outline_rgb)
