# screenie

Keyboard-driven screenshots for Hyprland.

screenie freezes the screen, then lets you select, inspect, edit and
extract from that frozen frame with keys modelled on vim's visual-block
mode. The keyboard comes first, but the mouse can do the whole job too.
Learn how **Select** works and the rest of the picker follows the same
grammar.

## Features

- **Select** a region by its corners, jump to window and monitor edges or
  to edges in the picture, widen from window to monitor to everything, or
  just click a window or drag a region.
- **Inspect** pixels: a pixel cursor, a loupe with a grid and the pixel's
  colour, and a zoomed lens with view filters that never reach the output.
- **Censor** with pixelation or solid bars. **Auto-redact** reads the box
  with OCR and proposes censors over emails, tokens, card numbers, IBANs,
  QR codes and more, for you to review.
- **Annotate** with a pen (freehand, straight or arced), a highlighter,
  rectangles, circles, lines, arrows, text and numbered callouts.
- **Filter** regions: greyscale, invert, brightness, contrast, blur.
- **Layers**: every edit is an operation in an ordered layer over the
  untouched capture, so layers can be reordered, hidden or deleted, and
  undo is exact.
- **Measure** lengths, arcs, paths, circles, rectangles, areas and angles,
  in pixels or, once calibrated (it can read a scale bar), in real units.
- **Finish** with the image, its text (OCR), a colour, a decoded QR code
  or barcode, or a pin: a floating copy on every workspace.
- **Reopen** recent shots with their layers still editable.
- **Script** it: a no-UI `shot`, slurp-style `--select-only`, `--json`
  output, played keys (`--keys`), config profiles.

Recording, scrolling captures, uploading, beautifying and live (unfrozen)
picking are out of scope: see the non-goals in
[docs/cli.md](docs/cli.md#non-goals).

## Requirements

- **Hyprland**, on Wayland. screenie reads windows and monitors from
  Hyprland's IPC, captures with wlr-screencopy (and
  ext-image-copy-capture for a window's own pixels), and draws its
  overlays with layer-shell. Other compositors and X11 aren't supported.
- **A monospace font**, found at the usual paths or through fontconfig's
  `fc-match`. Without one the picker exits with "no monospace font found".
- Optional, for the features that use them:
  - **tesseract** and its language data (`eng` by default): OCR,
    auto-redact, reading scale bars;
  - **zbarimg** from zbar: QR codes and barcodes;
  - **notify-send**: desktop notifications.
- Pins need a Hyprland window rule (below).

## Install

Build from source. You need Rust 1.86 or later, pkg-config and the
libxkbcommon headers:

```sh
# Debian, Ubuntu
sudo apt install pkg-config libxkbcommon-dev
sudo apt install tesseract-ocr tesseract-ocr-eng zbar-tools libnotify-bin   # optional

# Arch
sudo pacman -S pkgconf libxkbcommon
sudo pacman -S tesseract tesseract-data-eng zbar libnotify                  # optional
```

Then, from a clone of this repository:

```sh
cargo install --path rust/crates/screenie --locked
```

This installs `screenie` to `~/.cargo/bin`. If that isn't on Hyprland's
PATH (when it's started from a display manager, say), use the full path
in binds: `exec, ~/.cargo/bin/screenie`. Shell completions come from
`screenie completions bash|zsh|fish`.

## Hyprland setup

In `hyprland.conf` (the classic rule syntax; newer Hyprland releases spell
rules differently):

```ini
bind = , Print, exec, screenie                                # the picker
bind = SHIFT, Print, exec, screenie shot                      # the focused window, no UI
bind = SUPER, Print, exec, screenie --save                    # the picker, also saved to a file
bind = CTRL, Print, exec, screenie edit last --mode annotate  # reopen the last shot to mark it up

# Pins: float them, on every workspace.
windowrule = float, class:^(screenie-pin)$
windowrule = pin, class:^(screenie-pin)$

# Optional: show the overlay at once, without an animation.
layerrule = noanim, screenie
```

With a Lua config (newer Hyprland):

```lua
hl.bind("Print",         hl.dsp.exec_cmd("screenie"))
hl.bind("SHIFT + Print", hl.dsp.exec_cmd("screenie shot"))
hl.bind("SUPER + Print", hl.dsp.exec_cmd("screenie --save"))
hl.bind("CTRL + Print",  hl.dsp.exec_cmd("screenie edit last --mode annotate"))

hl.window_rule({ name = "screenie-pin", match = { class = "^screenie-pin$" }, float = true, pin = true })
hl.layer_rule({ name = "screenie", match = { namespace = "^screenie$" }, no_anim = true })
```

Whatever the spelling, pins are windows with the class (app id)
`screenie-pin`, to float and pin, and the overlay is the layer namespace
`screenie`.

## A quick tour

Press Print: every monitor shows the frozen frame, with a box on the
active window. Then:

| Keys                           | Do                                                     |
| ------------------------------ | ------------------------------------------------------ |
| `h j k l` / arrows             | Move the box's cursor corner (`Shift`: the whole box)  |
| `Ctrl`+`hjkl`                  | Select the neighbouring window that way                |
| `w b } {`                      | Move the cursor edge to the next window or monitor edge |
| `o` / `Space`                  | Switch to the opposite corner                          |
| `+` / `-`, wheel               | The current amount: here, scope (window → monitor → everything) |
| click / drag                   | Snap to a window / draw a region                       |
| `.`                            | Recall an earlier box                                  |
| `i` `z` `e`                    | Insert (a pixel cursor), loupe, enhance                |
| `x` `a` `f`                    | Censor, annotate, filter (in a new layer)              |
| `m`                            | Measure                                                |
| `1`–`9`                        | Manage committed layers                                |
| `Enter`, right click           | Done: the image, to the clipboard                      |
| `y` `p` `Q` `P`                | Finish with text (OCR), a colour, a code, a pin        |
| `u`                            | Undo                                                   |
| `Esc`, middle click            | Back (in Select: cancel)                               |
| `?`                            | This mode's keys                                       |

The same keys mean the same things in every mode: `hjkl` steers, `+`/`-`
adjusts, `Enter` confirms, `Esc` goes back. The full specification is
[docs/picker.md](docs/picker.md).

## Useful commands

```sh
screenie                                     # the picker; Enter copies the image
screenie shot --save                         # the focused window, saved and copied
screenie shot monitor --stdout > shot.png    # the focused monitor, to stdout
screenie edit last                           # reopen the last shot, layers intact
screenie edit --result text                  # open the most recent image; Enter OCRs it
screenie --result colour --stdout            # a colour picker for scripts
screenie --select-only                       # pick a region and print it, like slurp
screenie --redact suggest                    # open in Censor, with secrets proposed
screenie --result measure --json             # measure, as JSON
screenie history                             # recent shots
screenie help keys                           # every mode's keys
```

Results go to the clipboard by default. Everything else is in
[docs/cli.md](docs/cli.md).

## Documentation

- [docs/picker.md](docs/picker.md): the picker, mode by mode, with every
  key. The specification of its behaviour.
- [docs/cli.md](docs/cli.md): the command line, config, profiles, files,
  JSON and exit status.
- [docs/auto-redact.md](docs/auto-redact.md): finding and censoring
  secrets.
- [docs/measure.md](docs/measure.md): measuring, and calibrating to real
  units.
- [docs/architecture.md](docs/architecture.md): how the code is built.
- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing and changing
  screenie.

## Development

The code is a Cargo workspace in `rust/`. How to build, test and propose
changes is in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

screenie is licensed under either of the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in screenie by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
