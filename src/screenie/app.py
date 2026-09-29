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
from .frame import FILTERS  # noqa: E402
from .geometry import SIDES, Selection, snap_targets  # noqa: E402
from .hypr import Layout  # noqa: E402
from .keys import BACK, LOUPE  # noqa: E402

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
        # Every overlay is keyboard-exclusive. While an exclusive layer is
        # up, Hyprland only lets the pointer reach exclusive layers, so an
        # overlay without the keyboard would never see a click. Whichever one
        # holds the keyboard doesn't matter: they all forward keys to the
        # App. (keyboard: this is the focused monitor's, shown last so it
        # should end up with the focus.)
        GtkLayerShell.set_keyboard_mode(self, GtkLayerShell.KeyboardMode.EXCLUSIVE)
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
                        | Gdk.EventMask.POINTER_MOTION_MASK
                        | Gdk.EventMask.SCROLL_MASK)
        # The pointer's implicit grab keeps delivering motion to the overlay a
        # drag started on, even past its monitor; offsets make that global.
        g = lambda ev: (ev.x + mon["lx"], ev.y + mon["ly"])  # noqa: E731
        self.connect("button-press-event", lambda _w, ev: app.on_press(ev.button, *g(ev)))
        self.connect("motion-notify-event", lambda _w, ev: app.on_motion(*g(ev)))
        self.connect("button-release-event", lambda _w, ev: app.on_release(ev.button, *g(ev)))
        self.connect("scroll-event", lambda _w, ev: app.on_scroll(ev))
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
        # View filters: lenses (loupe, enhance) only, never in the output.
        self.filter, self.sharp = "off", False
        # The open layer: the build tools' operations, pending until Enter;
        # layer_main / layer_depth: the box it's clipped to, and how deep the
        # mode stack was when it opened (where Enter / Esc return to).
        self.layer, self.layer_main, self.layer_depth = None, None, 0
        self.discard_armed = False  # one Esc pressed on a layer with work in it
        self.quit_armed = False     # one q pressed with work in an open layer
        # Undo within the open layer: one entry per action (lock, insert,
        # delete, move, mute, adjust), each a function that reverses it.
        self.layer_log = []
        self.note = None        # a one-off remark for the status tag
        self.show_help = False  # ?: the current mode's key legend
        self.loupe_px = 21  # loupe window, in frame pixels across (odd)
        # annotate: (name, rgb) and width in logical px; starts red, 3px
        self.ink, self.ink_width = ("red", (0.9, 0.12, 0.12)), 3

        self.result = None
        # Our own key repeat (GTK's autorepeat is ignored): keycode ->
        # (direction, press time). Every held key counts, so k+l is diagonal.
        self.held = {}
        self.down = set()  # other keys currently down (to drop autorepeat)
        self.shift = self.ctrl = self.alt = False
        # Keys held across a mode change: their autorepeat mustn't count as a
        # fresh press in the new mode (until they're let go).
        self.stale = set()
        self.acc, self.last_tick = 0.0, None

        self.modes = [Select(self, self.sel)]
        self.overlays = [Overlay(self, m, m is layout.focused) for m in layout.monitors]
        self.overlays[layout.monitors.index(layout.focused)].add_tick_callback(self.on_tick)

    def selection(self, rect, bounds):
        return Selection(rect, bounds, self.layout.windows, self.monitor_rects, self.targets)

    # --- making modes (one place, so modes don't import each other) ------------

    def make(self, kind, sel, **kw):
        """A mode by name: insert, loupe, enhance, layers."""
        from .modes.enhance import Enhance
        from .modes.insert import Insert
        from .modes.layers_view import LayersView
        from .modes.loupe import Loupe
        return {"insert": Insert, "loupe": Loupe, "enhance": Enhance,
                "layers": LayersView}[kind](self, sel, **kw)

    def lens(self, key, main, at=None):
        """z (loupe) or e (enhance) on the main box, at a global point."""
        if key in LOUPE:
            px = self.frame.pixel_under(*at) if at else None
            return self.make("loupe", main, at_px=px)
        return self.make("enhance", main, at=at)

    def tool(self, key, main, at=None, corner=None):
        """A factory for the build tool on key (x censor, a annotate, f
        filter), for start_tool / switch_tool."""
        from .modes.layer import make_tool
        return make_tool(self, key, main, at=at, corner=corner)

    # --- lifecycle ------------------------------------------------------------

    def run(self):
        focused = self.overlays[self.layout.monitors.index(self.layout.focused)]
        for o in self.overlays:  # the focused monitor's last (see Overlay)
            if o is not focused:
                o.show_all()
        focused.show_all()
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

    def switching(self):
        """Before the top mode changes: a drag in progress ends, and held keys
        go stale (their autorepeat isn't a new press in the next mode)."""
        self.mode.reset_pointer()
        self.stale |= set(self.held) | set(self.down)
        self.held.clear()

    def push(self, mode):
        self.switching()
        self.modes.append(mode)
        self.redraw()

    def pop(self):
        self.switching()
        child = self.modes.pop()
        self.mode.on_resume(child)
        self.redraw()

    def redraw(self):
        for o in self.overlays:
            o.queue_draw()

    # --- the open layer (Model B: one layer, many build tools) ---------------

    def start_tool(self, make_tool, main=None):
        """Open a build tool. With no layer open, this opens one, clipped to
        the main box; the layer is implicit, it just remembers where it was
        opened, which is where Enter / Esc come back to."""
        from .layers import Layer
        if self.layer is None:
            main = main or self.sel
            self.layer = Layer(main.rect, [])
            self.layer_log = []
            self.layer_main, self.layer_depth = main, len(self.modes)
            self.frame.set_pending(self.layer)
        self.push(make_tool())

    def switch_tool(self, make_tool):
        """From one build tool straight to another, in the same layer."""
        self.switching()
        del self.modes[self.layer_depth:]
        self.push(make_tool())

    def layer_changed(self):
        self.frame.set_pending(self.layer)

    def logged(self, undo):
        """Record an action on the open layer; undo() reverses it. New work
        also takes back a pending Esc / q warning."""
        self.layer_log.append(undo)
        self.discard_armed = self.quit_armed = False
        self.layer_changed()

    def lock(self, op, after=None):
        """Add an operation to the open layer: on top, or just above after."""
        if op is None:
            return
        ops = self.layer.ops
        ops.insert(ops.index(after) + 1 if after is not None else len(ops), op)
        self.logged(lambda: ops.remove(op))

    def undo_layer(self):
        """u in a build tool: undo the last action on the open layer, wherever
        in the stack it was."""
        if self.layer and self.layer_log:
            self.layer_log.pop()()
            self.layer_changed()

    def close_layer(self, commit):
        """Enter (commit) or Esc (discard) in a build tool: back to wherever
        the layer was opened from."""
        main = self.layer_main
        if commit and self.layer.ops:
            main.history.append(self.frame.add_layer(self.layer, main.rect))
        else:
            self.frame.set_pending(None)
        self.layer, self.layer_log = None, []
        self.discard_armed = self.quit_armed = False
        del self.modes[self.layer_depth + 1:]
        self.pop()

    # --- view state -------------------------------------------------------------

    def view_key(self, name):
        """View-filter keys, for the lenses: they change how you see, never
        the output (baked filters are the Filter build tool). v / V cycle
        off -> grey -> invert -> contrast (the reading one: greyscale, an
        S-curve, dark backgrounds flipped); s toggles sharp / smooth pixels."""
        if name in ("v", "V"):
            i = FILTERS.index(self.filter) + (-1 if name == "V" else 1)
            self.filter = FILTERS[i % len(FILTERS)]
        elif name == "s":
            self.sharp = not self.sharp
        else:
            return False
        return True

    def tags(self):
        return f"  {self.filter}" if self.filter != "off" else ""

    # --- input --------------------------------------------------------------------

    def sync_mods(self, ev, name, pressed):
        # ev.state is the state *before* this event, so a modifier's own
        # press/release has to be applied by hand.
        st = ev.state
        self.shift = bool(st & Gdk.ModifierType.SHIFT_MASK)
        self.ctrl = bool(st & Gdk.ModifierType.CONTROL_MASK)
        self.alt = bool(st & Gdk.ModifierType.MOD1_MASK)
        if name.startswith("Shift"):
            self.shift = pressed
        elif name.startswith("Control"):
            self.ctrl = pressed
        elif name.startswith("Alt"):
            self.alt = pressed

    def user_acted(self, key=None):
        """Any input takes back a pending warning (except the key that set
        it) and clears the one-off note."""
        if key != BACK:
            self.discard_armed = False
        if key != "q":
            self.quit_armed = False
        self.note = None

    def on_key(self, ev):
        name = Gdk.keyval_name(ev.keyval) or ""
        self.sync_mods(ev, name, True)
        code = ev.hardware_keycode
        if code in self.stale:  # held since before a mode change: ignore
            return True
        if not name.startswith(("Shift", "Control", "Alt", "Meta", "Super", "ISO_Level")):
            self.user_acted(name)
        if name == "question" and code not in self.down:  # ?: the key legend
            self.down.add(code)
            self.show_help = not self.show_help
            self.redraw()
            return True
        if name == "q" and code not in self.down:
            # q quits the picker from any mode; with locked work in an open
            # layer, only on a second q (the tag warns).
            self.down.add(ev.hardware_keycode)
            if self.layer and self.layer.ops and not self.quit_armed:
                self.quit_armed = True
            else:
                self.finish(None)
            self.redraw()
            return True
        key = name.lower() if len(name) == 1 else name
        if key in SIDES:
            if ev.hardware_keycode not in self.held:  # GTK autorepeat: we do our own
                self.held[ev.hardware_keycode] = (SIDES[key], now())
                self.mode.on_dir(SIDES[key])
        elif ev.hardware_keycode not in self.down or self.mode.repeatable(name):
            # Other keys fire once per press: GTK's autorepeat would make a
            # held toggle (z, e, c...) flicker on and off. A mode can opt a
            # key in (e.g. annotate's +/- width), which then repeats at the
            # compositor's key-repeat rate.
            self.down.add(ev.hardware_keycode)
            self.mode.on_key(name)
        self.redraw()
        return True

    def on_key_release(self, ev):
        name = Gdk.keyval_name(ev.keyval) or ""
        self.sync_mods(ev, name, False)
        self.held.pop(ev.hardware_keycode, None)
        self.down.discard(ev.hardware_keycode)
        self.stale.discard(ev.hardware_keycode)
        self.mode.on_key_release(name)
        self.redraw()
        return True

    def on_tick(self, _w, _clock):
        t = now()
        dt = t - self.last_tick if self.last_tick else 0
        self.last_tick = t
        delay = DELAY
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

    # The mouse, the same in every mode: left drag draws, left click places,
    # right click is Enter, middle click is Esc, the wheel is + / -. With a
    # plain mouse that's a whole screenshot: click a window (or drag a box),
    # scroll to widen it, right click.

    def on_press(self, button, gx, gy):
        self.user_acted(BACK if button == 2 else None)  # a middle click is Esc
        if button in (2, 3):
            self.mode.on_key("Escape" if button == 2 else "Return")
            self.redraw()
            return True
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

    def on_scroll(self, ev):
        if ev.direction == Gdk.ScrollDirection.UP:
            up = True
        elif ev.direction == Gdk.ScrollDirection.DOWN:
            up = False
        elif ev.direction == Gdk.ScrollDirection.SMOOTH:
            dy = ev.get_scroll_deltas()[2]
            if not dy:
                return True
            up = dy < 0
        else:
            return True
        self.user_acted()
        self.mode.on_key("plus" if up else "minus")
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
        # Tags on top of everything, each kind on its own side of the main
        # box (draw.STATUS_SPOTS and friends): the status tag on the bottom,
        # the current mode's palette and key legend (behind ?) on the top,
        # lists on the sides.
        placed = [r for m in self.modes for r in m.occupied()]
        placed.append(self.modes[0].draw_status(cr, avoid=placed))
        top = self.mode.palette() + (self.mode.legend() if self.show_help else [])
        blocks = [(top, draw.LEGEND_SPOTS)]
        blocks += [(m.tag_list(), draw.LIST_SPOTS) for m in self.modes]
        for lines, spots in blocks:
            if lines:
                rect, _ = draw.block(cr, lines, self.sel.rect, spots,
                                     self.monitor_rects, avoid=placed)
                placed.append(rect)
