"""The last accepted boxes, for . in Select.

A small ring, newest first, in $XDG_STATE_HOME/screenie/regions.json. It
holds only coordinates, so it lives in the state dir and outlives logouts
and reboots (unlike the shots in the runtime dir).

Each box is kept relative to the monitor its centre was on, by name, so it
follows that monitor if the layout is rearranged; with the monitor gone,
the global rect is used (and the Selection clamps it to what's there).
"""

import json
import os

from .geometry import contains

RING = 10


def path():
    base = os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state")
    return os.path.join(base, "screenie", "regions.json")


def load():
    try:
        with open(path()) as f:
            data = json.load(f)
    except (OSError, ValueError):
        return []
    if not isinstance(data, list):
        return []
    return [e for e in data if isinstance(e, dict) and len(e.get("rect") or ()) == 4]


def home(monitors, rect):
    """The monitor holding the box's centre, or None."""
    x, y, w, h = rect
    return next((m for m in monitors
                 if contains([m["lx"], m["ly"], m["lw"], m["lh"]], x + w / 2, y + h / 2)), None)


def remember(monitors, rect):
    """Put rect at the front of the ring. A failed write never fails the
    shot, so errors are swallowed."""
    rect = [float(v) for v in rect]
    entry = {"rect": rect}
    m = home(monitors, rect)
    if m is not None and m.get("name"):
        entry["monitor"] = m["name"]
        entry["offset"] = [rect[0] - m["lx"], rect[1] - m["ly"]]
    ring = [entry] + [e for e in load() if e["rect"] != rect]
    p = path()
    try:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p + ".tmp", "w") as f:
            json.dump(ring[:RING], f)
        os.replace(p + ".tmp", p)
    except OSError:
        pass


def resolve(entry, monitors):
    """An entry as a global rect on today's layout."""
    x, y, w, h = entry["rect"]
    name = entry.get("monitor")
    m = next((m for m in monitors if name and m.get("name") == name), None)
    if m is not None and len(entry.get("offset") or ()) == 2:
        x, y = m["lx"] + entry["offset"][0], m["ly"] + entry["offset"][1]
    return [x, y, w, h]


def recalled(monitors):
    """The ring, newest first, as rects on today's layout."""
    return [resolve(e, monitors) for e in load()]
