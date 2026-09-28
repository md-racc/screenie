"""Small shared drawing helpers (global logical coordinates)."""

import cairo


def label(cr, x, y, text):
    """A text chip whose bottom-left corner is at (x, y)."""
    cr.select_font_face("monospace")
    cr.set_font_size(13)
    ext = cr.text_extents(text)
    cr.set_source_rgba(0, 0, 0, 0.7)
    cr.rectangle(x, y - ext.height - 8, ext.x_advance + 8, ext.height + 8)
    cr.fill()
    cr.set_source_rgba(1, 1, 1, 1)
    cr.move_to(x + 4, y - 4)
    cr.show_text(text)


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
