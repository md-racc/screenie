"""The mode interface. The App sends the top mode every event; every mode on
the stack draws, bottom to top."""


class Mode:
    name = "mode"
    shows_ink = False  # annotate / shape: the tag shows the ink and a swatch

    def __init__(self, app):
        self.app = app

    # Keyboard. on_dir: a direction key went down (h/j/k/l/arrows as L/B/T/R).
    # on_repeat: n repeat steps while directions are held; rates() says how
    # fast, as (starting, top) steps per second, or None for no repeat.
    def on_dir(self, d):
        pass

    def on_repeat(self, dirs, n):
        pass

    def rates(self):
        return None

    def cursor_px(self):
        """The frame pixel this mode's cursor is on, for the status tag's
        (x, y); None if it has no cursor."""
        return None

    def tag_list(self) -> list:
        """Lines for a list tag (the layer view's layers, the filter stack),
        placed by the App around the main box after the status tag."""
        return []

    def status(self) -> str:
        """This mode's part of the status tag (Select draws the one tag,
        bottom-left inside the main box)."""
        return ""

    def repeatable(self, name) -> bool:
        """Whether holding this (non-direction) key should repeat it."""
        return False

    def reset_pointer(self):
        """Another mode is taking over: drop any drag in progress (its release
        will go to the new mode)."""

    def palette(self) -> list:
        """What the mode's digits / letters pick (the filters, the shapes):
        always shown on top of the box, above the legend."""
        return []

    def legend(self) -> list:
        """The rest of the mode's keys, shown on top of the box with ?."""
        return []

    def occupied(self) -> list:
        """Rects this mode draws that tags must keep clear of (the loupe's
        magnifier)."""
        return []

    def on_resume(self, child):
        """Back on top after child (a mode pushed from here) was popped."""

    def on_key(self, name):
        pass

    def on_key_release(self, name):
        pass

    # Mouse, in global logical coordinates.
    def on_press(self, button, gx, gy):
        pass

    def on_motion(self, gx, gy):
        pass

    def on_release(self, button, gx, gy):
        pass

    def draw(self, cr, mon):
        pass
