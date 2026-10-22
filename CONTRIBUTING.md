# Contributing to screenie

screenie is a Rust workspace in `rust/`. How the code is organised is in
[docs/architecture.md](docs/architecture.md); read it before a larger
change.

## Prerequisites

- Rust 1.86 or later (`rust-version` in `rust/Cargo.toml`), with
  `clippy` and `rustfmt`.
- To build: `pkg-config` and the libxkbcommon headers
  (`libxkbcommon-dev` on Debian and Ubuntu, `libxkbcommon` on Arch).
- For the whole test suite: a monospace font through fontconfig (DejaVu
  Sans Mono or Noto Sans Mono), tesseract with English data, zbar's
  `zbarimg`, and tzdata. Tests that need a font, tesseract or zbar skip
  when it's missing.
- To run the picker: Hyprland on Wayland (see the [README](README.md#requirements)).

On Debian or Ubuntu, CI installs:

```
pkg-config libxkbcommon-dev tzdata fontconfig fonts-dejavu-core
tesseract-ocr tesseract-ocr-eng zbar-tools
```

`cargo test -p screenie-core` needs no system libraries at all.

## Building

From `rust/`:

```
cargo build                    # debug build of everything
cargo run -q -p screenie -- …  # run the binary
```

To install your build as `~/.cargo/bin/screenie`, from the repository
root:

```
cargo install --path rust/crates/screenie --locked
```

## The gate

Every commit passes, from `rust/`:

```
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

### CI

`.github/workflows/ci.yml` runs on every push and pull request:

- **gate**: installs the packages above, then `cargo test --workspace
  --locked` with `SCREENIE_REQUIRE_FONTS=1` (a missing font fails the
  overlay tests instead of skipping them), `cargo clippy --workspace
  --all-targets --locked -- -D warnings` and `cargo fmt --all --check`.
- **rust-version**: `cargo check --workspace --all-targets --locked` on
  Rust 1.86. Keep its toolchain in step with `rust-version` in
  `rust/Cargo.toml`.

`--locked` means `Cargo.lock` must be committed up to date with any
dependency change.

## Tests

| Where | What |
|---|---|
| `crates/screenie-core/tests/*.rs` | behaviour, one file per area (`select`, `censor`, `annotate`, `filter`, `layers`, `lenses`, `measure`, `redact`, `options`) |
| `crates/screenie-core/tests/common/mod.rs` | the shared fixture and input helpers |
| `crates/*/src/*.rs` (`#[cfg(test)]`) | unit tests beside the code: geometry, keymaps and collisions, legends, serialization, rendering, caches, parsers |
| `crates/screenie-image/tests/ui.rs` | the overlay drawn headlessly; set `SCREENIE_RENDER_OUT=DIR` to write the pictures out and look at them |
| `crates/screenie-image/tests/decode.rs` | opening PNG, JPEG, WebP and PPM/PGM |
| `crates/screenie/tests/cli.rs` | the binary on its headless paths (`shot --from`, `history`), each run in its own scratch directories |
| `crates/screenie/tests/ocr.rs` | OCR end to end with the real tesseract |

Run one file with `cargo test -p screenie-core --test censor`, or one
test by name with `cargo test NAME`.

### Testing behaviour headlessly

Picker behaviour is tested without a display. A test builds a `Session`
on the fixture (two 1920x1080 monitors side by side at scale 1, three
tiled windows, a starting box at 100,170 560x90), drives it, and asserts
on what it exposes: the box, the mode stack, the project, the status
line, the outcome.

Drive it with a `--keys` script, the same syntax as the command line
(see `--keys` in [docs/cli.md](docs/cli.md)):

```rust
mod common;
use common::*;

#[test]
fn enter_in_censor_commits_one_layer() {
    let mut s = session();
    s.keys("x t x Return");
    assert_eq!(s.layers().len(), 1);
    assert_eq!(s.mode_name(), "select");
}
```

When timing matters (held keys, repeat, keys held across a mode change),
use the raw-event helpers in `common` (`press`, `release`, `button`,
`motion`) with explicit times, then `Session::tick(now)`. Core never
reads a clock, so these tests are deterministic.

The same scripts drive the real binary: `screenie --keys '…' --no-ui`
plays them and fails (exit 2) if they don't finish the picker.

## Running screenie while developing

The picker takes over the keyboard on every monitor, and screenie writes
to your clipboard, history, recall ring and config. Keep development
runs from touching any of it.

- **Never leave an overlay waiting for input.** Give every run that
  could open the picker a way out: `--keys '…' --no-ui`, which never
  opens it, or `SCREENIE_DEADLINE=SECS`, which takes the overlays down
  after that many seconds, inside a `timeout` as a second net.
- **Keep your state out of it.** Point the XDG directories at scratch
  directories, and pass `--no-history --no-copy --no-notify` (or
  `--no-config` to ignore your config). Save only to scratch, with an
  explicit `--save DIR`.
  - `XDG_STATE_HOME` holds the recall ring (`regions.json`). Every box
    accepted from the screen joins it, and `--no-history` does not stop
    that: always use a scratch `XDG_STATE_HOME`.
  - `XDG_CONFIG_HOME` holds `screenie/config.toml`.
  - `XDG_RUNTIME_DIR` holds the history (`screenie/shots/`, `last`),
    pin records and scratch files. A scratch one is right for headless runs
    (`shot --from FILE`, `history`). Runs that capture the screen need
    the real one, because the Wayland and Hyprland sockets live there:
    rely on `--no-history` instead.
  - `XDG_PICTURES_DIR/Screenshots` is where a bare `--save` writes.
- **Hyprland.** A shell outside the session may lack
  `HYPRLAND_INSTANCE_SIGNATURE`; take it from `hyprctl instances -j`.

From `rust/`, a headless run that touches nothing of yours:

```
S=$(mktemp -d)
mkdir -p "$S"/{run,state,config,out}
XDG_RUNTIME_DIR="$S/run" XDG_STATE_HOME="$S/state" XDG_CONFIG_HOME="$S/config" \
  cargo run -q -p screenie -- shot '0,0 100x50' --from picture.png \
  --no-copy --no-notify --no-history --save "$S/out/" --json
```

A scripted run of the picker on the live screen:

```
XDG_STATE_HOME="$S/state" XDG_CONFIG_HOME="$S/config" \
  timeout 10 cargo run -q -p screenie -- --keys 'x Return Return' --no-ui \
  --no-copy --no-notify --no-history --save "$S/out/" --json
```

An interactive check, bounded:

```
XDG_STATE_HOME="$S/state" XDG_CONFIG_HOME="$S/config" SCREENIE_DEADLINE=5 \
  timeout 10 cargo run -q -p screenie -- --no-copy --no-notify --no-history
```

`screenie help keys`, `help regions`, `help config` and `--help` print
the reference and are always safe to run.

## Which document wins

The documentation is the specification. When documents disagree, the
higher one wins:

1. [docs/picker.md](docs/picker.md): the picker's behaviour.
2. [docs/cli.md](docs/cli.md): the command line.
3. The feature documents: [docs/auto-redact.md](docs/auto-redact.md),
   [docs/measure.md](docs/measure.md).
4. [docs/architecture.md](docs/architecture.md): how the code is built.

When the code and the spec disagree, the spec is right, and the code (or,
deliberately, the spec) changes.

## The development rule

- **A behaviour change updates three things together:** the spec
  (picker.md, cli.md or a feature document), the on-screen legend and
  palette data (`screenie-core/src/legend.rs`, and the keymaps in
  `keymap.rs` they are spelled from), and the tests. The interaction
  grammar is a product constraint, not an implementation detail.
- **New keys** must pass the keymap collision tests; never hand-write a
  legend string the keymap could spell.
- **Persistent formats** (the project file, the recall ring) change only
  with version handling and round-trip tests: an optional field with a
  serde default, or a version bump with a migration over the old data,
  tested on a file of the old version; and tests that a project
  serializes, reads back equal and renders the same pixels. Existing files
  must keep opening.
- **Persistent data or crate boundaries** changing updates
  docs/architecture.md too.
- Behaviour is tested headlessly in `screenie-core` wherever it can be.

## Commits

- [Conventional commits](https://www.conventionalcommits.org/):
  `feat:`, `fix:`, `docs:`, `test:`, `chore:`, `refactor:`, with an
  optional scope (`fix(core): …`, `docs(cli): …`).
- One logical change per commit, with the gate passing at each.
- Describe what changed and why, in the present tense.

## Reporting bugs and proposing changes

- Open an issue before a behaviour change, so the spec change can be
  agreed first.
- A bug report gives `screenie --version`, `hyprctl version` and, where
  it can, a `--keys` script that reproduces the problem.
- A pull request passes the gate and updates the spec, the legend and the
  tests together.

## License

Contributions are licensed as in the [README](README.md#license).
