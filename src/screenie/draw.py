"""Small shared drawing helpers (global logical coordinates)."""

import cairo


def label(cr, x, y, text):
    """A text chip whose bottom-left corner is at (x, y). Returns its right
    edge, for putting something after it."""
    cr.select_font_face("monospace")
    cr.set_font_size(13)
    ext = cr.text_extents(text)
    # Height from capitals and digits, not this text's own glyphs: text with
    # descenders (p, y, g) would otherwise grow the chip upward. Descenders
    # sit in the bottom padding.
    cap = cr.text_extents("H0").height
    cr.set_source_rgba(0, 0, 0, 0.92)  # near-opaque: readable on anything
    cr.rectangle(x, y - cap - 8, ext.x_advance + 8, cap + 8)
    cr.fill()
    cr.set_source_rgba(1, 1, 1, 1)
    cr.move_to(x + 4, y - 4)
    cr.show_text(text)
    return x + ext.x_advance + 8


# --- tag placement ------------------------------------------------------------
#
# Tags (the status tag, the layer / filter lists) are blocks of text chips.
# Each is placed at the first candidate spot around the main box that works:
# outside only where it sits wholly on one monitor, inside only while it's
# small next to the box (it mustn't cover most of it), and never on top of a
# block already placed.

ROW = 20          # a chip row, logical px
MARGIN = 4        # between a block and the box edge
INSIDE_MAX = 0.35  # an inside block covers at most this share of the box

# Each kind of tag has its own side of the box, so they never compete for
# the same spot: the status tag (mode, size, cursor px) on the bottom, the
# legends (static key help) on the top, the lists (the layer view, the
# filter stack) on the sides. Outside the box first, so it stays uncovered;
# then inside on that same side (while it's small next to the box); then
# the other sides.
STATUS_SPOTS = ("below-l", "below-r", "inside-bl", "above-l", "above-r", "right-b", "left-b")
LEGEND_SPOTS = ("above-l", "above-r", "inside-tl", "below-l", "below-r", "right-t", "left-t")
LIST_SPOTS = ("right-t", "left-t", "inside-tl", "below-l", "above-l")


def text_width(cr, text):
    cr.select_font_face("monospace")
    cr.set_font_size(13)
    return cr.text_extents(text).x_advance + 8


def _spot(name, box, bw, bh):
    x, y, w, h = box
    m = MARGIN
    return {
        "inside-bl": (x + m, y + h - m - bh),
        "inside-tl": (x + m, y + m),
        "below-l": (x, y + h + m),
        "below-r": (x + w - bw, y + h + m),  # right-aligned: near a monitor's right edge
        "above-l": (x, y - m - bh),
        "above-r": (x + w - bw, y - m - bh),
        "right-b": (x + w + m, y + h - bh),
        "right-t": (x + w + m, y),
        "left-b": (x - m - bw, y + h - bh),
        "left-t": (x - m - bw, y),
    }[name]


def _overlaps(a, b):
    return a[0] < b[0] + b[2] and b[0] < a[0] + a[2] and a[1] < b[1] + b[3] and b[1] < a[1] + a[3]


def _on_a_monitor(r, monitors):
    x, y, w, h = r
    return any(mx <= x and my <= y and x + w <= mx + mw and y + h <= my + mh
               for mx, my, mw, mh in monitors)


def place(box, bw, bh, spots, monitors, avoid=()):
    """Where a bw x bh block goes (its top-left): the first spot that fits.
    If none does, inside the box's first-choice corner, clamped onto the
    monitor under the box's centre."""
    x, y, w, h = box
    away = {"above": (0, -1), "below": (0, 1), "right": (1, 0), "left": (-1, 0)}
    for name in spots:
        bx, by = _spot(name, box, bw, bh)
        if name.startswith("inside"):
            r = (bx, by, bw, bh)
            fits = (bw <= w - 2 * MARGIN and bh <= h - 2 * MARGIN
                    and bw * bh <= INSIDE_MAX * w * h
                    and not any(_overlaps(r, o) for o in avoid))
        else:
            # Outside, a block already there doesn't rule the side out: slide
            # on past it, away from the box (a list above the status tag).
            dx, dy = away[name.split("-")[0]]
            for _ in range(len(avoid)):
                hit = next((o for o in avoid if _overlaps((bx, by, bw, bh), o)), None)
                if hit is None:
                    break
                if dy:
                    by = hit[1] - MARGIN - bh if dy < 0 else hit[1] + hit[3] + MARGIN
                else:
                    bx = hit[0] - MARGIN - bw if dx < 0 else hit[0] + hit[2] + MARGIN
            r = (bx, by, bw, bh)
            fits = _on_a_monitor(r, monitors) and not any(_overlaps(r, o) for o in avoid)
        if fits:
            return bx, by
    bx, by = _spot(spots[0], box, bw, bh)
    cx, cy = x + w / 2, y + h / 2
    mon = next((m for m in monitors
                if m[0] <= cx < m[0] + m[2] and m[1] <= cy < m[1] + m[3]), monitors[0])
    bx = max(mon[0], min(bx, mon[0] + mon[2] - bw))
    by = max(mon[1], min(by, mon[1] + mon[3] - bh))
    return bx, by


def block(cr, lines, box, spots, monitors, avoid=(), extra=0):
    """Place and draw a block of chips, one per line (extra: width reserved
    after the widest, e.g. for a swatch). Returns its rect, and the right
    edge of each chip (for drawing things after them)."""
    bw = max(text_width(cr, t) for t in lines) + extra
    bh = ROW * len(lines)
    bx, by = place(box, bw, bh, spots, monitors, avoid)
    ends = [label(cr, bx, by + ROW * (i + 1) - 2, t) for i, t in enumerate(lines)]
    return (bx, by, bw, bh), ends


def swatch(cr, x, y, rgb, size=13):
    """A colour square whose bottom-left corner is at (x, y), with a thin
    two-tone border so it shows on any background (and for white / black)."""
    cr.set_source_rgb(*rgb)
    cr.rectangle(x, y - size - 2, size, size)
    cr.fill_preserve()
    cr.set_line_width(1)
    cr.set_source_rgba(0, 0, 0, 0.8)
    cr.stroke()
    cr.set_source_rgba(1, 1, 1, 0.8)
    cr.rectangle(x - 1, y - size - 3, size + 2, size + 2)
    cr.stroke()


def pixel_cursor(cr, frame, px, py):
    """The cursor on one frame pixel (insert, loupe): a solid 3x3 on it and a
    hollow 15x15 around it, drawn in frame pixels so it sits exactly on the
    pixel grid at any scale, and inverted (DIFFERENCE with white) so it
    shows on any colour."""
    cr.save()
    in_frame(cr, frame)
    cr.set_operator(cairo.OPERATOR_DIFFERENCE)
    cr.set_source_rgb(1, 1, 1)
    cr.rectangle(px - 1, py - 1, 3, 3)
    cr.fill()
    cr.set_line_width(1)
    cr.rectangle(px - 7 + 0.5, py - 7 + 0.5, 14, 14)
    cr.stroke()
    cr.restore()


def marker(cr, x, y, rgb=(1, 1, 1)):
    """The filled square that marks a box's cursor."""
    cr.set_source_rgba(*rgb, 1)
    cr.rectangle(x - 4, y - 4, 8, 8)
    cr.fill()


def outline(cr, r, rgba=(1, 1, 1, 0.9), width=2):
    x, y, w, h = r
    cr.set_line_width(width)
    cr.set_source_rgba(*rgba)
    cr.rectangle(x - width / 2, y - width / 2, w + width, h + width)
    cr.stroke()


def dim_outside(cr, outer, inner, alpha=0.5):
    cr.set_fill_rule(cairo.FILL_RULE_EVEN_ODD)
    cr.rectangle(*outer)
    cr.rectangle(*inner)
    cr.set_source_rgba(0, 0, 0, alpha)
    cr.fill()
    cr.set_fill_rule(cairo.FILL_RULE_WINDING)


def in_frame(cr, frame):
    """Switch cr from global logical to frame-pixel coordinates."""
    cr.translate(frame.ox, frame.oy)
    cr.scale(1 / frame.k, 1 / frame.k)
