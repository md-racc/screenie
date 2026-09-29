"""Regressions for the multi-agent audit's findings (bugs and UX gaps).
Run: python -m unittest discover -s tests"""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import cairo  # noqa: E402

from tests.test_modes import HAVE_DISPLAY, MONS, START, Harness  # noqa: E402

if HAVE_DISPLAY:
    from screenie.frame import Frame  # noqa: E402
    from screenie.output import unique_path  # noqa: E402


@unittest.skipUnless(HAVE_DISPLAY, "needs a Wayland session")
class Bugs(Harness):
    def render(self):
        s = cairo.ImageSurface(cairo.FORMAT_ARGB32, 100, 100)
        for mon in MONS:
            self.app.draw(cairo.Context(s), mon)

    def test_b1_q_armed_then_right_click_commit_draws(self):
        a = self.app
        self.tap("x")
        self.tap("x")
        self.tap("q")  # armed: work in the layer
        a.on_press(3, 10, 10)  # right click = Enter: commits
        self.render()  # used to raise on a.layer = None
        self.assertFalse(a.quit_armed)

    def test_b2_click_in_a_sub_box_places_its_corner(self):
        a = self.app
        self.tap("x")
        a.on_press(1, 400, 240)
        a.on_release(1, 400, 240)
        x, y, w, h = a.mode.sel.rect
        self.assertEqual((x + w, y + h), (400, 240))  # the cursor corner, not the whole box
        self.assertLess(w, START[2])

    def test_b3_drag_past_the_bounds_stops_at_the_edge(self):
        a = self.app
        self.tap("x")
        a.on_press(1, 600, 200)
        a.on_motion(700, 230)
        self.assertEqual(a.mode.sel.rect, [600, 200, 60, 30])

    def test_b4_enter_mid_stroke_keeps_the_line(self):
        a = self.app
        self.tap("a")
        a.on_press(1, 200, 200)
        a.on_motion(300, 220)
        a.on_press(3, 300, 220)  # right click = Enter
        a.on_release(1, 300, 220)
        self.assertEqual(len(a.frame.layers), 1)

    def test_b5_new_work_takes_the_discard_warning_back(self):
        a = self.app
        self.tap("a")
        a.on_press(1, 200, 200)
        a.on_release(1, 250, 220)
        self.tap("Escape")
        self.assertTrue(a.discard_armed)
        a.on_press(1, 300, 200)  # another line
        a.on_release(1, 350, 220)
        self.assertFalse(a.discard_armed)
        a.on_press(2, 0, 0)  # middle click = Esc: only warns again
        self.assertEqual((a.mode.name, len(a.layer.ops)), ("annotate", 2))

    def test_b6_no_stale_drag_after_a_mode_change(self):
        a = self.app
        a.on_press(1, 300, 300)
        a.on_motion(400, 400)
        self.tap("i")
        a.on_release(1, 400, 400)  # the release goes to Insert
        self.tap("Escape")
        before = list(a.sel.rect)
        a.on_motion(900, 700)  # no button held
        self.assertEqual(a.sel.rect, before)

    def test_b6_held_key_across_a_mode_change_is_not_a_new_press(self):
        a = self.app
        self.tap("z")
        h = self.key("h")  # held in the loupe...
        self.tap("Escape")
        before = list(a.sel.rect)
        a.on_key(h)  # ...its autorepeat arrives in Select
        self.assertEqual(a.sel.rect, before)
        a.on_key_release(h)
        self.tap("h")  # a real press works again
        self.assertNotEqual(a.sel.rect, before)

    def test_b7_scrolling_down_a_drawn_box_leaves_it(self):
        a = self.app
        a.on_press(1, 200, 300)
        a.on_motion(400, 500)
        a.on_release(1, 400, 500)
        self.tap("minus")
        self.assertEqual(a.sel.rect, [200, 300, 200, 200])

    def test_b8_the_cursor_pixel_is_inside_the_crop_at_1_5x(self):
        f = Frame(cairo.ImageSurface(cairo.FORMAT_ARGB32, 1500, 900), [0, 0, 1000, 600])
        for rect in ([101, 10, 100, 50], [103, 10, 100, 50]):
            x, y, w, h = f.to_image(rect)
            left = f.edge_pixel(rect[0], rect[1], False, False)
            right = f.edge_pixel(rect[0] + rect[2], rect[1], True, False)
            self.assertEqual((left[0], right[0]), (x, x + w - 1), rect)

    def test_b9_a_sub_pixel_box_doesnt_capture(self):
        a = self.app
        a.sel.set_rect([100.5, 170, 0.6, 90])
        self.tap("Return")
        self.assertIsNone(a.result)
        self.assertEqual(a.note, "empty box")

    def test_b10_the_loupe_stays_on_one_monitor_and_off_the_box(self):
        a = self.app
        a.sel.set_rect([1700, 100, 200, 100])  # near the seam at 1920
        self.tap("z")
        lp = a.mode
        gx, gy = a.frame.to_global(lp.px, lp.py)
        size = a.loupe_px * 10
        x, y = lp.where(gx, gy, size)
        on_one = any(mx <= x and x + size <= mx + mw for mx, _, mw, _ in a.monitor_rects)
        self.assertTrue(on_one)
        box = a.sel.rect
        self.assertFalse(x < box[0] + box[2] and box[0] < x + size
                         and y < box[1] + box[3] and box[1] < y + size)

    def test_b11_same_second_shots_get_their_own_names(self):
        with tempfile.TemporaryDirectory() as d:
            p1 = unique_path(d, "stamp")
            open(p1, "w").close()
            p2 = unique_path(d, "stamp")
            self.assertNotEqual(p1, p2)


@unittest.skipUnless(HAVE_DISPLAY, "needs a Wayland session")
class Gaps(Harness):
    render = Bugs.render

    def test_lenses_open_from_every_mode(self):
        for keys in (["x"], ["f"], ["a"], ["a", "r"], ["i"], ["z"], ["e"]):
            for lens, name in (("z", "loupe"), ("e", "enhance")):
                self.setUp()
                a = self.app
                for k in keys:
                    self.tap(k)
                if a.mode.name == name:  # (the lens itself: z / e close or reopen)
                    continue
                self.tap(lens)
                self.assertEqual(a.mode.name, name, keys)

    def test_plus_minus_in_insert_is_the_step(self):
        a = self.app
        self.tap("i")
        ins = a.mode
        self.tap("plus")
        self.assertEqual(ins.stepsize, 6)
        x = ins.px
        self.tap("l")
        self.assertEqual(ins.px, x + 6)
        self.tap("u")  # back where it was
        self.assertEqual(ins.px, x)

    def test_u_in_the_layer_view_undoes_a_delete(self):
        a = self.app
        self.tap("x")
        self.tap("Return")
        self.tap("1")
        self.tap("d")
        self.assertEqual(a.mode.name, "select")  # the last layer: back out
        self.tap("1")  # nothing to view
        self.tap("u")  # Select's u brings it back
        self.assertEqual(len(a.frame.layers), 1)

    def test_click_on_empty_space_snaps_to_the_monitor(self):
        a = self.app
        a.on_press(1, 1000, 500)  # no window at x=1000 (between 958 and 1923)
        a.on_release(1, 1000, 500)
        self.assertEqual(a.sel.rect, [0, 0, 1920, 1080])

    def test_click_in_enhance_centres_the_lens(self):
        a = self.app
        self.tap("e")
        a.on_press(1, 200, 200)
        a.on_release(1, 200, 200)
        self.assertEqual(a.mode.lens.centre(), (240.0, 200.0))  # x as near as it fits

    def test_the_legend_is_behind_question_mark_and_only_the_top_modes(self):
        a = self.app
        self.tap("x")
        self.assertFalse(a.show_help)
        self.tap("question")
        self.assertTrue(a.show_help)
        self.assertTrue(a.mode.legend()[0].startswith("t pixelate"))
        self.render()

    def test_filter_and_annotate_always_show_what_they_pick(self):
        a = self.app
        self.assertFalse(a.show_help)
        self.tap("f")
        self.assertIn("5 blur", a.mode.palette()[0])
        self.assertNotIn("5 blur", " ".join(a.mode.legend()))
        self.render()
        self.tap("Escape")
        self.tap("a")
        self.assertEqual(a.mode.palette(), ["r rectangle  c circle"])
        self.tap("r")
        self.assertEqual(a.mode.palette(), ["t rectangle/circle"])
        self.render()

    def test_filter_undo_is_the_last_action_anywhere_in_the_stack(self):
        a = self.app
        self.tap("f")
        flt = a.mode
        self.tap("1")
        self.tap("2")
        self.tap("bracketleft")
        self.tap("3")  # brightness goes mid-stack
        self.tap("plus")
        self.tap("plus")  # a run of + is one step
        self.assertEqual(flt.op.amount, 3)
        self.tap("u")
        self.assertEqual(flt.op.amount, 1)
        self.assertEqual([op.kind for op in flt.stack], ["grey", "brightness", "invert"])
        self.tap("u")  # the insert, mid-stack, not the top
        self.assertEqual([op.kind for op in flt.stack], ["grey", "invert"])


if __name__ == "__main__":
    unittest.main()
