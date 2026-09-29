"""Where results go: clipboard, files, notifications, OCR, QR, pins."""

import os
import struct
import subprocess
import tempfile
import time
import zlib

import cairo

# Screen text is small for tesseract; 2x upscaling measurably fixes misreads
# at 1080p (and costs ~0.3s). HiDPI captures are already big enough.
OCR_UPSCALE = 2


def runtime_dir():
    """$XDG_RUNTIME_DIR/screenie: tmpfs, private, cleared at logout."""
    d = os.path.join(os.environ.get("XDG_RUNTIME_DIR", tempfile.gettempdir()), "screenie")
    os.makedirs(d, mode=0o700, exist_ok=True)
    return d


def notify(title, body=""):
    subprocess.run(["notify-send", "-a", "screenie", title, body])


def copy_text(text):
    subprocess.run(["wl-copy", "--type", "text/plain"], input=text.encode())


def encode_png(surface, level=1):
    """A cairo surface as PNG bytes (RGB; screenshots are opaque). About
    twice as fast as cairo's own writer: a slice swizzle of each row and
    zlib at a low level, for a file only a few percent bigger."""
    surface.flush()
    w, h, stride = surface.get_width(), surface.get_height(), surface.get_stride()
    data = bytes(surface.get_data())
    raw = bytearray()
    row = bytearray(w * 3)
    for y in range(h):
        line = data[y * stride:y * stride + w * 4]  # cairo ARGB32: BGRA bytes
        row[0::3], row[1::3], row[2::3] = line[2::4], line[1::4], line[0::4]
        raw += b"\x00"  # filter: none
        raw += row

    def chunk(kind, body):
        return (struct.pack(">I", len(body)) + kind + body
                + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), level))
            + chunk(b"IEND", b""))


def unique_path(d, stamp):
    """d/stamp.png, or stamp-1.png, stamp-2.png... if that's taken (two shots
    in the same second mustn't overwrite each other)."""
    path, n = os.path.join(d, f"{stamp}.png"), 1
    while os.path.exists(path):
        path, n = os.path.join(d, f"{stamp}-{n}.png"), n + 1
    return path


def save_image(surface, save_dir=None):
    """Copy it to the clipboard, save it to save_dir (or a runtime-dir copy,
    so every shot can still be annotated), and record it as the last shot:
    $XDG_RUNTIME_DIR/screenie/last holds its path."""
    png = encode_png(surface)
    subprocess.run(["wl-copy", "--type", "image/png"], input=png)
    d = os.path.expanduser(save_dir) if save_dir else runtime_dir()
    os.makedirs(d, exist_ok=True)
    path = unique_path(d, time.strftime("%Y-%m-%d_%H-%M-%S"))
    with open(path, "wb") as f:
        f.write(png)
    with open(os.path.join(runtime_dir(), "last"), "w") as f:
        f.write(path + "\n")
    return path


def ocr(crop, k, lang):
    """OCR a crop and copy the text. Returns True if text was found."""
    scale = OCR_UPSCALE if k < 1.5 else 1
    big = cairo.ImageSurface(cairo.FORMAT_RGB24, crop.get_width() * scale,
                             crop.get_height() * scale)
    cr = cairo.Context(big)
    cr.scale(scale, scale)
    cr.set_source_surface(crop, 0, 0)
    cr.get_source().set_filter(cairo.FILTER_GOOD)
    cr.paint()
    try:
        r = subprocess.run(["tesseract", "stdin", "stdout", "-l", lang],
                           input=encode_png(big), capture_output=True)
    except FileNotFoundError:
        notify("OCR failed", "tesseract is not installed")
        return False
    if r.returncode:
        notify("OCR failed", r.stderr.decode(errors="replace").strip()[:200])
        return False
    text = r.stdout.decode(errors="replace").strip()
    if not text:
        notify("No text found")
        return False
    copy_text(text)
    notify("Text copied", text[:200])
    return True


def decode_code(crop):
    """QR / barcode contents via zbar, or None."""
    fd, tmp = tempfile.mkstemp(prefix="screenie-qr-", suffix=".png")
    os.close(fd)
    try:
        crop.write_to_png(tmp)
        r = subprocess.run(["zbarimg", "-q", "--raw", tmp], capture_output=True)
    except FileNotFoundError:
        notify("QR failed", "zbarimg (zbar) is not installed")
        return None
    finally:
        os.unlink(tmp)
    return r.stdout.decode(errors="replace").strip() or None


def pin(crop, rect):
    """Float the crop over the spot it came from, on every workspace
    (swayimg with app id screenie-pin; the window rule makes it pinned)."""
    fd, path = tempfile.mkstemp(prefix="pin-", suffix=".png", dir=runtime_dir())
    os.close(fd)
    crop.write_to_png(path)
    x, y, w, h = (round(v) for v in rect)
    subprocess.Popen(["swayimg", "-a", "screenie-pin", "-P", f"{x},{y}",
                      "-S", f"{w},{h}", path],
                     start_new_session=True, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL)
