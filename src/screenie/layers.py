"""Layers: what the modes build on top of the captured picture.

The capture itself is never modified. The first build tool you open
(censor, annotate, filter) opens a layer, and every build tool you use
before committing adds its operations to that same layer, in order. Enter
in any of them commits the layer onto the stack; Esc in any of them throws
it away. The picture you see (and every output) is the capture with the
committed layers applied in order, so order matters: a pixelate censor
pixelates whatever is under it, earlier layers included.

Operations paint in global logical coordinates onto a surface whose
transform maps global coordinates to its pixels.
"""

import math

import cairo

PIXELATE_BLOCK = 10  # censor pixelation block size (logical px), the default


class CensorOp:
    """pixelate (amount: block size, logical px) or bar (amount: its shade,
    0 black .. 1 white)."""

    def __init__(self, rect, kind, amount=None):
        self.rect, self.kind = list(rect), kind
        self.amount = amount if amount is not None else (
            PIXELATE_BLOCK if kind == "pixelate" else 0.0)

    def key(self):
        return ("censor", self.kind, self.amount, tuple(self.rect))

    def bbox(self):
        return list(self.rect)

    def paint(self, cr, target):
        x, y, w, h = self.rect
        if w < 0.5 or h < 0.5:
            return
        cr.save()
        cr.rectangle(x, y, w, h)
        cr.clip()
        if self.kind == "bar":
            cr.set_source_rgb(self.amount, self.amount, self.amount)
            cr.paint()
        else:
            # Downsample what's under the rect (everything painted so far)
            # to blocks, then scale it back up with hard edges.
            sx, sy = cr.user_to_device_distance(w, h)
            block = max(2.0, cr.user_to_device_distance(self.amount, 0)[0])
            sw, sh = max(1, math.ceil(abs(sx) / block)), max(1, math.ceil(abs(sy) / block))
            small = cairo.ImageSurface(cairo.FORMAT_ARGB32, sw, sh)
            sc = cairo.Context(small)
            sc.scale(sw / w, sh / h)
            sc.translate(-x, -y)
            under = cairo.SurfacePattern(target)
            under.set_matrix(cr.get_matrix())  # global -> target pixels
            under.set_filter(cairo.FILTER_GOOD)
            sc.set_source(under)
            sc.paint()
            cr.translate(x, y)
            cr.scale(w / sw, h / sh)
            cr.set_source_surface(small, 0, 0)
            cr.get_source().set_filter(cairo.FILTER_NEAREST)
            cr.paint()
        cr.restore()


def _grown(rect, m):
    x, y, w, h = rect
    return [x - m, y - m, w + 2 * m, h + 2 * m]


def _ink(cr, rgb, width):
    cr.set_source_rgb(*rgb)
    cr.set_line_width(width)
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    cr.set_line_join(cairo.LINE_JOIN_ROUND)


class StrokeOp:
    """A freehand pen line."""

    def __init__(self, points, rgb, width):
        self.points, self.rgb, self.width = list(points), rgb, width

    def key(self):
        return ("stroke", tuple(self.rgb), self.width, tuple(self.points))

    def bbox(self):
        xs, ys = [p[0] for p in self.points], [p[1] for p in self.points]
        return _grown([min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)], self.width)

    def paint(self, cr, target):
        if not self.points:
            return
        _ink(cr, self.rgb, self.width)
        cr.move_to(*self.points[0])
        for p in self.points[1:]:
            cr.line_to(*p)
        if len(self.points) == 1:
            cr.line_to(*self.points[0])  # a click: round caps make it a dot
        cr.stroke()


class ShapeOp:
    """A rectangle, or a circle (an ellipse filling the rect)."""

    def __init__(self, kind, rect, rgb, width):
        self.kind, self.rect, self.rgb, self.width = kind, list(rect), rgb, width

    def key(self):
        return ("shape", self.kind, tuple(self.rect), tuple(self.rgb), self.width)

    def bbox(self):
        return _grown(self.rect, self.width)

    def paint(self, cr, target):
        x, y, w, h = self.rect
        _ink(cr, self.rgb, self.width)
        if self.kind == "rect":
            cr.rectangle(x, y, w, h)
        else:
            cr.save()
            cr.translate(x + w / 2, y + h / 2)
            cr.scale(max(w, 1e-3) / 2, max(h, 1e-3) / 2)
            cr.arc(0, 0, 1, 0, 2 * math.pi)
            cr.restore()
        cr.stroke()


class FilterOp:
    """A filter over a rect, made from cairo blend modes (fast enough to
    preview live, no per-pixel maths):

    grey        amount 0-1: a saturation blend with grey, at that opacity
    invert      a DIFFERENCE paint with white
    brightness  level -5..+5: add white (brighter) / paint black over (darker),
                10% a step
    contrast    level -4..+4: +, overlay-blend the picture onto itself (an
                S-curve around mid-grey) once per step; -, paint mid-grey
                over it, 20% a step
    blur        radius in logical px: scale the rect down and back up smoothly
    """

    def __init__(self, rect, kind, amount):
        self.rect, self.kind, self.amount = list(rect), kind, amount
        self.muted = False  # the filter stack's Space: kept, but not applied

    def key(self):
        # id too: two identical filters in a stack are still two steps
        return ("filter", id(self), self.kind, self.amount, tuple(self.rect), self.muted)

    def bbox(self):
        return list(self.rect)

    def paint(self, cr, target):
        x, y, w, h = self.rect
        if self.muted or w < 0.5 or h < 0.5:
            return
        cr.save()
        cr.rectangle(x, y, w, h)
        cr.clip()
        k, a = self.kind, self.amount
        if k == "grey":
            cr.set_operator(cairo.OPERATOR_HSL_SATURATION)
            cr.set_source_rgb(0.5, 0.5, 0.5)
            cr.paint_with_alpha(a)
        elif k == "invert":
            cr.set_operator(cairo.OPERATOR_DIFFERENCE)
            cr.set_source_rgb(1, 1, 1)
            cr.paint()
        elif k == "brightness" and a > 0:
            cr.set_operator(cairo.OPERATOR_ADD)
            cr.set_source_rgba(1, 1, 1, 0.1 * a)
            cr.paint()
        elif k == "brightness" and a < 0:
            cr.set_source_rgba(0, 0, 0, 0.1 * -a)
            cr.paint()
        elif k == "contrast" and a > 0:
            for _ in range(a):
                snap, dx, dy = self._snapshot(cr, target)
                cr.save()
                cr.identity_matrix()
                cr.set_operator(cairo.OPERATOR_OVERLAY)
                cr.set_source_surface(snap, dx, dy)
                cr.paint()
                cr.restore()
        elif k == "contrast" and a < 0:
            cr.set_source_rgba(0.5, 0.5, 0.5, min(1.0, 0.2 * -a))
            cr.paint()
        elif k == "blur" and a > 0:
            snap, dx, dy = self._snapshot(cr, target)
            radius = max(1.0, cr.user_to_device_distance(a, 0)[0])
            sw = max(1, round(snap.get_width() / radius))
            sh = max(1, round(snap.get_height() / radius))
            small = cairo.ImageSurface(cairo.FORMAT_ARGB32, sw, sh)
            sc = cairo.Context(small)
            sc.scale(sw / snap.get_width(), sh / snap.get_height())
            sc.set_source_surface(snap, 0, 0)
            sc.get_source().set_filter(cairo.FILTER_GOOD)
            sc.paint()
            cr.save()
            cr.identity_matrix()
            cr.translate(dx, dy)
            cr.scale(snap.get_width() / sw, snap.get_height() / sh)
            cr.set_source_surface(small, 0, 0)
            cr.get_source().set_filter(cairo.FILTER_BILINEAR)
            cr.get_source().set_extend(cairo.EXTEND_PAD)
            cr.paint()
            cr.restore()
        cr.restore()

    def _snapshot(self, cr, target):
        """A copy of what's under the rect so far, in device (target) pixels:
        (surface, device x, device y)."""
        x, y, w, h = self.rect
        x0, y0 = cr.user_to_device(x, y)
        x1, y1 = cr.user_to_device(x + w, y + h)
        dx, dy = max(0, int(min(x0, x1))), max(0, int(min(y0, y1)))
        # a pixel of margin, but never past the target (that's transparent,
        # and blur would smear it in from the edge)
        dw = max(1, min(int(abs(x1 - x0)) + 2, target.get_width() - dx))
        dh = max(1, min(int(abs(y1 - y0)) + 2, target.get_height() - dy))
        target.flush()
        snap = cairo.ImageSurface(cairo.FORMAT_ARGB32, dw, dh)
        sc = cairo.Context(snap)
        sc.set_source_surface(target, -dx, -dy)
        sc.paint()
        return snap, dx, dy


class Layer:
    """Operations from the build tools, in order, clipped to the box the
    layer was opened in."""

    def __init__(self, clip, ops):
        self.clip, self.ops = list(clip), list(ops)
        self.hidden = False  # layer view: kept, but left out of the picture

    def describe(self):
        """What's in it, for the layer list: "censor ×2, line"."""
        names = {CensorOp: "censor", StrokeOp: "line", ShapeOp: "shape", FilterOp: "filter"}
        counts = {}
        for op in self.ops:
            n = names.get(type(op), "op")
            counts[n] = counts.get(n, 0) + 1
        return ", ".join(n if c == 1 else f"{n} ×{c}" for n, c in counts.items()) or "empty"

    def bbox(self):
        """The area its ops can touch, within the clip (None if nothing)."""
        boxes = [op.bbox() for op in self.ops]
        if not boxes:
            return None
        x0 = max(min(b[0] for b in boxes), self.clip[0])
        y0 = max(min(b[1] for b in boxes), self.clip[1])
        x1 = min(max(b[0] + b[2] for b in boxes), self.clip[0] + self.clip[2])
        y1 = min(max(b[1] + b[3] for b in boxes), self.clip[1] + self.clip[3])
        return [x0, y0, max(0, x1 - x0), max(0, y1 - y0)]

    def apply(self, target, matrix, only=None):
        """Paint onto target (or just the ops in only); matrix maps global
        coords to target pixels."""
        cr = cairo.Context(target)
        cr.set_matrix(matrix)
        cr.rectangle(*self.clip)
        cr.clip()
        for op in self.ops if only is None else only:
            op.paint(cr, target)
        target.flush()


class LayerEntry:
    """A committed layer, as an undo entry in Select's history."""

    def __init__(self, frame, layer, rect):
        self.frame, self.layer, self.rect = frame, layer, list(rect)

    def undo(self):
        if self.layer in self.frame.layers:  # (it may have been deleted since)
            self.frame.remove_layer(self.layer)
        return self.rect


class LayerDeleted:
    """Layer view's delete, as an undo entry: puts it back where it was."""

    def __init__(self, frame, layer, index, rect):
        self.frame, self.layer, self.index, self.rect = frame, layer, index, list(rect)

    def undo(self):
        self.frame.layers.insert(self.index, self.layer)
        self.frame.rebuild()
        return self.rect


class LayerMoved:
    """Layer view's reorder, as an undo entry: moves it back."""

    def __init__(self, frame, layer, old_index, rect):
        self.frame, self.layer, self.old, self.rect = frame, layer, old_index, list(rect)

    def undo(self):
        if self.layer in self.frame.layers:
            self.frame.layers.remove(self.layer)
            self.frame.layers.insert(self.old, self.layer)
            self.frame.rebuild()
        return self.rect
