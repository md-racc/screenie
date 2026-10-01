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
    from screenie.modes.filter import describe  # noqa: E402
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


class Harness(unittest.TestCase):
    """A fresh App on the synthetic frame, and key helpers."""

    def setUp(self):
        self.app = App(fake_layout(), fake_frame())
        self.app.finish = lambda action=None: setattr(self.app, "result", action)

    def pic(self, r=START):
        """The view: what you see, pending layer included."""
        return self.app.frame.crop(r).get_data().tobytes()

    def committed(self, r=START):
        """The committed picture only (what a capture right now would hold
        if the pending layer were thrown away)."""
        f = self.app.frame
        x, y, w, h = f.to_image(r)
        cr_ = cairo.Context(out := cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h))
        cr_.set_source_surface(f.committed, -x, -y)
        cr_.paint()
        return out.get_data().tobytes()

    def key(self, name, **kw):
        ev = Ev(name, **kw)
        self.app.on_key(ev)
        return ev

    def tap(self, name, **kw):
        self.app.on_key_release(self.key(name, **kw))


@unittest.skipUnless(HAVE_DISPLAY, "needs a Wayland session")
class Modes(Harness):
    def test_view_filters_live_in_the_lenses(self):
        a = self.app
        self.tap("e")
        self.tap("v")
        self.assertEqual(a.filter, "grey")
        self.tap("V")
        self.assertEqual(a.filter, "off")
        self.tap("V")
        self.assertEqual(a.filter, "contrast")
        self.tap("Escape")
        before = self.pic()
        self.assertEqual(a.frame.layers, [])  # a view filter never bakes in
        self.assertEqual(self.pic(), before)

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

    def test_censor_plus_minus_pixelation_and_shade(self):
        a = self.app
        self.tap("x")
        cen = a.mode
        self.assertEqual(cen.describe(), "pixelate 10px")  # the defaults
        self.tap("plus")
        self.assertEqual(cen.describe(), "pixelate 12px")
        for _ in range(30):
            self.tap("minus")
        self.assertEqual(cen.describe(), "pixelate 4px")  # the floor
        self.tap("t")
        self.assertEqual(cen.describe(), "black bar")
        for _ in range(3):
            self.tap("plus")
        self.assertEqual(cen.describe(), "grey bar 30%")
        for _ in range(20):
            self.tap("plus")
        self.assertEqual(cen.describe(), "white bar")
        self.tap("x")  # the next keeps its type and shade
        self.assertEqual(cen.current().amount, 1.0)
        self.tap("t")
        self.assertEqual(cen.describe(), "pixelate 4px")  # each type kept its own
        cen.sel.set_rect([200, 180, 40, 40])
        self.tap("t")
        self.tap("Return")
        self.assertEqual(a.frame.rgb(220, 200), (255, 255, 255))  # a white bar

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
        self.assertEqual(a.mode.name, "loupe")  # back to where the layer opened

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

    def test_x_in_censor_locks_and_starts_the_next_enter_commits(self):
        a = self.app
        before = self.pic()
        self.tap("x")
        first = list(a.mode.sel.rect)
        self.tap("x")  # lock the pixelate, start the next
        cen = a.mode
        self.assertEqual(cen.name, "censor")
        self.assertEqual(len(a.layer.ops), 1)
        self.assertEqual(cen.sel.rect, first)  # the next starts where that one was
        self.assertEqual(self.committed(), before)  # locked, not committed yet
        self.tap("t")  # this one's a black bar
        for _ in range(8):
            self.tap("L", shift=True)
        self.tap("x")
        self.assertEqual([op.kind for op in a.layer.ops], ["pixelate", "bar"])
        self.tap("u")  # in the mode: drops the last locked censor
        self.assertEqual(len(a.layer.ops), 1)
        self.tap("Return")  # locks the current one, commits the layer
        self.assertEqual(a.mode.name, "select")
        self.assertEqual(len(a.frame.layers), 1)
        self.assertEqual(len(a.frame.layers[0].ops), 2)
        self.assertNotEqual(self.pic(), before)
        self.tap("u")  # in Select: the whole layer
        self.assertEqual(a.frame.layers, [])
        self.assertEqual(self.pic(), before)

    def test_tools_share_one_layer(self):
        """Model B: annotate, then a filter over it, then a censor, all one
        layer, committed in one go."""
        a = self.app
        before = self.pic()
        self.tap("a")
        self.tap("5")  # blue
        for _ in range(10):
            self.tap("plus")
        a.on_press(1, 370, 205)
        a.on_release(1, 390, 225)
        self.tap("f")  # straight to the filter tool, same layer
        self.assertEqual(a.mode.name, "filter")
        self.assertEqual(a.mode.sel.rect, START)  # the whole box by default
        self.tap("1")  # a grey over it
        self.tap("f")  # and a duplicate
        self.tap("x")  # lock that filter again (a copy) and switch to censor
        self.assertEqual(a.mode.name, "censor")
        self.assertEqual([type(op).__name__ for op in a.layer.ops],
                         ["StrokeOp", "FilterOp", "FilterOp"])
        self.tap("u")  # drops the last op, whichever tool made it
        self.assertEqual(len(a.layer.ops), 2)
        r, g, b = a.frame.rgb(380, 215)
        self.assertEqual(r, g)  # the blue line, greyed by the filter after it
        self.assertEqual(g, b)
        self.assertEqual(self.committed(), before)
        self.tap("Return")  # locks the censor, commits the one layer
        self.assertEqual((a.mode.name, len(a.frame.layers)), ("select", 1))
        self.assertEqual(len(a.frame.layers[0].ops), 3)
        self.tap("u")
        self.assertEqual(self.pic(), before)

    def test_the_layer_view(self):
        a = self.app
        self.tap("1")  # no layers yet: nothing happens
        self.assertEqual(a.mode.name, "select")
        for keys in (["x"], ["f", "1"], ["x"]):  # three layers
            for k in keys:
                self.tap(k)
            self.tap("Return")
        first = a.frame.layers[0]
        self.tap("2")
        lv = a.mode
        self.assertEqual((lv.name, lv.index), ("layers", 1))
        self.assertEqual(lv.status(), "2/3  filter")
        self.tap("bracketleft")  # the layer below
        self.assertEqual(lv.index, 0)
        self.tap("greater")  # move layer 1 up: it now applies second
        self.assertIs(a.frame.layers[1], first)
        self.assertEqual(lv.index, 1)
        self.tap("space")  # hide it: out of the picture, still listed
        self.assertTrue(first.hidden)
        self.tap("space")
        self.tap("d")  # delete it
        self.assertEqual(len(a.frame.layers), 2)
        self.assertNotIn(first, a.frame.layers)
        self.tap("Escape")
        self.assertEqual(a.mode.name, "select")
        self.tap("u")  # the delete, back in its place
        self.assertIs(a.frame.layers[1], first)
        self.tap("u")  # the move
        self.assertIs(a.frame.layers[0], first)

    def test_the_layer_view_shows_a_layer_alone(self):
        a = self.app
        before = self.pic()
        self.tap("x")
        self.tap("t")
        self.tap("Return")  # layer 1: a black censor at the centre
        self.tap("f")
        self.tap("2")
        self.tap("Return")  # layer 2: invert the whole box
        cx, cy = map(int, a.sel.centre())
        self.assertEqual(a.frame.rgb(cx, cy), (255, 255, 255))  # black, inverted
        patch, (px, py, _, _) = a.frame.isolated(a.frame.layers[0])
        patch.flush()
        i = (cy - py) * patch.get_stride() + (cx - px) * 4
        self.assertEqual(tuple(patch.get_data()[i:i + 3]), (0, 0, 0))  # layer 1 alone: black
        self.assertEqual(self.committed(), self.pic())
        self.assertNotEqual(self.pic(), before)

    def test_filter_keys_pick_the_filter_and_plus_minus_adjust_it(self):
        a = self.app
        self.tap("f")
        flt = a.mode
        self.assertEqual((flt.name, flt.stack, flt.status()), ("filter", [], "raw"))
        self.tap("plus")  # nothing to adjust yet
        self.assertEqual(flt.palette(), ["1 grey  2 invert  3 bright  4 contrast  5 blur"])
        self.tap("1")
        self.tap("minus")
        self.assertEqual(describe(flt.op), "grey 75%")
        self.tap("3")  # a second filter, on top
        for _ in range(3):
            self.tap("minus")
        self.assertEqual(describe(flt.op), "brightness -2")
        self.tap("4")
        for _ in range(9):
            self.tap("plus")
        self.assertEqual(describe(flt.op), "contrast +4")  # capped
        self.tap("5")
        self.tap("plus")
        self.assertEqual(describe(flt.op), "blur 6px")
        self.tap("2")
        self.tap("plus")  # invert has no amount
        self.assertEqual(describe(flt.op), "invert")
        self.assertEqual([op.kind for op in flt.stack],
                         ["grey", "brightness", "contrast", "blur", "invert"])
        for _ in range(20):  # the box reshapes the selected filter only
            self.tap("h")
        self.assertLess(flt.op.rect[2], START[2])
        self.assertEqual(flt.stack[0].rect, START)
        self.tap("Return")  # the whole stack, folded into one layer
        self.assertEqual(len(a.frame.layers), 1)
        self.assertEqual(len(a.frame.layers[0].ops), 5)

    def test_new_filters_go_just_above_the_selected_one(self):
        a = self.app
        self.tap("f")
        flt = a.mode
        self.tap("1")
        self.tap("3")
        self.tap("bracketleft")  # select grey (the bottom)
        self.tap("2")            # invert lands between grey and brightness
        self.assertEqual([op.kind for op in flt.stack], ["grey", "invert", "brightness"])
        self.assertEqual([op.kind for op in a.layer.ops], ["grey", "invert", "brightness"])
        self.assertEqual(flt.op.kind, "invert")  # and is selected
        self.tap("f")            # a duplicate goes just above it too
        self.assertEqual([op.kind for op in flt.stack],
                         ["grey", "invert", "invert", "brightness"])
        self.assertEqual(flt.selected, 2)

    def test_filter_stack_select_move_delete_mute_duplicate(self):
        a = self.app
        self.tap("f")
        flt = a.mode
        for k in "132":
            self.tap(k)
        self.assertEqual(flt.selected, 2)
        self.tap("bracketleft")
        self.assertEqual(flt.op.kind, "brightness")
        self.tap("greater")  # brightness up above invert
        self.assertEqual([op.kind for op in flt.stack], ["grey", "invert", "brightness"])
        self.assertEqual([op.kind for op in a.layer.ops], ["grey", "invert", "brightness"])
        self.tap("space")
        self.assertTrue(flt.op.muted)
        before = self.pic()
        self.tap("space")
        self.assertNotEqual(self.pic(), before)  # unmuted: it applies again
        self.tap("f")  # duplicate it, just above it (it's on top here)
        self.assertEqual(len(flt.stack), 4)
        self.assertIsNot(flt.stack[3], flt.stack[2])
        dup = flt.op
        self.tap("d")
        self.assertEqual(len(flt.stack), 3)
        self.tap("u")  # undo the last action: the delete
        self.assertEqual(len(flt.stack), 4)
        self.assertIs(flt.stack[3], dup)
        self.tap("u")  # then the duplicate
        self.tap("u")  # then the mute (unmuted state back to muted)
        self.assertTrue(flt.stack[2].muted)

    def test_each_filter_changes_the_picture(self):
        a = self.app
        before = self.pic()
        for keys in (["1"], ["2"], ["3"], ["3", "minus", "minus"], ["4"],
                     ["4", "minus", "minus"], ["5"]):
            self.setUp()
            a = self.app
            self.tap("f")
            for k in keys:
                self.tap(k)
            self.tap("Return")
            if keys != ["1"]:  # the synthetic frame is already grey
                self.assertNotEqual(self.pic(), before, keys)

    def test_lenses_see_the_pending_layer(self):
        a = self.app
        self.tap("x")
        self.tap("t")  # black
        self.tap("x")  # lock it; the next censor starts on top of it
        self.tap("z")  # the loupe, from inside the censor tool
        loupe = a.mode
        self.assertEqual(loupe.name, "loupe")
        self.assertEqual(a.frame.rgb(loupe.px, loupe.py), (0, 0, 0))  # sees the censor
        self.assertEqual(a.frame.layers, [])  # still only pending

    def test_layers_apply_in_order(self):
        """A pixelate over an annotation pixelates the annotation too."""
        a = self.app
        self.tap("a")
        self.tap("5")  # blue
        for _ in range(10):
            self.tap("plus")
        a.on_press(1, 370, 205)
        a.on_motion(390, 225)
        a.on_release(1, 390, 225)
        self.tap("Return")
        blue = a.frame.rgb(380, 215)
        self.assertGreater(blue[2], 200)
        self.tap("x")  # a pixelate right over it
        a.mode.sel.set_rect([360, 195, 40, 40])
        self.tap("Return")
        after = a.frame.rgb(380, 215)
        self.assertNotEqual(after, blue)  # averaged with its surroundings
        self.assertGreater(after[2], after[0])  # ...but the blue is still in there
        self.tap("u")  # undo the censor layer: the line is back, crisp
        self.assertEqual(a.frame.rgb(380, 215), blue)
        self.tap("u")
        self.assertEqual(a.frame.layers, [])

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

    def test_escape_twice_throws_locked_work_away(self):
        a = self.app
        before = self.pic()
        self.tap("x")
        self.tap("x")  # one censor locked
        self.assertNotEqual(self.pic(), before)       # you see it...
        self.assertEqual(self.committed(), before)    # ...but it isn't committed
        self.tap("Escape")  # there's work in it: only a warning
        self.assertEqual(a.mode.name, "censor")
        _, _, pw, ph = a.frame.to_image(START)
        self.assertIn("Esc again to discard 1", a.modes[0].label(pw, ph))
        self.tap("l")  # any other key disarms it
        self.assertFalse(a.discard_armed)
        self.tap("Escape")
        a.on_press(2, 500, 500)  # the middle click is Esc too
        self.assertEqual((a.mode.name, a.layer, a.frame.layers), ("select", None, []))
        self.assertIsNone(a.result)
        self.assertEqual(self.pic(), before)

    def test_escape_with_nothing_locked_leaves_at_once(self):
        a = self.app
        self.tap("x")
        self.tap("Escape")
        self.assertEqual((a.mode.name, a.layer), ("select", None))

    def test_escape_in_a_shape_only_drops_the_shape(self):
        a = self.app
        self.tap("a")
        self.tap("r")
        self.tap("a")  # one locked
        self.tap("c")
        self.tap("Escape")
        self.assertEqual((a.mode.name, len(a.layer.ops)), ("annotate", 1))

    def test_censor_mouse_drag_draws_the_censor_box(self):
        a = self.app
        self.tap("x")
        a.on_press(1, 150, 180)
        a.on_motion(300, 240)
        a.on_release(1, 300, 240)
        self.assertEqual(a.mode.sel.rect, [150, 180, 150, 60])
        self.assertIsNone(a.result)  # a drag never finishes anything

    def test_right_click_is_enter(self):
        a = self.app
        self.tap("x")
        a.on_press(3, 500, 500)  # in a build tool: commits the layer
        a.on_release(3, 500, 500)
        self.assertEqual((a.mode.name, len(a.frame.layers)), ("select", 1))
        self.assertIsNone(a.result)
        a.on_press(3, 500, 500)  # in Select: captures
        a.on_release(3, 500, 500)
        self.assertEqual((a.result.kind, a.result.rect), ("image", START))

    def test_enhance_lens_zooms_and_stays_inside_the_box(self):
        a = self.app
        self.tap("e")
        en = a.mode
        self.assertEqual(en.name, "enhance")
        self.assertEqual(en.lens.bounds, START)
        z = en.zoom
        self.tap("plus")
        self.assertAlmostEqual(en.zoom, z + 0.5)
        self.tap("minus")
        self.assertAlmostEqual(en.zoom, z)
        for _ in range(500):
            self.tap("l")  # plain hjkl move the lens
        x, y, w, h = en.lens.rect
        self.assertEqual(x + w, START[0] + START[2])  # stopped at the box edge
        self.assertEqual(a.sel.rect, START)  # the box itself is frozen
        self.tap("Escape")
        self.assertEqual(a.mode.name, "select")

    def test_enhance_zoom_taps_step_by_half(self):
        a = self.app
        self.tap("e")
        en = a.mode
        seen = []
        for k in ("plus", "plus", "minus"):
            self.tap(k)
            seen.append(en.zoom)
        self.assertEqual(seen, [2.5, 3.0, 2.5])
        en.zoom = 2.73  # after a continuous zoom, a tap lands on the next mark
        a.on_scroll(type("Scroll", (), {"direction": Gdk.ScrollDirection.UP})())  # the wheel is +
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
        self.tap("Return")  # commit: back to insert, cursor unmoved
        self.assertEqual(a.mode.name, "insert")
        self.assertEqual((ins.px, ins.py), cursor)
        self.tap("x")
        self.tap("Escape")  # the layer away: back to insert
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

    def test_plus_minus_and_the_wheel_change_the_scope(self):
        a = self.app
        self.tap("plus")
        self.assertEqual(a.sel.rect, WINDOWS[2])  # the window the box is in
        a.on_scroll(type("Scroll", (), {"direction": Gdk.ScrollDirection.UP})())
        self.assertEqual(a.sel.rect, [0, 0, 1920, 1080])  # its monitor
        self.tap("minus")
        self.assertEqual(a.sel.rect, WINDOWS[2])

    def test_annotate_pen_layer_is_clipped_and_committed_on_enter(self):
        a = self.app
        before = self.pic()
        outside = self.pic([0, 0, 90, 160])
        self.tap("a")
        self.assertEqual(a.mode.name, "annotate")
        self.tap("4")
        self.tap("plus")
        self.assertEqual((a.ink[0], a.ink_width), ("green", 4))
        a.on_press(1, 50, 150)     # starts outside the box...
        a.on_motion(300, 200)      # ...ends inside it
        a.on_release(1, 300, 200)
        self.assertEqual(self.committed(), before)  # locked into the layer only
        a.on_press(1, 200, 240)
        a.on_release(1, 200, 240)  # a dot
        self.tap("u")              # drops the dot
        self.assertEqual(len(a.layer.ops), 1)
        self.tap("Return")
        self.assertEqual(a.mode.name, "select")
        self.assertNotEqual(self.pic(), before)
        self.assertEqual(self.pic([0, 0, 90, 160]), outside)  # clipped to the box
        r, g, b = a.frame.rgb(200, 180)  # on the line: green
        self.assertTrue(g > 150 and r < 80, (r, g, b))
        self.tap("u")
        self.assertEqual(self.pic(), before)

    def test_annotate_colours_and_eyedropper(self):
        a = self.app
        self.tap("a")
        names = []
        for k in "12345":
            self.tap(k)
            names.append(a.ink[0])
        self.assertEqual(names, ["white", "black", "red", "green", "blue"])
        a.on_motion(125.5, 196.5)  # over the text's light grey
        self.tap("p")
        r, g, b = a.frame.rgb(125, 196)
        self.assertEqual(a.ink[0], f"#{r:02x}{g:02x}{b:02x}")
        self.assertEqual(a.ink[1], (r / 255, g / 255, b / 255))
        self.tap("3")  # a number key replaces it
        self.assertEqual(a.ink[0], "red")

    def test_a_locks_the_shape_and_goes_back_for_another(self):
        a = self.app
        before = self.pic()
        self.tap("a")
        self.tap("a")  # pen mode: a does nothing
        self.assertEqual(a.mode.name, "annotate")
        ann = a.mode
        self.tap("r")
        self.tap("a")  # lock the rect, back to annotate
        self.assertEqual(a.mode, ann)
        self.tap("c")
        self.tap("a")  # lock a circle too
        self.assertEqual([op.kind for op in a.layer.ops], ["rect", "circle"])
        self.assertEqual(self.committed(), before)  # nothing committed yet
        self.tap("c")
        self.tap("u")  # in a shape: drops the last locked shape
        self.assertEqual(len(a.layer.ops), 1)
        self.tap("Escape")
        self.tap("Return")  # commit from annotate
        self.assertEqual(a.mode.name, "select")
        self.assertEqual(len(a.frame.layers), 1)
        self.assertNotEqual(self.pic(), before)

    def test_holding_plus_keeps_widening_the_line(self):
        a = self.app
        self.tap("a")
        ev = self.key("plus")
        for _ in range(5):  # GTK autorepeat: same keycode, no release
            a.on_key(ev)
        a.on_key_release(ev)
        self.assertEqual(a.ink_width, 3 + 6)
        self.tap("c")  # ...in a shape box too
        ev = self.key("minus")
        a.on_key(ev)
        a.on_key_release(ev)
        self.assertEqual(a.ink_width, 3 + 6 - 2)
        ev = self.key("t")  # but other keys still fire once per press
        a.on_key(ev)
        a.on_key(ev)
        self.assertEqual(a.mode.kind, "rect")

    def test_the_tag_tracks_the_active_cursor(self):
        a = self.app
        select = a.modes[0]
        _, _, pw, ph = a.frame.to_image(START)
        self.assertIn("(659,259)", select.label(pw, ph))  # bottom-right corner
        self.tap("o")
        self.assertIn("(100,170)", select.label(pw, ph))  # top-left
        self.tap("O")
        self.assertIn("(659,170)", select.label(pw, ph))  # top-right
        self.tap("space")  # Space is o: the opposite corner
        self.assertIn("(100,259)", select.label(pw, ph))  # bottom-left
        self.tap("space")
        self.assertIn("(659,170)", select.label(pw, ph))
        self.tap("space", shift=True)  # Shift+Space is O: the sideways corner
        self.assertIn("(100,170)", select.label(pw, ph))
        self.tap("i")
        ins = a.mode
        self.assertIn(f"({ins.px},{ins.py})", select.label(pw, ph))
        self.tap("l")
        self.assertIn(f"({ins.px},{ins.py})", select.label(pw, ph))  # follows it
        self.tap("z")
        self.tap("h")
        self.assertIn(f"({a.mode.px},{a.mode.py})", select.label(pw, ph))  # the loupe's

    def test_label_shows_the_mode(self):
        a = self.app
        _, _, pw, ph = a.frame.to_image(START)
        select = a.modes[0]
        self.assertEqual(select.label(pw, ph), f"SELECT  {pw}×{ph}  (659,259)")  # the corner
        self.tap("a")
        self.assertEqual(select.label(pw, ph), f"ANNOTATE  {pw}×{ph}  red 3px  layer")  # no cursor
        self.tap("Escape")
        self.assertTrue(select.label(pw, ph).startswith("SELECT"))
        self.tap("i")
        self.assertTrue(select.label(pw, ph).startswith("INSERT"))

    def test_annotate_shapes_edit_like_a_box(self):
        a = self.app
        before = self.pic()
        self.tap("a")
        self.tap("r")
        sh = a.mode
        self.assertEqual((sh.name, sh.kind), ("shape", "rect"))
        self.assertEqual(sh.sel.bounds, START)
        self.tap("l")                  # box keys shape it
        self.tap("t")
        self.assertEqual(sh.kind, "circle")
        self.tap("Escape")             # drop just this shape
        self.assertEqual(a.mode.name, "annotate")
        self.assertEqual(a.layer.ops, [])
        self.tap("r")
        self.tap("Return")             # lock it and commit the whole layer
        self.assertEqual(a.mode.name, "select")
        self.assertNotEqual(self.pic(), before)
        self.tap("u")
        self.assertEqual(self.pic(), before)

    def test_every_mode_draws(self):
        """Smoke test: draw each mode (with pending layers) on both monitors."""
        a = self.app

        def render():
            for mon in MONS:
                s = cairo.ImageSurface(cairo.FORMAT_ARGB32, 1920, 1080)
                a.draw(cairo.Context(s), mon)

        render()                                   # select
        for keys in (["i"], ["i", "z"], ["i", "z", "x"], ["i", "z", "x", "x"]):
            self.setUp()
            a = self.app
            for k in keys:
                self.tap(k)
            render()                               # insert / loupe / censor (+ locked)
        self.setUp()
        a = self.app
        self.tap("e")
        render()                                   # enhance
        self.tap("Escape")
        self.tap("a")
        a.on_press(1, 150, 200)
        a.on_motion(250, 220)
        render()                                   # annotate, mid-stroke
        a.on_release(1, 250, 220)
        self.tap("r")
        self.tap("a")
        self.tap("c")
        render()                                   # shape over a pending layer
        self.tap("Return")
        render()                                   # committed
        self.tap("f")
        render()                                   # filter tool
        self.tap("Return")
        self.tap("1")
        render()                                   # the layer view

    def test_enter_and_right_click_back_out_of_the_views(self):
        a = self.app
        for keys in (["i"], ["z"], ["e"], ["i", "z"]):
            self.setUp()
            a = self.app
            for k in keys:
                self.tap(k)
            depth = len(a.modes)
            a.on_press(3, 500, 500)  # right click = Enter = back
            self.assertEqual(len(a.modes), depth - 1, keys)
            self.assertIsNone(a.result)

    def test_the_loupe_opens_every_build_tool_at_its_pixel(self):
        for key, name in (("x", "censor"), ("a", "annotate"), ("f", "filter")):
            self.setUp()
            a = self.app
            self.tap("z")
            self.tap(key)
            self.assertEqual(a.mode.name, name)
            self.assertIsNotNone(a.layer)

    def test_the_modifier_grammar(self):
        a = self.app
        x, y, w, h = START
        a.alt = True
        a.mode.on_dir("L")  # Alt: halve
        self.assertEqual(a.sel.rect, [x, y, w / 2, h])
        a.shift = True
        a.mode.on_dir("R")  # Alt+Shift: double
        self.assertEqual(a.sel.rect, [x, y, w, h])
        a.alt = False
        a.mode.on_dir("R")  # Shift: move the box
        self.assertEqual(a.sel.rect, [x + 5, y, w, h])
        a.shift = False
        a.mode.on_dir("R")  # plain: the cursor corner moves
        self.assertEqual(a.sel.rect, [x + 5, y, w + 5, h])

    def test_q_quits_from_any_mode(self):
        for keys in ([], ["i"], ["z"], ["e"], ["x"], ["a", "r"], ["f"]):
            self.setUp()
            a = self.app
            quit_ = []
            a.finish = lambda action=None, q=quit_: q.append(action)
            for k in keys:
                self.tap(k)
            self.tap("q")
            self.assertEqual(quit_, [None], keys)  # quit, no output

    def test_q_with_locked_work_asks_twice(self):
        a = self.app
        quit_ = []
        a.finish = lambda action=None: quit_.append(action)
        self.tap("x")
        self.tap("x")  # a censor locked
        self.tap("q")
        self.assertEqual(quit_, [])  # only a warning
        _, _, pw, ph = a.frame.to_image(START)
        self.assertIn("q again to quit, discarding 1", a.modes[0].label(pw, ph))
        self.tap("t")  # any other key takes it back
        self.assertFalse(a.quit_armed)
        self.tap("q")
        self.tap("q")
        self.assertEqual(quit_, [None])

    def test_finishing_keys(self):
        for key, kind in (("Return", "image"), ("y", "ocr"), ("P", "pin"), ("p", "text")):
            self.setUp()
            self.tap(key)
            self.assertEqual(self.app.result.kind, kind, key)


if __name__ == "__main__":
    unittest.main()
