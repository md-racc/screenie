"""Tag placement (draw.place). Run: python -m unittest discover -s tests"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from screenie import draw  # noqa: E402
from screenie.draw import LEGEND_SPOTS, LIST_SPOTS, MARGIN, STATUS_SPOTS, place  # noqa: E402

MONS = [[0, 0, 1920, 1080], [1920, 0, 1920, 1080]]
TAG = (250, 20)  # a typical status tag, logical px


class Placement(unittest.TestCase):
    def test_the_ideal_is_outside_bottom_left(self):
        box = [100, 100, 800, 400]  # even when it would fit inside
        self.assertEqual(place(box, *TAG, STATUS_SPOTS, MONS), (100, 500 + MARGIN))

    def test_a_small_box_pushes_it_outside_below(self):
        box = [100, 100, 120, 40]  # the tag would cover it
        self.assertEqual(place(box, *TAG, STATUS_SPOTS, MONS), (100, 140 + MARGIN))

    def test_at_the_bottom_of_the_monitor_it_goes_above(self):
        box = [100, 1050, 120, 30]
        self.assertEqual(place(box, *TAG, STATUS_SPOTS, MONS), (100, 1050 - MARGIN - 20))

    def test_a_box_filling_the_monitor_puts_it_inside(self):
        box = [0, 0, 1920, 1080]
        x, y = place(box, *TAG, STATUS_SPOTS, MONS)
        self.assertEqual((x, y), (MARGIN, 1080 - MARGIN - 20))

    def test_nowhere_fits_falls_back_inside_on_the_monitor(self):
        box = [0, 0, 3840, 1080]  # both monitors, and a list too big to be inside
        x, y = place(box, 900, 700, LIST_SPOTS, MONS)
        self.assertTrue(0 <= x and x + 900 <= 3840 and 0 <= y and y + 700 <= 1080)

    def test_a_box_filling_one_monitor_puts_its_list_on_the_other(self):
        box = [0, 0, 1920, 1080]
        self.assertEqual(place(box, 900, 700, LIST_SPOTS, MONS), (1920 + MARGIN, 0))

    def test_outside_spots_stay_on_one_monitor(self):
        # A tiny box in the bottom-left corner of the right monitor: wherever
        # the tag goes, it's wholly on one monitor.
        box = [1925, 1060, 60, 18]
        x, y = place(box, *TAG, STATUS_SPOTS, MONS)
        self.assertTrue(draw._on_a_monitor((x, y, *TAG), MONS))

    def test_each_kind_of_tag_has_its_own_side(self):
        box = [300, 300, 400, 200]
        status = (*place(box, *TAG, STATUS_SPOTS, MONS), *TAG)
        legend = (*place(box, 300, 20, LEGEND_SPOTS, MONS, avoid=[status]), 300, 20)
        lst = place(box, 200, 60, LIST_SPOTS, MONS, avoid=[status, legend])
        self.assertEqual(status[:2], (300, 500 + MARGIN))       # bottom
        self.assertEqual(legend[:2], (300, 300 - MARGIN - 20))  # top
        self.assertEqual(lst, (700 + MARGIN, 300))              # right side

    def test_a_list_goes_left_when_the_right_is_the_screen_edge(self):
        box = [1700, 300, 200, 200]
        x, y = place(box, 200, 60, LIST_SPOTS, MONS)
        self.assertEqual((x, y), (1700 - MARGIN - 200, 300))

    def test_at_the_bottom_edge_the_status_tag_stays_on_the_bottom_inside(self):
        box = [100, 700, 800, 380]  # flush with the bottom: no room below
        self.assertEqual(place(box, *TAG, STATUS_SPOTS, MONS), (100 + MARGIN, 1080 - MARGIN - 20))

    def test_near_a_monitor_edge_the_tag_right_aligns_under_the_box(self):
        box = [1700, 300, 200, 200]  # a tag left-aligned here would cross 1920
        self.assertEqual(place(box, *TAG, STATUS_SPOTS, MONS), (1900 - 250, 500 + MARGIN))

    def test_tags_keep_clear_of_what_a_mode_occupies(self):
        box = [300, 300, 400, 200]
        loupe = (300, 504, 400, 240)  # say the magnifier sits right under the box
        x, y = place(box, *TAG, STATUS_SPOTS, MONS, avoid=[loupe])
        self.assertFalse(draw._overlaps((x, y, *TAG), loupe))


if __name__ == "__main__":
    unittest.main()
