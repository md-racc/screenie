"""screenie [--save-dir DIR] [--window] [--lang LANG]

Without --window: the interactive picker (see README.md for the keys).
With --window: the active window, straight away, no picker.

Images always go to the clipboard. With --save-dir they're also saved there;
without, a copy goes to $XDG_RUNTIME_DIR/screenie/ so the last shot can
still be annotated ($XDG_RUNTIME_DIR/screenie/last holds its path).
"""

import argparse
import os
import sys

from . import output, recall
from .frame import Frame
from .hypr import Capture, Layout, capture_window


def parse_args(argv):
    p = argparse.ArgumentParser(prog="screenie", description="Keyboard-driven screenshots for Hyprland.")
    p.add_argument("--save-dir", metavar="DIR",
                   help="also save images here (default: clipboard only)")
    p.add_argument("--window", action="store_true",
                   help="capture the active window immediately, no picker")
    p.add_argument("--lang", default=os.environ.get("SCREENIE_LANG", "eng"),
                   help="tesseract language for OCR (default: eng, or $SCREENIE_LANG)")
    return p.parse_args(argv)


def deliver_image(surface, save_dir, what):
    path = output.save_image(surface, save_dir)
    if save_dir:
        output.notify("Screenshot", f"{what} saved to {path}")
    else:
        output.notify("Screenshot", f"{what} copied to clipboard")


def main(argv=None):
    args = parse_args(sys.argv[1:] if argv is None else argv)
    # The layout picture first, before hyprctl and GTK (see hypr.Capture).
    shot = None if args.window else Capture()
    layout = Layout()

    if args.window:
        if not layout.active:
            output.notify("Screenshot", "No active window")
            return 1
        deliver_image(capture_window(layout.active), args.save_dir,
                      f"{layout.active['class']} window")
        return 0

    from .app import App  # GTK only for the picker

    frame = Frame(shot.surface(), layout.rect)
    app = App(layout, frame)
    action = app.run()
    if action is None:
        return 1
    if action.kind in ("image", "ocr", "pin"):  # for . next time
        recall.remember(layout.monitors, action.rect)

    # The picture is the capture plus the committed layers; view filters
    # (the lenses') never reach the output.
    if action.kind == "image":
        deliver_image(frame.crop(action.rect), args.save_dir, "Area")
    elif action.kind == "ocr":
        output.ocr(frame.ocr_crop(action.rect), frame.k, args.lang)
    elif action.kind == "text":
        output.copy_text(action.text)
        output.notify(action.note, action.text[:200])
    elif action.kind == "pin":
        output.pin(frame.crop(action.rect), action.rect)
    return 0
