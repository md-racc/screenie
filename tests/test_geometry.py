"""Selection behaviour. Run: python -m unittest discover -s tests"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from screenie.geometry import Selection, snap_targets  # noqa: E402

LAYOUT = [0, 0, 3840, 1080]
MONITORS = [[0, 0, 1920, 1080], [1920, 0, 1920, 1080]]
WINDOWS = [[1923, 3, 955, 1074], [2883, 3, 954, 1074], [3, 3, 955, 1074]]


def sel(rect=(2000, 100, 200, 100), bounds=LAYOUT):
    return Selection(list(rect), bounds, WINDOWS, MONITORS, snap_targets(WINDOWS + MONITORS))


class Cursor(unittest.TestCase):
    def test_tap_moves_the_cursor_corner(self):
        s = sel()
        s.cursor_by("R", 5)
        self.assertEqual(s.rect, [2000, 100, 205, 100])

    def test_crossing_the_anchor_flips_the_cursor(self):
        s = sel()
        for _ in range(50):
            s.cursor_by("L", 5)
        self.assertEqual(s.rect, [1950, 100, 50, 100])
        self.assertEqual(s.cur, ["L", "B"])

    def test_o_swaps_to_the_opposite_corner(self):
        s = sel()
        s.swap_corner(both=True)
        self.assertEqual(s.cur, ["L", "T"])
        s.swap_corner(both=False)
        self.assertEqual(s.cur, ["R", "T"])

    def test_centre_mode_h_grows_j_shrinks_symmetrically(self):
        s = sel()
        s.toggle_center()
        s.cursor_by("L", 5)
        self.assertEqual(s.rect, [1995, 100, 210, 100])
        s.cursor_by("B", 5)
        self.assertEqual(s.rect, [1995, 105, 210, 90])

    def test_centre_mode_stops_at_zero(self):
        s = sel()
        s.toggle_center()
        for _ in range(100):
            s.cursor_by("B", 5)
        self.assertEqual(s.rect[3], 0)
        self.assertEqual(s.centre()[1], 150)


class Snapping(unittest.TestCase):
    def test_jump_skips_the_window_gap(self):
        s = sel()
        s.cursor_jump("R")
        self.assertEqual(s.edges()["R"], 2878)
        s.cursor_jump("R")  # 2883 is across a 5px gap: skipped
        self.assertEqual(s.edges()["R"], 3837)

    def test_neighbour_skips_the_window_the_box_is_in(self):
        s = sel()
        s.neighbour("R")
        self.assertEqual(s.rect, [2883, 3, 954, 1074])

    def test_neighbour_prefers_windows_over_monitors(self):
        s = sel(WINDOWS[0])
        s.neighbour("L")
        self.assertEqual(s.rect, [3, 3, 955, 1074])

    def test_zoom_out_cycles_window_monitor_all(self):
        s = sel()
        seen = []
        for _ in range(4):
            s.zoom_out()
            seen.append(list(s.rect))
        self.assertEqual(seen, [WINDOWS[0], MONITORS[1], LAYOUT, WINDOWS[0]])


class Fullscreen(unittest.TestCase):
    """With a fullscreen window on the left monitor, hypr.visible_clients
    leaves only it there, so these are the windows the picker sees."""

    FS = [0, 0, 1920, 1080]
    RIGHT = [[1923, 3, 955, 1074], [2883, 3, 954, 1074]]

    def sel(self, rect):
        wins = [self.FS] + self.RIGHT
        return Selection(list(rect), LAYOUT, wins, MONITORS, snap_targets(wins + MONITORS))

    def test_ctrl_l_then_ctrl_h_is_a_round_trip(self):
        s = self.sel(self.FS)
        s.neighbour("R")
        self.assertEqual(s.rect, self.RIGHT[0])
        s.neighbour("L")
        self.assertEqual(s.rect, self.FS)

    def test_from_a_floating_window_back_to_the_one_under_it(self):
        maxed, floating = [0, 0, 1920, 1080], [200, 300, 400, 300]
        s = Selection(list(floating), LAYOUT, [floating, maxed], MONITORS,
                      snap_targets([floating, maxed] + MONITORS))
        s.neighbour("R")  # nothing beside it that way: the window it sits on
        self.assertEqual(s.rect, maxed)


class Bounds(unittest.TestCase):
    def test_a_censor_box_stays_inside_the_main_box(self):
        s = sel((2050, 125, 100, 50), bounds=[2000, 100, 200, 100])
        s.move(1000, 1000)
        self.assertEqual(s.rect, [2100, 150, 100, 50])
        s.cursor_jump("R", to_limit=True)
        self.assertEqual(s.edges()["R"], 2200)


class Undo(unittest.TestCase):
    def test_undo_restores_rects_and_calls_entries(self):
        class Entry:
            def undo(self):
                return [1, 2, 3, 4]
        s = sel()
        s.remember()
        s.move(10, 0)
        s.history.append(Entry())
        s.undo()
        self.assertEqual(s.rect, [1, 2, 3, 4])
        s.undo()
        self.assertEqual(s.rect, [2000, 100, 200, 100])


if __name__ == "__main__":
    unittest.main()
