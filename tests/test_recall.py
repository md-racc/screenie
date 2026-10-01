"""The . ring: the last accepted boxes, remembered across runs.
Run: python -m unittest discover -s tests"""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from screenie import recall  # noqa: E402
from tests.test_modes import HAVE_DISPLAY, START, Harness  # noqa: E402

LEFT = {"name": "DP-1", "lx": 0, "ly": 0, "lw": 1920, "lh": 1080}
RIGHT = {"name": "DP-2", "lx": 1920, "ly": 0, "lw": 1920, "lh": 1080}


class StateDir(unittest.TestCase):
    """A throwaway $XDG_STATE_HOME for each test."""

    def setUp(self):
        super().setUp()
        self._tmp = tempfile.TemporaryDirectory()
        self._old = os.environ.get("XDG_STATE_HOME")
        os.environ["XDG_STATE_HOME"] = self._tmp.name

    def tearDown(self):
        if self._old is None:
            del os.environ["XDG_STATE_HOME"]
        else:
            os.environ["XDG_STATE_HOME"] = self._old
        self._tmp.cleanup()
        super().tearDown()


class Ring(StateDir):
    def test_empty_until_something_is_remembered(self):
        self.assertEqual(recall.recalled([LEFT, RIGHT]), [])

    def test_newest_first_without_repeats_and_capped(self):
        mons = [LEFT, RIGHT]
        for i in range(recall.RING + 3):
            recall.remember(mons, [i, 0, 50, 50])
        recall.remember(mons, [recall.RING + 1, 0, 50, 50])  # again: moves to the front
        rects = recall.recalled(mons)
        self.assertEqual(len(rects), recall.RING)
        self.assertEqual(rects[0][0], recall.RING + 1)
        self.assertEqual(rects[1][0], recall.RING + 2)
        self.assertEqual(len({r[0] for r in rects}), recall.RING)

    def test_a_box_follows_its_monitor(self):
        recall.remember([LEFT, RIGHT], [2000, 100, 300, 200])  # on DP-2
        swapped = [dict(RIGHT, lx=0), dict(LEFT, lx=1920)]  # the layout rearranged
        self.assertEqual(recall.recalled(swapped), [[80, 100, 300, 200]])

    def test_with_its_monitor_gone_the_global_rect_is_used(self):
        recall.remember([LEFT, RIGHT], [2000, 100, 300, 200])
        self.assertEqual(recall.recalled([LEFT]), [[2000, 100, 300, 200]])

    def test_a_broken_file_is_an_empty_ring(self):
        os.makedirs(os.path.dirname(recall.path()))
        with open(recall.path(), "w") as f:
            f.write("{not json")
        self.assertEqual(recall.recalled([LEFT]), [])
        recall.remember([LEFT], [1, 2, 3, 4])  # and it's replaced, not a crash
        self.assertEqual(recall.recalled([LEFT]), [[1, 2, 3, 4]])


@unittest.skipUnless(HAVE_DISPLAY, "needs a Wayland session")
class Dot(StateDir, Harness):
    def test_dot_with_nothing_remembered_says_so(self):
        self.tap("period")
        self.assertEqual(self.app.sel.rect, START)
        self.assertEqual(self.app.note, "no last selection")

    def test_dot_loads_the_last_box_then_older_ones_and_u_undoes(self):
        a = self.app
        mons = a.layout.monitors
        recall.remember(mons, [500, 500, 200, 100])
        recall.remember(mons, [2000, 50, 400, 300])  # the newest
        self.tap("period")
        self.assertEqual(a.sel.rect, [2000, 50, 400, 300])
        self.assertEqual(a.note, "last 1/2")
        self.tap("period")  # again: the one before
        self.assertEqual(a.sel.rect, [500, 500, 200, 100])
        self.tap("period")  # and round
        self.assertEqual(a.sel.rect, [2000, 50, 400, 300])
        self.tap("l")  # the box changed: . starts from the newest again
        self.tap("period")
        self.assertEqual(a.sel.rect, [2000, 50, 400, 300])
        for _ in range(5):  # each . and the step were one change
            self.tap("u")
        self.assertEqual(a.sel.rect, START)


if __name__ == "__main__":
    unittest.main()
