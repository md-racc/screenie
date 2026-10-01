"""Layers: a view mode for the committed layers, entered with 1-9 in Select
(at that layer).

The selected layer is shown on its own: the capture with only that layer
applied, inside the box it was made in (hiding the layers below and above
it). A list shows every layer and what's in it (placed like every tag: on a
side of the box, outside it first).

The list keys are the same as the filter stack's:

    1-9              pick a layer
    [ / ], + / -     select the layer below / above (the wheel too)
    click            pick the topmost layer under the pointer
    < / >            move it down / up the stack (it applies earlier / later)
    d                delete it
    Space            hide / unhide it (kept, but left out of the picture)
    u                undo the last delete / move (the same undo as Select's)
    Enter / Esc      back to Select

Delete and move are undo steps in Select's history; hiding is a view toggle.
"""

from .. import draw
from ..geometry import contains
from ..keys import AMOUNT, CONFIRM, INC
from ..layers import LayerDeleted, LayerMoved
from .base import Mode


class LayersView(Mode):
    name = "layers"

    def __init__(self, app, sel, index):
        super().__init__(app)
        self.sel = sel
        self.index = min(index, len(app.frame.layers) - 1)

    @property
    def layers(self):
        return self.app.frame.layers

    @property
    def layer(self):
        return self.layers[self.index] if self.layers else None

    def pick(self, i):
        if self.layers:
            self.index = max(0, min(i, len(self.layers) - 1))

    def move(self, delta):
        """< / >: down / up the stack (earlier / later in the order)."""
        f, new = self.app.frame, self.index + delta
        if self.layer and 0 <= new < len(self.layers):
            layer = self.layer
            self.sel.history.append(LayerMoved(f, layer, self.index, self.sel.rect))
            self.layers.remove(layer)
            self.layers.insert(new, layer)
            self.index = new
            f.rebuild()

    def on_key(self, name):
        a, f = self.app, self.app.frame
        if name in "123456789" and len(name) == 1:
            self.pick(int(name) - 1)
        elif name in ("bracketleft", "bracketright") or name in AMOUNT:
            up = name == "bracketright" or name in INC
            self.pick(self.index + (1 if up else -1))
        elif name == "u":  # the same history as Select's: deletes and moves
            self.sel.undo()
            if not self.layers:
                a.pop()
                return
            self.pick(self.index)
        elif name in ("less", "greater"):
            self.move(1 if name == "greater" else -1)
        elif name == "d" and self.layer:
            layer = self.layer
            self.sel.history.append(LayerDeleted(f, layer, self.index, self.sel.rect))
            f.remove_layer(layer)
            if not self.layers:
                a.pop()
                return
            self.pick(self.index)
        elif name == "space" and self.layer:
            self.layer.hidden = not self.layer.hidden
            f.rebuild()
        elif name == "Escape" or name in CONFIRM:
            a.pop()

    def repeatable(self, name):
        return name in AMOUNT

    def on_press(self, button, gx, gy):
        """A click picks the topmost layer that touches the pointer."""
        if button != 1:
            return
        for i in reversed(range(len(self.layers))):
            area = self.layers[i].bbox()
            if area and contains(area, gx, gy):
                self.pick(i)
                return

    def legend(self):
        return ["1-9 pick  [ ] or +/- select  < > move  d delete  Space hide",
                "u undo  click pick  Enter/Esc back"]

    def status(self):
        if not self.layer:
            return "no layers"
        return (f"{self.index + 1}/{len(self.layers)}  {self.layer.describe()}"
                + ("  hidden" if self.layer.hidden else ""))

    def draw(self, cr, mon):
        a, f, layer = self.app, self.app.frame, self.layer
        if layer:
            # The capture with only this layer, where the layer was made.
            cr.save()
            cr.rectangle(*layer.clip)
            cr.clip()
            draw.in_frame(cr, f)
            patch, (px, py, _, _) = f.isolated(layer)
            cr.set_source_surface(patch, px, py)
            cr.paint()
            cr.restore()
            draw.outline(cr, layer.clip, (1, 1, 1, 0.9), width=1)

    def tag_list(self):
        """Every layer, the picked one marked (placed by the App)."""
        return [(">" if i == self.index else " ") + f" {i + 1}  {self.layers[i].describe()}"
                + ("  (hidden)" if self.layers[i].hidden else "")
                for i in reversed(range(len(self.layers)))]  # top first, like the filter stack
