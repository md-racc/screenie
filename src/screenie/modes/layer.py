"""The build tools' shared plumbing: which key opens which tool, and the
keys that mean the same thing in every tool.

The open layer is implicit (see App.start_tool): the first build tool opens
it, every build tool used before Enter adds to it, in order, and Enter / Esc
come back to wherever it was opened.
"""

from typing import TYPE_CHECKING

from ..keys import CONFIRM


def make_tool(app, key, main, at=None, corner=None):
    """A factory for the build tool on key (x censor, a annotate, f filter),
    for app.start_tool / app.switch_tool."""
    from .annotate import Annotate
    from .censor import Censor
    from .filter import Filter
    if key == "x":
        return lambda: Censor(app, main, at=at, corner=corner)
    if key == "a":
        return lambda: Annotate(app, main)
    return lambda: Filter(app, main)


TOOL_KEYS = ("x", "a", "f")

if TYPE_CHECKING:
    from ..app import App


class BuildTool:
    """Shared behaviour of the build tools (mixed into their mode classes):
    the layer keys that mean the same thing in every tool."""

    app: "App"  # set by the Mode this is mixed into

    def layer_key(self, name, own_key, main):
        """Returns True if handled. own_key: the tool's own key, handled by
        the tool itself (lock this item and start another)."""
        a = self.app
        if name in TOOL_KEYS and name != own_key:  # another tool, same layer
            self.lock()
            a.switch_tool(make_tool(a, name, main))
        elif name == "u":  # undo the last action on the layer
            a.undo_layer()
        elif name in CONFIRM:
            self.lock()
            a.close_layer(commit=True)
        elif name == "Escape":
            # Nothing's committed until Enter, so Esc throws the layer away;
            # with locked work in it, only on a second Esc (the tag warns).
            if a.layer and a.layer.ops and not a.discard_armed:
                a.discard_armed = True
            else:
                a.close_layer(commit=False)
        else:
            return False
        return True

    def lock(self):
        """Lock the item being edited into the layer (tools override)."""
