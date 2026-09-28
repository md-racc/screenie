"""The frozen frame: one grim capture of the whole layout, and every image
operation on it (filters, crops, pixel reads, censoring).

Filters are built from cairo blend modes rather than per-pixel maths, which
would be too slow in Python to preview live: a saturation blend with grey
drops colour, overlay-blending the image onto itself is an S-curve around
mid-grey, and a DIFFERENCE paint with white inverts.
"""

import cairo

FILTERS = ["off", "grey", "invert", "contrast"]
CONTRAST_MAX = 4       # past ~2 the S-curve starts eating letter strokes
PIXELATE_BLOCK = 10    # censor pixelation block size (logical px)


class Censor:
    """An applied censor, as an undo entry for the main selection."""

    def __init__(self, frame, px_rect, backup, rect):
        self.frame, self.px_rect, self.backup, self.rect = frame, px_rect, backup, rect

    def undo(self):
        self.frame.restore(self.px_rect, self.backup)
        return self.rect


class Frame:
    def __init__(self, surface, layout_rect):
        self.surface = surface
        self.ox, self.oy, self.W, self.H = layout_rect
        # grim renders the layout at the highest monitor scale.
        self.k = surface.get_width() / self.W
        self._processed = {}  # contrast level -> processed full frame

    # --- coordinates ----------------------------------------------------------

    def to_image(self, r):
        """Global logical rect -> pixel rect in the frame."""
        x, y, w, h = r
        k = self.k
        x0, y0 = round((x - self.ox) * k), round((y - self.oy) * k)
        return x0, y0, round((x - self.ox + w) * k) - x0, round((y - self.oy + h) * k) - y0

    def to_global(self, px, py):
        """Centre of a frame pixel, in global logical coordinates."""
        return self.ox + (px + 0.5) / self.k, self.oy + (py + 0.5) / self.k

    def pixel_under(self, gx, gy, inside_right=False, inside_bottom=False):
        """The frame pixel under a global point. A point on a box's far edge
        belongs to the pixel just inside it (inside_right / inside_bottom)."""
        w, h = self.surface.get_width(), self.surface.get_height()
        px = int((gx - self.ox) * self.k - (1e-6 if inside_right else 0))
        py = int((gy - self.oy) * self.k - (1e-6 if inside_bottom else 0))
        return max(0, min(px, w - 1)), max(0, min(py, h - 1))

    def rgb(self, px, py):
        self.surface.flush()
        data, stride = self.surface.get_data(), self.surface.get_stride()
        i = py * stride + px * 4
        b, g, r = data[i:i + 3]  # cairo ARGB32 is little-endian BGRA
        return r, g, b

    # --- filters ----------------------------------------------------------------

    def processed(self, level):
        """The whole frame in greyscale with level-1 contrast passes, cached.
        Overlay is symmetric, so inverting afterwards commutes with it."""
        if level not in self._processed:
            w, h = self.surface.get_width(), self.surface.get_height()
            out = cairo.ImageSurface(cairo.FORMAT_RGB24, w, h)
            cr = cairo.Context(out)
            cr.set_source_surface(self.surface, 0, 0)
            cr.paint()
            cr.set_operator(cairo.OPERATOR_HSL_SATURATION)
            cr.set_source_rgb(0.5, 0.5, 0.5)
            cr.paint()
            for _ in range(level - 1):
                copy = cairo.ImageSurface(cairo.FORMAT_RGB24, w, h)
                c2 = cairo.Context(copy)
                c2.set_source_surface(out, 0, 0)
                c2.paint()
                cr.set_operator(cairo.OPERATOR_OVERLAY)
                cr.set_source_surface(copy, 0, 0)
                cr.paint()
            out.flush()
            self._processed[level] = out
        return self._processed[level]

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

    def source(self, px_rect, filt, contrast):
        """(full-frame surface, invert?) for a filter over px_rect."""
        if filt == "grey":
            return self.processed(1), False
        if filt == "invert":
            return self.surface, True
        if filt == "contrast":  # the OCR-tuned one: dark backgrounds flip
            pp = self.processed(contrast)
            return pp, self.is_dark(pp, px_rect)
        return self.surface, False

    def paint(self, cr, px_rect, filt="off", contrast=1, sharp=False):
        """Paint the filtered frame into cr, which must be in frame-pixel
        coordinates and clipped by the caller."""
        src, invert = self.source(px_rect, filt, contrast)
        cr.set_source_surface(src, 0, 0)
        cr.get_source().set_filter(cairo.FILTER_NEAREST if sharp else cairo.FILTER_GOOD)
        cr.paint()
        if invert:
            cr.set_operator(cairo.OPERATOR_DIFFERENCE)
            cr.set_source_rgb(1, 1, 1)
            cr.paint()
            cr.set_operator(cairo.OPERATOR_OVER)

    def crop(self, r, filt="off", contrast=1):
        """The rect at full resolution, filtered."""
        x, y, w, h = self.to_image(r)
        out = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
        cr = cairo.Context(out)
        cr.translate(-x, -y)
        self.paint(cr, (x, y, w, h), filt, contrast)
        return out

    def ocr_crop(self, r, filt, contrast):
        # With no useful filter chosen, OCR still gets greyscale + dark-flip:
        # it measured cleaner and faster than raw pixels.
        if filt in ("off", "invert"):
            return self.crop(r, "contrast", 1)
        return self.crop(r, filt, contrast)

    # --- censoring ------------------------------------------------------------

    def pixelated(self, px_rect):
        """px_rect downsampled to PIXELATE_BLOCK blocks: (small surface,
        scale back up). Paint it with FILTER_NEAREST to get the blocks."""
        x, y, w, h = px_rect
        block = max(2, round(PIXELATE_BLOCK * self.k))
        sw, sh = max(1, -(-w // block)), max(1, -(-h // block))
        small = cairo.ImageSurface(cairo.FORMAT_ARGB32, sw, sh)
        sc = cairo.Context(small)
        sc.scale(sw / w, sh / h)
        sc.set_source_surface(self.surface, -x, -y)
        sc.get_source().set_filter(cairo.FILTER_GOOD)
        sc.paint()
        return small, (w / sw, h / sh)

    def paint_censor(self, cr, px_rect, kind):
        """Paint a censor over px_rect into cr (frame-pixel coordinates);
        used both for the live preview and to apply it."""
        x, y, w, h = px_rect
        if w < 1 or h < 1:
            return
        cr.save()
        cr.rectangle(x, y, w, h)
        cr.clip()
        if kind == "black":
            cr.set_source_rgb(0, 0, 0)
            cr.paint()
        else:
            small, (sx, sy) = self.pixelated(px_rect)
            cr.translate(x, y)
            cr.scale(sx, sy)
            cr.set_source_surface(small, 0, 0)
            cr.get_source().set_filter(cairo.FILTER_NEAREST)
            cr.paint()
        cr.restore()

    def censor(self, r, kind, main_rect):
        """Burn a censor into the frame; returns its undo entry."""
        px_rect = self.to_image(r)
        x, y, w, h = px_rect
        backup = cairo.ImageSurface(cairo.FORMAT_ARGB32, max(1, w), max(1, h))
        bc = cairo.Context(backup)
        bc.set_source_surface(self.surface, -x, -y)
        bc.paint()
        self.paint_censor(cairo.Context(self.surface), px_rect, kind)
        self.surface.flush()
        self._processed.clear()
        return Censor(self, px_rect, backup, list(main_rect))

    def restore(self, px_rect, backup):
        cr = cairo.Context(self.surface)
        cr.set_source_surface(backup, px_rect[0], px_rect[1])
        cr.paint()
        self.surface.flush()
        self._processed.clear()
