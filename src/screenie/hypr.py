"""Hyprland state and screen capture (hyprctl + grim)."""

import json
import os
import subprocess
import tempfile

import cairo


def hyprctl(*args):
    return json.loads(subprocess.check_output(["hyprctl", "-j", *args]))


def visible_clients(clients, ws_ids):
    """The clients you can actually see on the workspaces in ws_ids,
    topmost first. A fullscreen window hides everything else on its
    workspace; a maximised one hides the other tiled windows, but floating
    ones stay above it. Hidden windows would otherwise be Tab / Ctrl+hjkl
    targets you could land on but never see (or leave)."""
    shown = [c for c in clients
             if c["mapped"] and not c["hidden"] and c["workspace"]["id"] in ws_ids]
    visible = []
    for c in shown:
        others = [o for o in shown
                  if o is not c and o["workspace"]["id"] == c["workspace"]["id"]]
        if c["fullscreen"] == 0 and any(o["fullscreen"] >= 2 for o in others):
            continue  # behind a fullscreen window
        if (c["fullscreen"] == 0 and not c["floating"]
                and any(o["fullscreen"] == 1 for o in others)):
            continue  # tiled, behind a maximised window
        visible.append(c)
    return sorted(visible, key=lambda c: c["focusHistoryID"])


class Layout:
    """Monitors, visible windows and the active window, in global logical
    coordinates (hyprctl's monitor width/height are physical)."""

    def __init__(self):
        self.monitors = hyprctl("monitors")
        for m in self.monitors:
            m["lx"], m["ly"] = m["x"], m["y"]
            m["lw"], m["lh"] = m["width"] / m["scale"], m["height"] / m["scale"]
            if m["transform"] % 2:  # rotated 90/270
                m["lw"], m["lh"] = m["lh"], m["lw"]
        self.focused = next(m for m in self.monitors if m["focused"])

        ws_ids = {m["specialWorkspace"]["id"] if m["specialWorkspace"]["name"]
                  else m["activeWorkspace"]["id"] for m in self.monitors}
        clients = visible_clients(hyprctl("clients"), ws_ids)  # topmost first
        self.windows = [self.client_rect(c) for c in clients]

        active = hyprctl("activewindow")
        self.active = active if active.get("address") and active["workspace"]["id"] in ws_ids else None

        ox = min(m["lx"] for m in self.monitors)
        oy = min(m["ly"] for m in self.monitors)
        self.rect = [ox, oy,
                     max(m["lx"] + m["lw"] for m in self.monitors) - ox,
                     max(m["ly"] + m["lh"] for m in self.monitors) - oy]

    @staticmethod
    def client_rect(c):
        return [*c["at"], *c["size"]]

    @staticmethod
    def monitor_rect(m):
        return [m["lx"], m["ly"], m["lw"], m["lh"]]

    def start_rect(self):
        """Where the box starts: the active window, else the middle of the
        focused monitor."""
        if self.active:
            return self.client_rect(self.active)
        f = self.focused
        return [f["lx"] + f["lw"] / 4, f["ly"] + f["lh"] / 4, f["lw"] / 2, f["lh"] / 2]


def grim(*args):
    """Run grim with args and load the PNG it writes (uncompressed, fast)."""
    fd, tmp = tempfile.mkstemp(prefix="screenie-", suffix=".png")
    os.close(fd)
    try:
        subprocess.run(["grim", "-l", "0", *args, tmp], check=True)
        return cairo.ImageSurface.create_from_png(tmp)
    finally:
        os.unlink(tmp)


def capture_layout():
    return grim()


def capture_window(client):
    """Just that window's own pixels (foreign-toplevel capture), even if it's
    partly covered."""
    return grim("-T", client["stableId"])
