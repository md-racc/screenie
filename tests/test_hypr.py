"""Which windows count as visible. Run: python -m unittest discover -s tests"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from screenie.hypr import visible_clients  # noqa: E402


def client(name, ws=1, fullscreen=0, floating=False, focus=0, mapped=True, hidden=False):
    return {"class": name, "workspace": {"id": ws}, "fullscreen": fullscreen,
            "floating": floating, "focusHistoryID": focus, "mapped": mapped,
            "hidden": hidden}


def names(clients):
    return [c["class"] for c in clients]


class Visible(unittest.TestCase):
    def test_fullscreen_hides_the_rest_of_its_workspace_only(self):
        cs = [client("tiled", focus=1), client("full", fullscreen=2, focus=0),
              client("float", floating=True, focus=2), client("other", ws=2, focus=3)]
        self.assertEqual(names(visible_clients(cs, {1, 2})), ["full", "other"])

    def test_maximised_hides_tiled_but_not_floating(self):
        cs = [client("tiled", focus=1), client("max", fullscreen=1, focus=0),
              client("float", floating=True, focus=2)]
        self.assertEqual(names(visible_clients(cs, {1})), ["max", "float"])

    def test_plain_workspace_keeps_everything_topmost_first(self):
        cs = [client("b", focus=1), client("a", focus=0), client("off", ws=9)]
        self.assertEqual(names(visible_clients(cs, {1})), ["a", "b"])

    def test_unmapped_and_hidden_are_dropped(self):
        cs = [client("gone", mapped=False), client("tabbed", hidden=True), client("ok")]
        self.assertEqual(names(visible_clients(cs, {1})), ["ok"])


if __name__ == "__main__":
    unittest.main()
