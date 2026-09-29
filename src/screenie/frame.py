"""The frozen frame: one grim capture of the whole layout, and every image
operation on it (view filters, crops, pixel reads, the layer stack).

Three pictures, one per stage:
- base: the capture, never modified;
- committed: the capture with the committed layers applied, in order;
- surface (the view): committed plus the open, pending layer. Everything
  that shows or reads the picture (display, lenses, eyedropper, outputs)
  reads the view.

The view is rebuilt lazily (at most once per frame, however many edits
land in between) and only inside the open layer's clip, from cached
snapshots of the unchanged ops below the one that changed: adjusting the
top of a filter stack doesn't re-run the rest.

View filters (the lenses') are cairo blend modes over the box only: a
saturation blend with grey drops colour, overlay-blending the picture onto
itself is an S-curve around mid-grey, a DIFFERENCE paint with white inverts.
"""

import math

import cairo

from .layers import LayerEntry

FILTERS = ["off", "grey", "invert", "contrast"]
VIEW_CONTRAST = 2  # S-curve passes + 1 for the contrast view filter
SNAPSHOTS = 12     # pending-layer prefix snapshots kept (clip-sized each)


def rnd(v):
    """Round half up: the one rule for logical -> frame pixels, so a box's
    crop, its edge pixels and the cursor pixel always agree (Python's round
    is half-to-even, and at 1.5x scale edges land on .5 all the time)."""
    return math.floor(v + 0.5)


def copy_of(surface):
    out = cairo.ImageSurface(cairo.FORMAT_ARGB32, surface.get_width(), surface.get_height())
    cr = cairo.Context(out)
    cr.set_source_surface(surface, 0, 0)
    cr.paint()
    return out


def cut(surface, px_rect):
    """A copy of px_rect of surface."""
    x, y, w, h = px_rect
    out = cairo.ImageSurface(cairo.FORMAT_ARGB32, max(1, w), max(1, h))
    cr = cairo.Context(out)
    cr.set_source_surface(surface, -x, -y)
    cr.paint()
    return out


class Frame:
    def __init__(self, surface, layout_rect):
        self.base = self.committed = surface
        self.layers = []
        self.pending = None
        self.ox, self.oy, self.W, self.H = layout_rect
        # grim renders the layout at the highest monitor scale.
        self.k = surface.get_width() / self.W
        self.version = 0       # bumps whenever the view changes
        self._view = surface
        self._dirty = False
        self._snapshots = []   # [(clip px rect, op keys so far, patch)]
        self._processed = {}   # (level, px rect) -> region, for this version
        self._previews = {}    # op keys -> (patch, px rect), for this version
        self._isolated = {}    # layer id -> (patch, px rect)

    # --- coordinates ----------------------------------------------------------

    def to_image(self, r):
        """Global logical rect -> pixel rect in the frame."""
        x, y, w, h = r
        k = self.k
        x0, y0 = rnd((x - self.ox) * k), rnd((y - self.oy) * k)
        x1, y1 = rnd((x + w - self.ox) * k), rnd((y + h - self.oy) * k)
        return x0, y0, max(0, x1 - x0), max(0, y1 - y0)

    def to_global(self, px, py):
        """Centre of a frame pixel, in global logical coordinates."""
        return self.ox + (px + 0.5) / self.k, self.oy + (py + 0.5) / self.k

    def clamp_px(self, px, py):
        w, h = self.base.get_width(), self.base.get_height()
        return max(0, min(px, w - 1)), max(0, min(py, h - 1))

    def pixel_under(self, gx, gy):
        """The frame pixel under a global point (a click, a cursor)."""
        return self.clamp_px(math.floor((gx - self.ox) * self.k),
                             math.floor((gy - self.oy) * self.k))

    def edge_pixel(self, gx, gy, right, bottom):
        """The pixel at a box corner that's inside the box: a left / top
        edge's own pixel, or the one just before a right / bottom edge, by
        the same rounding as to_image, so it's always inside the crop."""
        px = rnd((gx - self.ox) * self.k) - (1 if right else 0)
        py = rnd((gy - self.oy) * self.k) - (1 if bottom else 0)
        return self.clamp_px(px, py)

    def rgb(self, px, py):
        s = self.surface
        s.flush()
        data, stride = s.get_data(), s.get_stride()
        i = py * stride + px * 4
        b, g, r = data[i:i + 3]  # cairo ARGB32 is little-endian BGRA
        return r, g, b

    # --- the view (committed + the pending layer) ------------------------------

    @property
    def surface(self):
        if self._dirty:
            self._rebuild_view()
        return self._view

    def matrix(self, px_offset=(0, 0)):
        """Global logical coords -> frame pixels (minus px_offset)."""
        k = self.k
        return cairo.Matrix(k, 0, 0, k, -self.ox * k - px_offset[0], -self.oy * k - px_offset[1])

    def _changed(self):
        self.version += 1
        self._processed.clear()
        self._previews.clear()

    def set_pending(self, layer):
        """The open layer changed (opened, edited, closed). The view is rebuilt
        when it's next read."""
        self.pending = layer
        self._dirty = True
        self._changed()

    def _clip_px(self, clip):
        x, y, w, h = self.to_image(clip)
        W, H = self.base.get_width(), self.base.get_height()
        x, y = max(0, x), max(0, y)
        return x, y, max(0, min(w, W - x)), max(0, min(h, H - y))

    def _rebuild_view(self):
        self._dirty = False
        layer = self.pending
        if not layer or not layer.ops:
            self._view = self.committed
            return
        clip = self._clip_px(layer.clip)
        keys = [op.key() for op in layer.ops]
        # the longest run of ops that still match a snapshot
        start, patch = 0, None
        for c, ks, p in self._snapshots:
            if c == clip and len(ks) > start and keys[:len(ks)] == ks:
                start, patch = len(ks), p
        patch = copy_of(patch) if patch is not None else cut(self.committed, clip)
        m = self.matrix(clip[:2])
        snaps = [s for s in self._snapshots if s[0] == clip and keys[:len(s[1])] == s[1]]
        for i in range(start, len(keys)):
            layer.apply(patch, m, only=[layer.ops[i]])
            snaps.append((clip, keys[:i + 1], copy_of(patch)))
        self._snapshots = snaps[-SNAPSHOTS:]
        view = copy_of(self.committed)
        cr = cairo.Context(view)
        cr.set_source_surface(patch, clip[0], clip[1])
        cr.paint()
        view.flush()
        self._view = view

    def rebuild(self):
        """The committed picture from scratch: the capture, then every visible
        layer in order; then the view."""
        self.committed = self.base
        if any(not l.hidden for l in self.layers):
            self.committed = copy_of(self.base)
            for layer in self.layers:
                if not layer.hidden:
                    layer.apply(self.committed, self.matrix())
        self._snapshots = []
        self.set_pending(self.pending)

    def add_layer(self, layer, main_rect):
        """Commit a layer onto the stack (closing it); returns its undo entry."""
        self.layers.append(layer)
        if self.committed is self.base:
            self.committed = copy_of(self.base)
        layer.apply(self.committed, self.matrix())
        self._snapshots = []
        self.set_pending(None)
        return LayerEntry(self, layer, main_rect)

    def remove_layer(self, layer):
        self.layers.remove(layer)
        self._isolated.pop(id(layer), None)
        self.rebuild()

    def isolated(self, layer):
        """Layer view: the capture with only this layer applied, over the
        layer's clip (hiding what was below and above it): (patch, px rect),
        cached."""
        if id(layer) not in self._isolated:
            clip = self._clip_px(layer.clip)
            patch = cut(self.base, clip)
            layer.apply(patch, self.matrix(clip[:2]))
            self._isolated[id(layer)] = (patch, clip)
        return self._isolated[id(layer)]

    def preview(self, cr, layer):
        """Draw ops (the item being edited) over the view, into cr (global
        coords): recomputed only over the ops' own area, and only when they
        change (so every monitor reuses one render)."""
        keys = tuple(op.key() for op in layer.ops)
        if not keys:
            return
        if keys not in self._previews:
            area = layer.bbox()
            px = self._clip_px(area) if area else (0, 0, 0, 0)
            if px[2] < 1 or px[3] < 1:
                self._previews[keys] = None
            else:
                patch = cut(self.surface, px)
                layer.apply(patch, self.matrix(px[:2]))
                self._previews[keys] = (patch, px)
        hit = self._previews[keys]
        if hit:
            patch, (x, y, _, _) = hit
            cr.save()
            cr.translate(self.ox, self.oy)
            cr.scale(1 / self.k, 1 / self.k)
            cr.set_source_surface(patch, x, y)
            cr.paint()
            cr.restore()

    # --- view filters (the lenses') and crops -----------------------------------

    def processed(self, level, px_rect):
        """px_rect of the view in greyscale with level-1 contrast passes: a
        region surface, cached for this version of the view."""
        key = (level, tuple(px_rect))
        if key not in self._processed:
            out = cut(self.surface, px_rect)
            w, h = out.get_width(), out.get_height()
            cr = cairo.Context(out)
            cr.set_operator(cairo.OPERATOR_HSL_SATURATION)
            cr.set_source_rgb(0.5, 0.5, 0.5)
            cr.paint()
            for _ in range(level - 1):
                snap = cut(out, (0, 0, w, h))
                cr.set_operator(cairo.OPERATOR_OVERLAY)
                cr.set_source_surface(snap, 0, 0)
                cr.paint()
            out.flush()
            self._processed[key] = out
        return self._processed[key]

    @staticmethod
    def is_dark(surface, px_rect):
        """Mean grey over a sample grid (tesseract wants dark text on light)."""
        x0, y0, w, h = px_rect
        if w < 1 or h < 1:
            return False
        surface.flush()
        data, stride = surface.get_data(), surface.get_stride()
        n = total = 0
        for j in range(20):
            y = y0 + (j * 2 + 1) * h // 40
            for i in range(40):
                x = x0 + (i * 2 + 1) * w // 80
                total += data[y * stride + x * 4 + 1]
                n += 1
        return total / n < 128

    def source(self, px_rect, filt):
        """(surface, its x, its y, invert?) for a view filter over px_rect."""
        x, y, w, h = px_rect
        if filt in ("grey", "contrast") and w >= 1 and h >= 1:
            pp = self.processed(1 if filt == "grey" else VIEW_CONTRAST, px_rect)
            dark = filt == "contrast" and self.is_dark(pp, (0, 0, w, h))
            return pp, x, y, dark
        return self.surface, 0, 0, filt == "invert"

    def paint(self, cr, px_rect, filt="off", sharp=False):
        """Paint the view, view-filtered over px_rect, into cr (frame-pixel
        coordinates, clipped by the caller)."""
        src, sx, sy, invert = self.source(px_rect, filt)
        cr.set_source_surface(src, sx, sy)
        cr.get_source().set_filter(cairo.FILTER_NEAREST if sharp else cairo.FILTER_GOOD)
        cr.paint()
        if invert:
            cr.set_operator(cairo.OPERATOR_DIFFERENCE)
            cr.set_source_rgb(1, 1, 1)
            cr.paint()
            cr.set_operator(cairo.OPERATOR_OVER)

    def crop(self, r, filt="off"):
        """The rect of the view at full resolution (at least 1x1)."""
        x, y, w, h = self.to_image(r)
        w, h = max(1, w), max(1, h)
        out = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
        cr = cairo.Context(out)
        cr.translate(-x, -y)
        cr.rectangle(x, y, w, h)
        cr.clip()
        self.paint(cr, (x, y, w, h), filt)
        return out

    def ocr_crop(self, r):
        """For OCR: greyscale with dark backgrounds flipped; it measured
        cleaner and faster than raw pixels."""
        x, y, w, h = self.to_image(r)
        w, h = max(1, w), max(1, h)
        grey = self.processed(1, (x, y, w, h))
        out = copy_of(grey)
        if self.is_dark(grey, (0, 0, w, h)):
            cr = cairo.Context(out)
            cr.set_operator(cairo.OPERATOR_DIFFERENCE)
            cr.set_source_rgb(1, 1, 1)
            cr.paint()
        return out
