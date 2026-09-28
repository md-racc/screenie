"""The overlay windows, input plumbing and the mode stack.

One layer-shell overlay per monitor shows the frozen frame; all of them
forward input here in global logical coordinates. Behaviour lives in modes
(modes/): the top of the stack gets every key and mouse event, and every
mode on the stack draws, bottom to top, so a mode on top of Select sees
Select's box underneath it.
"""

from dataclasses import dataclass
from typing import Optional

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GtkLayerShell", "0.1")
from gi.repository import Gdk, GLib, Gtk, GtkLayerShell  # noqa: E402

from . import draw  # noqa: E402
from .frame import CONTRAST_MAX, FILTERS  # noqa: E402
from .geometry import SIDES, Selection, snap_targets  # noqa: E402
from .hypr import Layout  # noqa: E402

DELAY = 0.08   # s a key is held before our own repeat starts
RAMP = 0.6     # s from a mode's starting repeat rate to its top rate


def now():
    return GLib.get_monotonic_time() / 1e6


@dataclass
class Action:
    """How the picker finished; cli.py carries it out."""
    kind: str               # image | ocr | text | pin
    rect: Optional[list] = None
    text: Optional[str] = None   # for "text": what to copy
    note: Optional[str] = None   # for "text": the notification title


class Overlay(Gtk.Window):
    """One monitor's slice of the picker."""

    def __init__(self, app, mon, keyboard):
        super().__init__()
        self.mon = mon
        GtkLayerShell.init_for_window(self)
        GtkLayerShell.set_namespace(self, "screenie")
        GtkLayerShell.set_layer(self, GtkLayerShell.Layer.OVERLAY)
        # Only the focused monitor's overlay takes the keyboard; the others
        # would just fight over focus.
        GtkLayerShell.set_keyboard_mode(
            self, GtkLayerShell.KeyboardMode.EXCLUSIVE if keyboard
            else GtkLayerShell.KeyboardMode.NONE)
        GtkLayerShell.set_exclusive_zone(self, -1)
        for edge in (GtkLayerShell.Edge.TOP, GtkLayerShell.Edge.BOTTOM,
                     GtkLayerShell.Edge.LEFT, GtkLayerShell.Edge.RIGHT):
            GtkLayerShell.set_anchor(self, edge, True)
        gdk_mon = self._gdk_monitor()
        if gdk_mon:
            GtkLayerShell.set_monitor(self, gdk_mon)

        area = Gtk.DrawingArea()
        area.connect("draw", lambda _w, cr: app.draw(cr, mon))
        self.add(area)
        self.add_events(Gdk.EventMask.BUTTON_PRESS_MASK
                        | Gdk.EventMask.BUTTON_RELEASE_MASK
                        | Gdk.EventMask.POINTER_MOTION_MASK)
        # The pointer's implicit grab keeps delivering motion to the overlay a
        # drag started on, even past its monitor; offsets make that global.
        g = lambda ev: (ev.x + mon["lx"], ev.y + mon["ly"])  # noqa: E731
        self.connect("button-press-event", lambda _w, ev: app.on_press(ev.button, *g(ev)))
        self.connect("motion-notify-event", lambda _w, ev: app.on_motion(*g(ev)))
        self.connect("button-release-event", lambda _w, ev: app.on_release(ev.button, *g(ev)))
        self.connect("key-press-event", lambda _w, ev: app.on_key(ev))
        self.connect("key-release-event", lambda _w, ev: app.on_key_release(ev))
        self.connect("focus-out-event", lambda *_: (app.held.clear(), app.down.clear()))

    def _gdk_monitor(self):
        # GTK3 doesn't expose connector names; match on logical position.
        display = Gdk.Display.get_default()
        for i in range(display.get_n_monitors()):
            m = display.get_monitor(i)
            g = m.get_geometry()
            if (g.x, g.y) == (self.mon["lx"], self.mon["ly"]):
                return m
        return None


class App:
    def __init__(self, layout: Layout, frame):
        from .modes.select import Select  # modes import app helpers

        self.layout, self.frame = layout, frame
        self.monitor_rects = [Layout.monitor_rect(m) for m in layout.monitors]
        self.targets = snap_targets(layout.windows + self.monitor_rects)
        self.sel = self.selection(layout.start_rect(), layout.rect)

        # View state shared by every mode.
        self.filter, self.contrast, self.sharp = "off", 1, False
        self.loupe_px = 21  # loupe window, in frame pixels across (odd)

        self.result = None
        # Our own key repeat (GTK's autorepeat is ignored): keycode ->
        # (direction, press time). Every held key counts, so k+l is diagonal.
        self.held = {}
        self.down = set()  # other keys currently down (to drop autorepeat)
        self.shift = self.ctrl = self.alt = self.space = False
        self.acc, self.last_tick = 0.0, None

        self.modes = [Select(self, self.sel)]
        self.overlays = [Overlay(self, m, m is layout.focused) for m in layout.monitors]
        self.overlays[layout.monitors.index(layout.focused)].add_tick_callback(self.on_tick)

    def selection(self, rect, bounds):
        return Selection(rect, bounds, self.layout.windows, self.monitor_rects, self.targets)

    # --- lifecycle ------------------------------------------------------------

    def run(self):
        for o in self.overlays:
            o.show_all()
        Gtk.main()
        return self.result

    def finish(self, action=None):
        self.result = action
        for o in self.overlays:
            o.destroy()
        Gtk.main_quit()

    @property
    def mode(self):
        return self.modes[-1]

    def push(self, mode):
        self.held.clear()
        self.modes.append(mode)
        self.redraw()

    def pop(self):
        self.held.clear()
        child = self.modes.pop()
        self.mode.on_resume(child)
        self.redraw()

    def redraw(self):
        for o in self.overlays:
            o.queue_draw()

    # --- view state -------------------------------------------------------------

    def view_key(self, name):
        """Filter keys, shared by the modes that show the frame."""
        if name in ("f", "F"):
            i = FILTERS.index(self.filter) + (-1 if name == "F" else 1)
            self.filter = FILTERS[i % len(FILTERS)]
        elif name in ("plus", "equal", "KP_Add"):
            if self.filter == "contrast":
                self.contrast = min(CONTRAST_MAX, self.contrast + 1)
            else:
                self.filter, self.contrast = "contrast", 1
        elif name in ("minus", "KP_Subtract"):
            if self.filter == "contrast":
                self.contrast -= 1
                if self.contrast < 1:  # below the weakest level: back to raw
                    self.filter, self.contrast = "off", 1
        elif name == "s":
            self.sharp = not self.sharp
        else:
            return False
        return True

    def tags(self):
        return "".join([
            f"  {self.filter}" if self.filter != "off" else "",
            f" {self.contrast}" if self.filter == "contrast" else "",
        ])

    # --- input --------------------------------------------------------------------

    def sync_mods(self, ev, name, pressed):
        # ev.state is the state *before* this event, so a modifier's own
        # press/release has to be applied by hand.
        st = ev.state
        self.shift = bool(st & Gdk.ModifierType.SHIFT_MASK)
        self.ctrl = bool(st & Gdk.ModifierType.CONTROL_MASK)
        self.alt = bool(st & Gdk.ModifierType.MOD1_MASK)
        for prefix, attr in (("Shift", "shift"), ("Control", "ctrl"), ("Alt", "alt")):
            if name.startswith(prefix):
                setattr(self, attr, pressed)
        if name == "space":
            self.space = pressed

    def on_key(self, ev):
        name = Gdk.keyval_name(ev.keyval) or ""
        self.sync_mods(ev, name, True)
        key = name.lower() if len(name) == 1 else name
        if key in SIDES:
            if ev.hardware_keycode not in self.held:  # GTK autorepeat: we do our own
                self.held[ev.hardware_keycode] = (SIDES[key], now())
                self.mode.on_dir(SIDES[key])
        elif ev.hardware_keycode not in self.down:
            # Other keys fire once per press: GTK's autorepeat would make a
            # held toggle (z, e, c...) flicker on and off.
            self.down.add(ev.hardware_keycode)
            self.mode.on_key(name)
        self.redraw()
        return True

    def on_key_release(self, ev):
        name = Gdk.keyval_name(ev.keyval) or ""
        self.sync_mods(ev, name, False)
        self.held.pop(ev.hardware_keycode, None)
        self.down.discard(ev.hardware_keycode)
        self.mode.on_key_release(name)
        self.redraw()
        return True

    def on_tick(self, _w, _clock):
        t = now()
        dt = t - self.last_tick if self.last_tick else 0
        self.last_tick = t
        delay = self.mode.repeat_delay()
        repeating = [(d, t0) for d, t0 in self.held.values() if t - t0 >= delay]
        rates = self.mode.rates() if repeating else None
        if not rates:
            self.acc = 0.0
            return GLib.SOURCE_CONTINUE
        r0, rmax = rates  # steps per second
        held_for = t - min(t0 for _, t0 in repeating) - delay
        self.acc += (r0 + (rmax - r0) * min(1.0, held_for / RAMP)) * dt
        n = int(self.acc)
        if n:
            self.acc -= n
            self.mode.on_repeat({d for d, _ in repeating}, n)
            self.redraw()
        return GLib.SOURCE_CONTINUE

    def on_press(self, button, gx, gy):
        self.mode.on_press(button, gx, gy)
        self.redraw()
        return True

    def on_motion(self, gx, gy):
        self.mode.on_motion(gx, gy)
        self.redraw()
        return True

    def on_release(self, button, gx, gy):
        self.mode.on_release(button, gx, gy)
        self.redraw()
        return True

    # --- drawing ----------------------------------------------------------------

    def draw(self, cr, mon):
        cr.translate(-mon["lx"], -mon["ly"])  # everything in global coords
        cr.save()
        draw.in_frame(cr, self.frame)
        self.frame.paint(cr, (0, 0, 1, 1))
        cr.restore()
        for m in self.modes:
            m.draw(cr, mon)
