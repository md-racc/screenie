"""Filter: a build tool (f). A stack of filters, folded into the open layer
(and baked into the picture) on Enter.

It opens raw. A digit inserts a filter just above the selected one (on top
if none is) and selects it (the palette on top shows which digit is which). + / -
adjust the selected one (hold to keep going). Every filter has its own box,
starting as the whole main box: the box shows the selected filter's, and
the usual box keys and the mouse reshape it. The stack is listed (top
first; placed like every tag) and previews live; order matters.

    1          greyscale      + / -  amount, 25% - 100%
    2          invert         (on or off)
    3          brightness     + / -  brighter / darker, -5 .. +5 (10% a step)
    4          contrast       + / -  more (an S-curve) / less (toward grey), -4 .. +4
    5          blur           + / -  radius, 2 - 20 px

    [ / ]      select the filter below / above
    < / >      move the selected filter down / up the stack
    d          delete it
    Space      mute / unmute it (kept, not applied)
    f          duplicate it (just above it)
    x / a      switch to censor / annotate (same layer)
    u          undo the last action (add, delete, move, mute, adjust, reshape)
    Enter      fold the stack into the layer and commit it
    Esc        throw the layer away (twice if there's locked work in it)
"""

import copy

from ..keys import AMOUNT, INC
from ..layers import FilterOp
from .box import BoxMode
from .layer import BuildTool

# key -> (kind, starting amount, step, min, max); invert has no amount.
KINDS = {
    "1": ("grey", 1.0, 0.25, 0.25, 1.0),
    "2": ("invert", 1, 0, 1, 1),
    "3": ("brightness", 1, 1, -5, 5),
    "4": ("contrast", 1, 1, -4, 4),
    "5": ("blur", 4, 2, 2, 20),
}
BY_KIND = {v[0]: v for v in KINDS.values()}


def describe(op):
    k, v = op.kind, op.amount
    if k == "grey":
        text = f"grey {round(v * 100)}%"
    elif k in ("brightness", "contrast"):
        text = f"{k} {v:+d}"
    elif k == "blur":
        text = f"blur {v}px"
    else:
        text = k
    return text + ("  (muted)" if op.muted else "")


class Filter(BuildTool, BoxMode):
    name = "filter"

    def __init__(self, app, main):
        super().__init__(app, app.selection(list(main.rect), main.rect))
        self.main = main
        self.mine = []        # the filters this tool made (they live in the layer)
        self.op = None        # the selected one
        self._run = None      # (op, what, log length): an ongoing adjust / reshape

    def repeatable(self, name):
        return name in AMOUNT

    # --- the stack: this tool's filters, in the layer's order --------------------

    @property
    def stack(self):
        ops = self.app.layer.ops if self.app.layer else []
        return [op for op in ops if any(op is m for m in self.mine)]

    @property
    def selected(self):
        stack = self.stack
        return next((i for i, op in enumerate(stack) if op is self.op), None)

    def select(self, op):
        self.op = op
        if op is not None:
            self.sel.set_rect(op.rect)  # the box shows the selected filter's

    def resync(self):
        """After an undo: the selected filter may be gone."""
        if self.selected is None:
            stack = self.stack
            self.select(stack[-1] if stack else None)
        else:
            self.sel.set_rect(self.op.rect)

    def edit(self, what, undo):
        """Log an edit to the selected filter. A run of the same edit to the
        same filter (holding +, a drag) is one undo step."""
        a = self.app
        if self._run == (self.op, what, len(a.layer_log)):
            a.layer_changed()
            return
        a.logged(undo)
        self._run = (self.op, what, len(a.layer_log))

    def insert(self, op):
        """Just above the selected filter (on top if none is), in the layer;
        the new one is selected."""
        self.mine.append(op)
        self.app.lock(op, after=self.op)
        self.select(op)

    def move(self, delta):
        """Down / up the stack, and so in the layer's order."""
        stack, i = self.stack, self.selected
        j = i + delta
        if not 0 <= j < len(stack):
            return
        ops = self.app.layer.ops
        a, b = ops.index(stack[i]), ops.index(stack[j])

        def swap():
            ops[a], ops[b] = ops[b], ops[a]
        swap()
        self.app.logged(swap)

    def delete(self):
        op, ops = self.op, self.app.layer.ops
        at, below = ops.index(op), self.selected
        ops.remove(op)
        self.app.logged(lambda: ops.insert(at, op))
        stack = self.stack
        self.select(stack[max(0, below - 1)] if stack else None)

    def changed(self):
        """Box edits (keys, the mouse) reshape the selected filter."""
        op = self.op
        if op is not None and op.rect != self.sel.rect:
            old = list(op.rect)
            op.rect = list(self.sel.rect)
            self.edit("rect", lambda: setattr(op, "rect", old))

    # --- input --------------------------------------------------------------------

    def on_key(self, name):
        a, op = self.app, self.op
        if name == "u":
            a.undo_layer()
            self._run = None
            self.resync()
            return
        if self.layer_key(name, "f", self.main):
            return
        if name in KINDS:
            kind, amount = KINDS[name][:2]
            self.insert(FilterOp(self.sel.rect, kind, amount))
        elif name in AMOUNT:
            if op is None:
                a.note = "pick a filter: 1-5"
                return
            _, _, step, lo, hi = BY_KIND[op.kind]
            old = op.amount
            op.amount = max(lo, min(hi, op.amount + (step if name in INC else -step)))
            if op.amount != old:
                self.edit("amount", lambda: setattr(op, "amount", old))
        elif name in ("bracketleft", "bracketright"):
            stack, i = self.stack, self.selected
            if i is not None:
                self.select(stack[max(0, min(len(stack) - 1,
                                             i + (1 if name == "bracketright" else -1)))])
        elif name in ("less", "greater"):
            if op is not None:
                self.move(1 if name == "greater" else -1)
        elif name == "d":
            if op is not None:
                self.delete()
        elif name == "space" and not a.shift:  # mute / unmute (the layer view's hide)
            if op is not None:
                def toggle():
                    op.muted = not op.muted
                toggle()
                a.logged(toggle)
        elif name == "f":  # duplicate the selected filter, just above it
            if op is not None:
                self.insert(copy.copy(op))
        else:
            self.edit_key(name)

    # --- drawing ------------------------------------------------------------------

    def draw(self, cr, mon):
        self.draw_box(cr)

    def tag_list(self):
        """The stack, top first, the selected filter marked (placed by the
        App). No numbers: the digits pick filter kinds here."""
        return [(">" if op is self.op else " ") + f" {describe(op)}"
                for op in reversed(self.stack)]

    def status(self):
        if self.op is None:
            return "raw"
        return f"{self.selected + 1}/{len(self.stack)} {describe(self.op)}  {self.size_status()}"

    def palette(self):
        return ["1 grey  2 invert  3 bright  4 contrast  5 blur"]

    def legend(self):
        return ["+/- amount  [ ] select  < > move  d delete  Space mute  f duplicate  u undo",
                "x censor  a annotate  Enter commit  Esc discard"]
