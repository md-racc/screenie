"""The mode interface. The App sends the top mode every event; every mode on
the stack draws, bottom to top."""


class Mode:
    name = "mode"

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

    def repeat_delay(self):
        """Seconds a direction is held before repeat starts."""
        from ..app import DELAY
        return DELAY

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
