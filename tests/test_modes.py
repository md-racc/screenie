"""Mode behaviour, driving the real App with synthetic events on a synthetic
frame. Needs a Wayland session (GTK windows are created, never shown).
Run: python -m unittest discover -s tests"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import cairo  # noqa: E402

HAVE_DISPLAY = bool(os.environ.get("WAYLAND_DISPLAY"))

if HAVE_DISPLAY:
    # screenie.app pins GTK/Gdk 3, so it has to be imported before Gdk.
    from screenie.app import App  # noqa: E402
    from screenie.frame import Frame  # noqa: E402
    from screenie.hypr import Layout  # noqa: E402

    from gi.repository import Gdk  # noqa: E402

MONS = [
    {"lx": 0, "ly": 0, "lw": 1920, "lh": 1080},
    {"lx": 1920, "ly": 0, "lw": 1920, "lh": 1080},
]
WINDOWS = [[1923, 3, 955, 1074], [2883, 3, 954, 1074], [3, 3, 955, 1074]]
START = [100, 170, 560, 90]


def fake_layout():
    layout = object.__new__(Layout)
    layout.monitors, layout.focused = MONS, MONS[0]
    layout.windows, layout.active = WINDOWS, None
    layout.rect = [0, 0, 3840, 1080]
    layout.start_rect = lambda: list(START)
    return layout


def fake_frame():
    s = cairo.ImageSurface(cairo.FORMAT_ARGB32, 3840, 1080)
    cr = cairo.Context(s)
    cr.set_source_rgb(0.12, 0.12, 0.14)
    cr.paint()
    cr.select_font_face("monospace")
    cr.set_font_size(15)
    cr.set_source_rgb(0.85, 0.85, 0.85)
    for i in range(3):
        cr.move_to(120, 200 + i * 24)
        cr.show_text("The quick brown fox jumps over the lazy dog.")
    return Frame(s, [0, 0, 3840, 1080])


class Ev:
    """Just enough of a Gdk key event."""
    _code = 100

    def __init__(self, name, shift=False, code=None):
        self.keyval = Gdk.keyval_from_name(name)
        self.state = Gdk.ModifierType.SHIFT_MASK if shift else 0
        Ev._code += 1
        self.hardware_keycode = code or Ev._code


@unittest.skipUnless(HAVE_DISPLAY, "needs a Wayland session")
class Modes(unittest.TestCase):
    def setUp(self):
        self.app = App(fake_layout(), fake_frame())
        self.app.finish = lambda action=None: setattr(self.app, "result", action)

    def key(self, name, **kw):
        ev = Ev(name, **kw)
        self.app.on_key(ev)
        return ev

    def tap(self, name, **kw):
        self.app.on_key_release(self.key(name, **kw))

    def test_contrast_steps_back_down_to_raw(self):
        a = self.app
        self.tap("plus")
        self.tap("plus")
        self.assertEqual((a.filter, a.contrast), ("contrast", 2))
        self.tap("minus")
        self.tap("minus")
        self.assertEqual(a.filter, "off")

    def test_loupe_has_its_own_pixel_cursor(self):
        a = self.app
        self.tap("z")
        self.assertEqual(a.mode.name, "loupe")  # a toggle: still on after release
        loupe = a.mode
        x, y, w, h = a.frame.to_image(a.sel.rect)
        self.assertEqual((loupe.px, loupe.py), (x + w // 2, y + h // 2))
        self.tap("l")
        self.assertEqual(loupe.px, x + w // 2 + 1)
        box_before = list(a.sel.rect)
        for _ in range(2000):  # can't leave the box
            loupe.on_repeat({"R"}, 1)
        self.assertEqual(loupe.px, x + w - 1)
        self.assertEqual(a.sel.rect, box_before)
        self.tap("plus")
        self.assertEqual(a.loupe_px, 25)
        self.tap("z")
        self.assertEqual(a.mode.name, "select")

    def test_loupe_shift_jumps_one_loupe_width(self):
        a = self.app
        self.tap("z")
        loupe = a.mode
        px = loupe.px
        self.tap("H", shift=True)
        self.assertEqual(loupe.px, px - a.loupe_px)

    def test_autorepeat_does_not_flicker_toggles(self):
        a = self.app
        ev = self.key("z")
        a.on_key(ev)  # GTK autorepeat: same keycode, no release in between
        a.on_key(ev)
        self.assertEqual(a.mode.name, "loupe")

    def test_censor_starts_as_a_text_height_square_at_the_centre(self):
        a = self.app
        self.tap("x")
        x, y, w, h = a.mode.sel.rect
        self.assertEqual((w, h), (20, 20))
        self.assertEqual((x + w / 2, y + h / 2), tuple(a.sel.centre()))

    def test_censor_from_the_loupe_starts_at_the_loupe(self):
        a = self.app
        self.tap("z")
        for _ in range(40):
            self.tap("h")
        gx, gy = a.frame.to_global(a.mode.px, a.mode.py)
        self.tap("x")
        self.assertEqual(a.mode.name, "censor")
        x, y, w, h = a.mode.sel.rect
        self.assertEqual((x + w / 2, y + h / 2), (gx, gy))
        self.tap("Escape")
        self.assertEqual(a.mode.name, "loupe")  # back to where it started

    def test_censor_box_lives_inside_the_main_box(self):
        a = self.app
        self.tap("x")
        self.assertEqual(a.mode.name, "censor")
        self.assertEqual(a.mode.sel.bounds, START)
        for _ in range(100):
            self.tap("L", shift=True)
        x, y, w, h = a.mode.sel.rect
        self.assertLessEqual(x + w, START[0] + START[2])

    def test_censor_enter_burns_in_and_undo_restores(self):
        a = self.app
        before = a.frame.crop(START).get_data().tobytes()
        self.tap("x")
        self.tap("t")  # black bar
        self.tap("Return")
        self.assertEqual(a.mode.name, "select")
        self.assertNotEqual(a.frame.crop(START).get_data().tobytes(), before)
        self.tap("u")
        self.assertEqual(a.frame.crop(START).get_data().tobytes(), before)

    def test_x_in_censor_burns_and_starts_the_next(self):
        a = self.app
        self.tap("x")
        first = list(a.mode.sel.rect)
        before = a.frame.crop(first).get_data().tobytes()
        self.tap("x")  # lock in the pixelate, start the next
        cen = a.mode
        self.assertEqual(cen.name, "censor")
        self.assertEqual((cen.kind, cen.burned), ("pixelate", 1))
        self.tap("t")  # this one's a black bar
        self.assertEqual(cen.kind, "black")
        self.tap("x")  # ...and the next keeps the type
        self.assertEqual((cen.kind, cen.burned), ("black", 2))
        self.tap("t")
        self.assertEqual(cen.kind, "pixelate")
        self.assertEqual(cen.sel.rect, first)  # the next starts where that one was
        self.assertNotEqual(a.frame.crop(first).get_data().tobytes(), before)
        for _ in range(8):
            self.tap("L", shift=True)
        self.tap("Return")
        self.assertEqual(a.mode.name, "select")
        self.assertEqual(len([h for h in a.sel.history if not isinstance(h, list)]), 3)
        self.tap("u")
        self.tap("u")
        self.tap("u")
        self.assertEqual(a.frame.crop(first).get_data().tobytes(), before)

    def test_loupe_follows_clicks_and_drags(self):
        a = self.app
        self.tap("z")
        loupe = a.mode
        a.on_press(1, 200.5, 190.5)
        self.assertEqual((loupe.px, loupe.py), (200, 190))
        a.on_motion(300.5, 200.5)
        self.assertEqual((loupe.px, loupe.py), (300, 200))
        a.on_release(1, 300.5, 200.5)
        a.on_motion(400.5, 220.5)  # not dragging any more
        self.assertEqual((loupe.px, loupe.py), (300, 200))
        a.on_press(1, 5000, 5000)  # clamped to the box
        x, y, w, h = a.frame.to_image(START)
        self.assertEqual((loupe.px, loupe.py), (x + w - 1, y + h - 1))

    def test_censor_escape_changes_nothing(self):
        a = self.app
        before = a.frame.crop(START).get_data().tobytes()
        self.tap("x")
        self.tap("Escape")
        self.assertEqual(a.mode.name, "select")
        self.assertIsNone(a.result)
        self.assertEqual(a.frame.crop(START).get_data().tobytes(), before)

    def test_censor_mouse_drag_draws_the_censor_box(self):
        a = self.app
        self.tap("x")
        a.on_press(1, 150, 180)
        a.on_motion(300, 240)
        a.on_release(1, 300, 240)
        self.assertEqual(a.mode.sel.rect, [150, 180, 150, 60])
        self.assertIsNone(a.result)  # a drag never finishes anything

    def test_enhance_lens_zooms_and_stays_inside_the_box(self):
        a = self.app
        self.tap("f")
        self.tap("e")
        en = a.mode
        self.assertEqual(en.name, "enhance")
        self.assertEqual(en.lens.bounds, START)
        z = en.zoom
        self.tap("j")
        self.assertAlmostEqual(en.zoom, z * 1.25)
        self.tap("k")
        self.assertAlmostEqual(en.zoom, z)
        for _ in range(500):
            self.tap("L", shift=True)
        x, y, w, h = en.lens.rect
        self.assertEqual(x + w, START[0] + START[2])  # stopped at the box edge
        self.assertEqual(a.sel.rect, START)  # the box itself is frozen
        self.assertEqual(a.filter, "grey")   # filters carry over
        self.tap("Escape")
        self.assertEqual(a.mode.name, "select")

    def test_enhance_zoom_taps_step_by_half(self):
        a = self.app
        self.tap("e")
        en = a.mode
        seen = []
        for k in ("j", "j", "k"):
            self.tap(k)
            seen.append(en.zoom)
        self.assertEqual(seen, [2.5, 3.0, 2.5])
        en.zoom = 2.73  # after a continuous zoom, a tap lands on the next mark
        self.tap("j")
        self.assertEqual(en.zoom, 3.0)

    def test_insert_cursor_moves_and_tools_open_at_it(self):
        a = self.app
        self.tap("i")
        ins = a.mode
        self.assertEqual(ins.name, "insert")
        bx, by, bw, bh = a.frame.to_image(START)
        self.assertEqual((ins.px, ins.py), (bx + bw // 2, by + bh // 2))
        self.tap("l")
        self.assertEqual(ins.px, bx + bw // 2 + 5)
        self.tap("L", shift=True)
        self.assertEqual(ins.px, bx + bw // 2 + 25)
        self.tap("0")
        self.assertEqual(ins.px, bx)
        self.tap("m")
        self.tap("G")
        self.assertEqual(ins.py, by + bh - 1)
        self.tap("m")

        at = a.frame.to_global(ins.px, ins.py)
        cursor = (ins.px, ins.py)
        self.tap("x")  # the cursor is the censor's top-left; its own cursor bottom-right
        c = a.mode.sel
        self.assertEqual(c.rect[:2], [ins.px, ins.py])  # frame k=1: pixel == logical
        self.assertEqual(c.cur, ["R", "B"])
        self.tap("l")
        self.assertEqual(c.rect[2], 25)
        self.tap("Return")  # burn it in: back to insert, cursor unmoved
        self.assertEqual(a.mode.name, "insert")
        self.assertEqual((ins.px, ins.py), cursor)
        self.tap("x")
        self.tap("Escape")
        self.assertEqual(a.mode.name, "insert")
        self.tap("e")
        self.assertEqual(a.mode.lens.centre(), at)
        self.tap("Escape")

        self.tap("z")  # the loupe starts at the cursor, and the cursor follows it
        self.assertEqual((a.mode.px, a.mode.py), (ins.px, ins.py))
        for _ in range(7):  # the loupe still moves 1px per tap
            self.tap("h")
        self.tap("z")
        self.assertEqual(a.mode.name, "insert")
        self.assertEqual(ins.px, bx + bw // 2 - 7)

        self.tap("Escape")
        self.assertEqual(a.mode.name, "select")

    def test_insert_alt_halves_toward_the_box_edge(self):
        a = self.app
        self.tap("i")
        ins = a.mode
        bx, by, bw, bh = a.frame.to_image(START)
        a.alt = True
        ins.on_dir("L")
        a.alt = False
        self.assertEqual(ins.px, round((bx + bw // 2 + bx) / 2))

    def test_insert_top_speed_scales_with_the_box(self):
        def top(rect, held):
            lay = fake_layout()
            lay.start_rect = lambda: list(rect)
            app = App(lay, fake_frame())
            self.tap_on(app, "i")
            app.held = {1: (held, 0.0)}
            return app.mode.rates()[1] * app.mode.step()  # steps/s -> px/s
        self.assertEqual(top([100, 100, 400, 50], "R"), 400)   # 400px wide: 1s across
        self.assertEqual(top([100, 100, 400, 50], "B"), 50)    # 50px tall: 1s down
        self.assertEqual(top([100, 100, 400, 30], "B"), 40)    # 30px tall: floor
        self.assertEqual(top([0, 0, 3000, 1000], "R"), 800)    # huge: capped

    @staticmethod
    def tap_on(app, name):
        ev = Ev(name)
        app.on_key(ev)
        app.on_key_release(ev)

    def test_finishing_keys(self):
        for key, kind in (("Return", "image"), ("y", "ocr"), ("P", "pin"), ("p", "text")):
            self.setUp()
            self.tap(key)
            self.assertEqual(self.app.result.kind, kind, key)


if __name__ == "__main__":
    unittest.main()
