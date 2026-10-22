//! The command line (docs/cli.md). An option it doesn't know, or one the
//! verb doesn't use, is a usage error rather than silently ignored.

use std::ffi::OsString;
use std::path::PathBuf;

use lexopt::{Arg, Parser, ValueExt};

/// `--png-level`'s default.
pub const PNG_LEVEL: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// `--save [DIR|TEMPLATE]`: `Some(None)` for the default directory.
    pub save: Option<Option<PathBuf>>,
    pub stdout: bool,
    /// `--copy` / `--no-copy`; `None` is the default (on unless stdout
    /// carries the data).
    pub copy: Option<bool>,
    pub png_level: u8,
    pub quiet: bool,
    /// `--notify` / `--no-notify`; `None` is on when stdout isn't a terminal.
    pub notify: Option<bool>,
    pub no_history: bool,
    /// `--ocr-lang` (or `[ocr] lang`); `None` is `eng`.
    pub ocr_lang: Option<String>,
    /// `--json`: one line describing what happened.
    pub json: bool,
    /// `--save-project`: the project beside the `--save` image too.
    pub save_project: bool,
    /// `--redact suggest|auto`.
    pub redact: Option<Redact>,
    /// `--delay SECONDS`: wait before capturing the screen.
    pub delay: Option<f64>,
    /// `--cursor`: the pointer in the capture.
    pub cursor: bool,
    /// `--format`: how images are written out.
    pub format: Format,
    /// `--scale`: the output's resolution.
    pub scale: OutputScale,
    /// `--pin`: float the result too.
    pub pin: bool,
    /// `--colour-format`: how a colour result is written.
    pub colour_format: ColourFormat,
    /// `--no-copy-uri`: copy a saved image without its file's URI.
    pub no_copy_uri: bool,
    /// `--quality N`: JPEG's, 1-100 (unused by other formats, so a
    /// config's quality and a profile's format can sit together).
    pub quality: Option<u8>,
}

/// `--scale native|min|FACTOR`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OutputScale {
    /// The highest scale of the monitors the box is on.
    #[default]
    Native,
    /// The lowest: no monitor upsampled.
    Min,
    /// Pixels a logical pixel.
    Factor(f64),
}

/// How a colour result is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColourFormat {
    /// `#rrggbb`
    #[default]
    Hex,
    /// `rgb(r, g, b)`
    Rgb,
    /// `hsl(h, s%, l%)`
    Hsl,
}

/// An image's encoding for `--save`, `--stdout` and the clipboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    /// Lossless.
    Webp,
    Ppm,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Ppm => "ppm",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
            Format::Ppm => "image/x-portable-pixmap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redact {
    /// Open Censor with the suggestions to review.
    Suggest,
    /// Apply the ticked suggestions without asking.
    Auto,
}

impl Output {
    pub fn copies(&self) -> bool {
        self.copy.unwrap_or(!self.stdout)
    }

    pub fn ocr_lang(&self) -> String {
        self.ocr_lang.clone().unwrap_or_else(|| "eng".into())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pick {
    pub region: Option<String>,
    pub keys: Option<String>,
    /// `--keys-file FILE|-`: keys to read, into `keys`, before the picker.
    pub keys_file: Option<String>,
    pub no_ui: bool,
    /// `--measure-scale '84.2px=1m'`: Measure starts calibrated.
    pub measure_scale: Option<screenie_core::units::Scale>,
    /// `--select-only`: print the box's geometry, deliver nothing else.
    pub select_only: bool,
    pub region_format: RegionFormat,
    /// `--result`: what Enter produces.
    pub result: Option<screenie_core::select::EnterResult>,
    /// `--mode`: the mode the picker opens in.
    pub mode: Option<screenie_core::select::StartMode>,
    pub lock_box: bool,
    /// `--from SOURCE`: the picture (None is the screen).
    pub from: Option<Source>,
    pub output: Output,
    /// `--stdout` was given, not implied by `--select-only`.
    pub stdout_typed: bool,
}

/// Where the picture comes from (docs/cli.md, SOURCES AND REGIONS).
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// The frozen screen, the default.
    Screen,
    /// An image file (PNG, JPEG, WebP, PPM), or a screenie project PNG.
    File(PathBuf),
    /// An image on stdin.
    Stdin,
    /// A past shot, with its project: `last` (1) or `last:N`.
    Last(usize),
    /// The image on the clipboard (a screenie shot reopens with its
    /// project).
    Clipboard,
    /// The most recent image: the clipboard's, else the last shot.
    Recent,
}

impl Source {
    pub fn parse(s: &str) -> Result<Source, String> {
        Ok(match s {
            "screen" => Source::Screen,
            "-" => Source::Stdin,
            "clipboard" => Source::Clipboard,
            "recent" => Source::Recent,
            _ if s == "last" || s.starts_with("last:") => Source::Last(parse_last(s)?),
            _ => Source::File(PathBuf::from(s)),
        })
    }

    /// An image rather than the screen or a shot: only `all` and `X,Y WxH`
    /// (in its pixels) mean anything there.
    pub fn is_image(&self) -> bool {
        matches!(self, Source::File(_) | Source::Stdin | Source::Clipboard)
    }
}

/// A region that doesn't mean anything over an image (a window, a
/// monitor, the ring), as a usage error.
fn check_image_region(from: &Source, region: Option<&str>) -> Result<(), String> {
    let Some(r) = region.map(str::trim) else {
        return Ok(());
    };
    if r == "-" && *from == Source::Stdin {
        return Err(
            "--from - and a region from - both read stdin: give one in the arguments".into(),
        );
    }
    let screen_only = ["monitor", "window:", "active", "last-region"]
        .iter()
        .any(|p| r.starts_with(p));
    if screen_only {
        return Err(format!(
            "{r:?} means a part of the screen: over an image, the region is all or X,Y WxH (in its pixels)"
        ));
    }
    Ok(())
}

/// How `--select-only` prints the box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegionFormat {
    /// `X,Y WxH`, rounded outward.
    #[default]
    Slurp,
    /// `{"x":…,"y":…,"w":…,"h":…}`, exact.
    Json,
    /// `X Y W H`, rounded outward.
    Hypr,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shot {
    pub region: Option<String>,
    /// `--from SOURCE`: the picture (None is the screen).
    pub from: Option<Source>,
    pub output: Output,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Pick(Pick),
    Shot(Shot),
    /// `edit [FILE|-|clipboard|recent|last[:N]]`: the picker over an image, or a past shot's
    /// project (`pick --from SOURCE --region all`).
    Edit {
        source: Source,
        pick: Pick,
    },
    /// `history [-n N] [--json] [--quiet]`.
    History {
        n: usize,
        json: bool,
        quiet: bool,
    },
    Help,
    /// `help keys|regions|config`.
    HelpTopic(String),
    /// `completions bash|zsh|fish`.
    Completions(String),
    Version,
    /// Internal: serve the clipboard (see platform::clipboard).
    ServeClipboard,
    /// Internal: a pin's window (see platform::pins).
    ServePin(Vec<String>),
    /// `pin [FILE|-|clipboard|recent|last[:N]] [--at X,Y] [--quiet]`.
    Pin {
        source: Source,
        at: Option<(f64, f64)>,
        json: bool,
        quiet: bool,
    },
    /// `pin list [--json] [--quiet]`.
    PinList {
        json: bool,
        quiet: bool,
    },
    /// `pin close [ID...|--all] [--quiet]`.
    PinClose {
        ids: Vec<u32>,
        json: bool,
        quiet: bool,
    },
}

pub const USAGE: &str = "\
usage: screenie [pick] [--from SOURCE] [--region REGION] [PICK OPTIONS] [OUTPUT OPTIONS]
       screenie shot [REGION] [--from SOURCE] [CAPTURE OPTIONS] [OUTPUT OPTIONS]
       screenie edit [FILE|-|clipboard|recent|last[:N]] [PICK OPTIONS] [OUTPUT OPTIONS]
       screenie history [-n N] [--json]
       screenie pin [FILE|-|clipboard|recent|last[:N]] [--at X,Y]
       screenie pin list [--json]  |  pin close ID...|--all
       screenie help [keys|regions|config]
       screenie completions bash|zsh|fish
       screenie --version

pick options:   --keys KEYS | --keys-file FILE|-  --no-ui  --mode MODE  --result RESULT
                --lock-box | --no-lock-box  --select-only
                --region-format slurp|json|hypr  --measure-scale PX=LENGTH
MODE: select | insert | censor | annotate | filter | measure
RESULT: image | text | colour | code | pin | measure

output options: --save [DIR|TEMPLATE] | --no-save  --stdout  --copy | --no-copy
                --notify | --no-notify  --quiet | --no-quiet  --no-history
                --json | --no-json  --save-project | --no-save-project
                --pin | --no-pin  --redact suggest|auto  --ocr-lang LANG
                --format png|jpeg|webp|ppm  --quality 1-100 (JPEG)  --png-level 0-9
                --colour-format hex|rgb|hsl  --copy-uri | --no-copy-uri
capture options: --delay SECONDS  --cursor | --no-cursor  --scale native|min|FACTOR
global options: -p, --profile NAME (a config profile)  --config PATH  --no-config

SOURCE: screen | FILE | - | clipboard | recent | last[:N]
REGION: active | window:class:RE|title:RE|address:0x..|pid:N | monitor[:NAME]
        | all | X,Y WxH | last-region[:N] | -
--redact auto is best effort: OCR misses small, stylised, low-contrast and
rotated text, and text in images. --json lists what it redacted.
See screenie help keys|regions|config for the reference.";

/// `last` (1) or `last:N`.
fn parse_last(s: &str) -> Result<usize, String> {
    match s.strip_prefix("last") {
        Some("") => Ok(1),
        Some(n) => n
            .strip_prefix(':')
            .and_then(|n| n.parse().ok())
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("bad shot {s:?}: last or last:N")),
        None => Err(format!("bad shot {s:?}: last or last:N")),
    }
}

/// Whether a bare `--save`'s next argument is its directory or template:
/// not an option, and with a `/`, a leading `~` or `.`, or an image
/// extension.
fn looks_like_a_path(s: &str) -> bool {
    !s.starts_with('-')
        && (s.contains('/')
            || s.starts_with('~')
            || s.starts_with('.')
            || crate::files::has_image_extension(s))
}

/// Parse the arguments (without the program name).
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let args: Vec<OsString> = args.into_iter().collect();
    if args
        .first()
        .is_some_and(|a| a == screenie_platform::clipboard::SERVE_ARG)
    {
        return Ok(Command::ServeClipboard);
    }
    if args
        .first()
        .is_some_and(|a| a == screenie_platform::pins::SERVE_ARG)
    {
        let rest = args[1..].iter().map(|a| a.to_string_lossy().to_string());
        return Ok(Command::ServePin(rest.collect()));
    }
    let mut p = Parser::from_args(args);
    let mut verb: Option<String> = None;
    let unset = Pick {
        output: Output {
            png_level: PNG_LEVEL,
            ..Output::default()
        },
        ..Pick::default()
    };
    let mut pick = unset.clone();
    let mut positional: Vec<String> = Vec::new();
    let mut history_n: usize = 20;
    let mut pin_at: Option<(f64, f64)> = None;
    let mut pin_all = false;
    let mut region_format = false;
    let err = |e: lexopt::Error| e.to_string();
    while let Some(arg) = p.next().map_err(err)? {
        let o = &mut pick.output;
        match arg {
            Arg::Long("help") | Arg::Short('h') => return Ok(Command::Help),
            Arg::Long("version") => return Ok(Command::Version),
            Arg::Long("region") => {
                pick.region = Some(p.value().map_err(err)?.string().map_err(err)?)
            }
            Arg::Long("keys") => pick.keys = Some(p.value().map_err(err)?.string().map_err(err)?),
            Arg::Long("keys-file") => {
                pick.keys_file = Some(p.value().map_err(err)?.string().map_err(err)?)
            }
            Arg::Long("no-ui") => pick.no_ui = true,
            Arg::Long("from") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                pick.from = match Source::parse(&v)? {
                    Source::Screen => None,
                    other => Some(other),
                };
            }
            Arg::Long("select-only") => pick.select_only = true,
            Arg::Long("lock-box") => pick.lock_box = true,
            Arg::Long("no-lock-box") => pick.lock_box = false,
            Arg::Long("region-format") => {
                pick.region_format = match p.value().map_err(err)?.string().map_err(err)?.as_str() {
                    "slurp" => RegionFormat::Slurp,
                    "json" => RegionFormat::Json,
                    "hypr" => RegionFormat::Hypr,
                    other => return Err(format!("--region-format {other:?}: slurp, json or hypr")),
                };
                region_format = true;
            }
            Arg::Long("result") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                pick.result = Some(screenie_core::select::EnterResult::parse(&v).ok_or_else(
                    || format!("--result {v:?}: image, text, colour, code, pin or measure"),
                )?);
            }
            Arg::Long("mode") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                pick.mode = Some(screenie_core::select::StartMode::parse(&v).ok_or_else(|| {
                    format!("--mode {v:?}: select, insert, censor, annotate, filter or measure")
                })?);
            }
            Arg::Long("measure-scale") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                pick.measure_scale = Some(screenie_core::units::Scale::parse(&v)?);
            }
            Arg::Long("save") => {
                // `--save DIR` takes the next argument when it is a path,
                // so `shot --save monitor` still means a region.
                let dir = match p.optional_value() {
                    Some(v) => Some(PathBuf::from(v)),
                    None => {
                        let next = p.try_raw_args().and_then(|raw| {
                            let s = raw.peek()?.to_str()?;
                            looks_like_a_path(s).then(|| s.to_owned())
                        });
                        if next.is_some() {
                            p.value().map_err(err)?; // consume it
                        }
                        next.map(PathBuf::from)
                    }
                };
                o.save = Some(dir);
            }
            Arg::Long("no-save") => o.save = None,
            Arg::Long("stdout") => o.stdout = true,
            Arg::Long("json") => o.json = true,
            Arg::Long("no-json") => o.json = false,
            Arg::Long("save-project") => o.save_project = true,
            Arg::Long("no-save-project") => o.save_project = false,
            Arg::Long("cursor") => o.cursor = true,
            Arg::Long("no-cursor") => o.cursor = false,
            Arg::Long("pin") => o.pin = true,
            Arg::Long("no-pin") => o.pin = false,
            Arg::Long("scale") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                o.scale = match v.as_str() {
                    "native" => OutputScale::Native,
                    "min" => OutputScale::Min,
                    _ => OutputScale::Factor(
                        v.parse()
                            .ok()
                            .filter(|f: &f64| (0.05..=8.0).contains(f))
                            .ok_or_else(|| {
                                format!("--scale {v:?}: native, min, or a factor 0.05 to 8")
                            })?,
                    ),
                };
            }
            Arg::Long("copy-uri") => o.no_copy_uri = false,
            Arg::Long("no-copy-uri") => o.no_copy_uri = true,
            Arg::Long("colour-format") | Arg::Long("color-format") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                o.colour_format = match v.as_str() {
                    "hex" => ColourFormat::Hex,
                    "rgb" => ColourFormat::Rgb,
                    "hsl" => ColourFormat::Hsl,
                    _ => return Err(format!("--colour-format {v:?}: hex, rgb or hsl")),
                };
            }
            Arg::Long("format") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                o.format = match v.as_str() {
                    "png" => Format::Png,
                    "jpeg" | "jpg" => Format::Jpeg,
                    "webp" => Format::Webp,
                    "ppm" => Format::Ppm,
                    _ => return Err(format!("--format {v:?}: png, jpeg, webp or ppm")),
                };
            }
            Arg::Long("quality") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                o.quality = Some(
                    v.parse()
                        .ok()
                        .filter(|q| (1..=100).contains(q))
                        .ok_or_else(|| format!("--quality {v:?}: 1 to 100"))?,
                );
            }
            Arg::Long("delay") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                let secs: f64 = v
                    .parse()
                    .ok()
                    .filter(|s: &f64| (0.0..=3600.0).contains(s))
                    .ok_or_else(|| format!("--delay {v:?}: seconds, 0 to 3600"))?;
                o.delay = Some(secs);
            }
            Arg::Long("redact") => {
                o.redact = Some(
                    match p.value().map_err(err)?.string().map_err(err)?.as_str() {
                        "suggest" => Redact::Suggest,
                        "auto" => Redact::Auto,
                        other => return Err(format!("--redact {other:?}: suggest or auto")),
                    },
                )
            }
            Arg::Long("at") if verb.as_deref() == Some("pin") => {
                let v = p.value().map_err(err)?.string().map_err(err)?;
                let xy = v
                    .split_once(',')
                    .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
                    .ok_or_else(|| format!("--at {v:?}: X,Y"))?;
                pin_at = Some(xy);
            }
            Arg::Long("all") if verb.as_deref() == Some("pin") => pin_all = true,
            Arg::Short('n') if verb.as_deref() == Some("history") => {
                history_n = p.value().map_err(err)?.parse().map_err(err)?;
            }
            Arg::Long("copy") => o.copy = Some(true),
            Arg::Long("no-copy") => o.copy = Some(false),
            Arg::Long("notify") => o.notify = Some(true),
            Arg::Long("no-notify") => o.notify = Some(false),
            Arg::Long("quiet") => o.quiet = true,
            Arg::Long("no-quiet") => o.quiet = false,
            Arg::Long("no-history") => o.no_history = true,
            Arg::Long("ocr-lang") => {
                o.ocr_lang = Some(p.value().map_err(err)?.string().map_err(err)?)
            }
            Arg::Long("png-level") => {
                let n: u8 = p.value().map_err(err)?.parse().map_err(err)?;
                if n > 9 {
                    return Err("--png-level is 0-9".into());
                }
                o.png_level = n;
            }
            Arg::Value(v) => {
                let v = v.string().map_err(err)?;
                if verb.is_none()
                    && positional.is_empty()
                    && crate::help::VERBS.contains(&v.as_str())
                {
                    verb = Some(v);
                } else {
                    positional.push(v);
                }
            }
            other => return Err(other.unexpected().to_string()),
        }
    }
    if let Some(v @ ("help" | "completions" | "history" | "pin")) = verb.as_deref() {
        // These take none of pick's, capture's or output's options, but
        // the global ones.
        let mut used = pick.clone();
        (used.output.json, used.output.quiet, used.output.no_history) = (false, false, false);
        if v == "pin" && used.from.is_some() {
            return Err("pin takes the image as FILE, not --from".into());
        }
        if used != unset {
            return Err(format!("{v} takes no pick, capture or output options"));
        }
    }
    if pin_all && positional.first().map(String::as_str) != Some("close") {
        return Err("--all is for pin close".into());
    }
    if pin_at.is_some()
        && matches!(
            positional.first().map(String::as_str),
            Some("list" | "close")
        )
    {
        return Err("--at is for pinning an image".into());
    }
    if pick.keys.is_some() && pick.keys_file.is_some() {
        return Err("--keys or --keys-file, not both".into());
    }
    if pick.keys_file.as_deref() == Some("-") {
        let image = pick.from == Some(Source::Stdin)
            || verb.as_deref() == Some("edit") && positional.first().is_some_and(|f| f == "-");
        if image || pick.region.as_deref().map(str::trim) == Some("-") {
            return Err(
                "--keys-file - and another - both read stdin: give one in the arguments".into(),
            );
        }
    }
    if pick.output.stdout && pick.output.json {
        return Err("--stdout and --json both claim stdout: pick one".into());
    }
    if region_format && !pick.select_only {
        return Err("--region-format is for --select-only".into());
    }
    if pick.output.redact.is_some() && pick.mode.is_some() {
        return Err("--redact opens Censor itself: drop --mode".into());
    }
    if pick.select_only && pick.result.is_some() {
        return Err("--select-only already says what Enter gives".into());
    }
    pick.stdout_typed = pick.output.stdout;
    if pick.select_only {
        // The geometry is the output: stdout's (unless --json describes
        // it), and never the clipboard's unless --copy says so.
        pick.output.copy.get_or_insert(false);
        if !pick.output.json {
            pick.output.stdout = true;
        }
    }
    if pick.output.redact == Some(Redact::Suggest)
        && (pick.no_ui || verb.as_deref() == Some("shot"))
    {
        return Err("--redact suggest needs the picker: use --redact auto".into());
    }
    // Waiting and the pointer are about capturing the screen.
    let capturing = pick.output.delay.is_some() || pick.output.cursor;
    let from_image =
        pick.from.as_ref().is_some_and(|f| *f != Source::Screen) || verb.as_deref() == Some("edit");
    if capturing && from_image {
        return Err("--delay and --cursor are for capturing the screen, not an image".into());
    }
    match verb.as_deref() {
        Some("help") => match positional.as_slice() {
            [] => Ok(Command::Help),
            [t] => Ok(Command::HelpTopic(t.clone())),
            _ => Err("help takes one topic: keys, regions or config".into()),
        },
        Some("pin") => match positional.first().map(String::as_str) {
            Some("list") if positional.len() == 1 => Ok(Command::PinList {
                json: pick.output.json,
                quiet: pick.output.quiet,
            }),
            Some("close") => {
                let ids = positional[1..]
                    .iter()
                    .map(|i| i.parse().map_err(|_| format!("pin close {i:?}: an id")))
                    .collect::<Result<Vec<u32>, String>>()?;
                match (ids.is_empty(), pin_all) {
                    (true, false) => Err("pin close which? IDs, or --all".into()),
                    (false, true) => Err("pin close: IDs or --all, not both".into()),
                    _ => Ok(Command::PinClose {
                        ids,
                        json: pick.output.json,
                        quiet: pick.output.quiet,
                    }),
                }
            }
            _ if positional.len() > 1 => Err("pin takes one image".into()),
            first => {
                let source = match first {
                    None => Source::Last(1),
                    Some(s) => Source::parse(s)?,
                };
                if source == Source::Screen {
                    return Err("pin what? (a file, -, clipboard, recent, last or last:N)".into());
                }
                Ok(Command::Pin {
                    source,
                    at: pin_at,
                    json: pick.output.json,
                    quiet: pick.output.quiet,
                })
            }
        },
        Some("completions") => match positional.as_slice() {
            [sh] => Ok(Command::Completions(sh.clone())),
            _ => Err("completions bash, zsh or fish?".into()),
        },
        Some("history") => {
            if let Some(extra) = positional.first() {
                return Err(format!("unexpected argument {extra:?}"));
            }
            Ok(Command::History {
                n: history_n,
                json: pick.output.json,
                quiet: pick.output.quiet,
            })
        }
        Some("edit") => {
            if pick.from.is_some() {
                return Err("edit takes what it opens as FILE, not --from".into());
            }
            let source = match positional.as_slice() {
                [w] => Source::parse(w)?,
                [] => Source::Recent,
                _ => return Err("edit takes one image".into()),
            };
            if source == Source::Screen {
                return Err("edit what? (a file, -, clipboard, recent, last or last:N)".into());
            }
            if source.is_image() {
                check_image_region(&source, pick.region.as_deref())?;
            }
            Ok(Command::Edit { source, pick })
        }
        Some("shot") => {
            let picker_only = pick.keys.is_some()
                || pick.keys_file.is_some()
                || pick.no_ui
                || pick.measure_scale.is_some()
                || pick.select_only
                || pick.result.is_some()
                || pick.mode.is_some()
                || pick.lock_box;
            if picker_only {
                return Err("shot has no picker: that option is for pick".into());
            }
            if pick.region.is_some() && !positional.is_empty() {
                return Err("give the region once".into());
            }
            let region = pick
                .region
                .or_else(|| (!positional.is_empty()).then(|| positional.join(" ")));
            if let Some(from) = pick.from.as_ref().filter(|f| f.is_image()) {
                check_image_region(from, region.as_deref())?;
            }
            Ok(Command::Shot(Shot {
                region,
                from: pick.from,
                output: pick.output,
            }))
        }
        _ => {
            if let Some(extra) = positional.first() {
                return Err(format!("unexpected argument {extra:?}"));
            }
            if let Some(from) = pick.from.as_ref().filter(|f| f.is_image()) {
                check_image_region(from, pick.region.as_deref())?;
            }
            Ok(Command::Pick(pick))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Result<Command, String> {
        parse(s.split_whitespace().map(OsString::from))
    }

    #[test]
    fn bare_screenie_is_pick() {
        let Command::Pick(p) = cmd("").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.png_level, PNG_LEVEL);
        assert!(p.output.copies());
        assert!(matches!(cmd("pick").unwrap(), Command::Pick(_)));
    }

    #[test]
    fn save_takes_a_path_but_not_a_region() {
        let Command::Pick(p) = cmd("--save ~/shots").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.save, Some(Some(PathBuf::from("~/shots"))));
        let Command::Shot(s) = cmd("shot --save monitor").unwrap() else {
            panic!()
        };
        assert_eq!(
            (s.output.save, s.region.as_deref()),
            (Some(None), Some("monitor"))
        );
        let Command::Pick(p) = cmd("--save=pics").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.save, Some(Some(PathBuf::from("pics"))));
        let Command::Pick(p) = cmd("--save out.png").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.save, Some(Some(PathBuf::from("out.png"))));
        // An option after it is never its directory.
        for line in ["--save --from=./pic.png", "--save --from=pic.png"] {
            let Command::Pick(p) = cmd(line).unwrap() else {
                panic!("{line}")
            };
            assert_eq!(p.output.save, Some(None), "{line}");
            assert!(matches!(p.from, Some(Source::File(_))), "{line}");
        }
    }

    #[test]
    fn shot_joins_a_slurp_geometry() {
        let Command::Shot(s) = cmd("shot 10,20 300x200 --stdout").unwrap() else {
            panic!()
        };
        assert_eq!(s.region.as_deref(), Some("10,20 300x200"));
        assert!(!s.output.copies()); // stdout carries the data
        let Command::Shot(s) = cmd("shot --stdout --copy").unwrap() else {
            panic!()
        };
        assert!(s.output.copies());
    }

    #[test]
    fn usage_errors() {
        assert!(cmd("--bogus").is_err());
        assert!(cmd("pick extra").is_err());
        assert!(cmd("shot --keys x").is_err());
        assert!(cmd("--png-level 12").is_err());
        assert!(cmd("shot all --region all").is_err());
        assert!(cmd("--stdout --json").is_err());
        assert!(cmd("--redact maybe").is_err());
        assert!(cmd("shot --redact suggest").is_err());
        assert!(cmd("--no-ui --redact suggest").is_err());
    }

    #[test]
    fn pin_verbs() {
        assert!(matches!(
            cmd("pin").unwrap(),
            Command::Pin {
                source: Source::Last(1),
                at: None,
                ..
            }
        ));
        assert!(matches!(
            cmd("pin pic.png --at 10,20").unwrap(),
            Command::Pin {
                source: Source::File(_),
                at: Some((10.0, 20.0)),
                ..
            }
        ));
        assert_eq!(
            cmd("pin list --json").unwrap(),
            Command::PinList {
                json: true,
                quiet: false
            }
        );
        assert_eq!(
            cmd("pin list --quiet").unwrap(),
            Command::PinList {
                json: false,
                quiet: true
            }
        );
        assert_eq!(
            cmd("pin close 2 3").unwrap(),
            Command::PinClose {
                ids: vec![2, 3],
                json: false,
                quiet: false
            }
        );
        assert_eq!(
            cmd("pin close --all --json").unwrap(),
            Command::PinClose {
                ids: vec![],
                json: true,
                quiet: false
            }
        );
        for bad in [
            "pin close",
            "pin close 1 --all",
            "pin close x",
            "pin --at 1",
            "pin a b",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
        assert!(cmd("--at 1,2").is_err(), "--at is pin's");
        // An option the verb doesn't use is an error, not ignored.
        for bad in [
            "pin --from x.png",
            "pin --format webp",
            "pin --region all",
            "pin list --all",
            "pin pic.png --all",
            "pin close 1 --at 1,2",
            "pin list --at 1,2",
            "history --save",
            "history --stdout",
            "help --redact auto",
            "completions bash --pin",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
        // The global ones are fine anywhere.
        for good in [
            "history --json",
            "history --no-history --quiet",
            "pin list --json",
            "pin close --all --json",
            "pin --quiet",
            "help keys --json",
        ] {
            assert!(cmd(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn scale() {
        let Command::Shot(s) = cmd("shot --scale 0.5").unwrap() else {
            panic!()
        };
        assert_eq!(s.output.scale, OutputScale::Factor(0.5));
        let Command::Pick(p) = cmd("--scale min").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.scale, OutputScale::Min);
        for bad in ["--scale 0", "--scale big", "--scale 20"] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn formats_and_quality() {
        let Command::Shot(s) = cmd("shot --format jpeg --quality 80").unwrap() else {
            panic!()
        };
        assert_eq!(
            (s.output.format, s.output.quality),
            (Format::Jpeg, Some(80))
        );
        let Command::Pick(p) = cmd("--format webp").unwrap() else {
            panic!()
        };
        assert_eq!(
            (p.output.format.ext(), p.output.format.mime()),
            ("webp", "image/webp")
        );
        for bad in ["--format gif", "--format jpeg --quality 0", "--quality 101"] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
        // Quality with another format is just unused (a config's quality
        // under a profile's format).
        assert!(cmd("--format webp --quality 50").is_ok());
    }

    #[test]
    fn delay_and_cursor_are_for_capturing_the_screen() {
        let Command::Shot(s) = cmd("shot --delay 2.5 --cursor").unwrap() else {
            panic!()
        };
        assert_eq!((s.output.delay, s.output.cursor), (Some(2.5), true));
        let Command::Pick(p) = cmd("--delay 3").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.delay, Some(3.0));
        for bad in [
            "--delay -1",
            "--delay soon",
            "--delay 3 --from pic.png",
            "shot --cursor --from pic.png",
            "edit pic.png --delay 1",
            "--delay 99999",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn from_a_file_stdin_or_a_past_shot() {
        let Command::Pick(p) = cmd("--from shot.png").unwrap() else {
            panic!()
        };
        assert_eq!(p.from, Some(Source::File(PathBuf::from("shot.png"))));
        let Command::Shot(s) = cmd("shot --from - 0,0 10x10").unwrap() else {
            panic!()
        };
        assert_eq!(
            (s.from, s.region.as_deref()),
            (Some(Source::Stdin), Some("0,0 10x10"))
        );
        let Command::Shot(s) = cmd("shot --from last:2").unwrap() else {
            panic!()
        };
        assert_eq!(s.from, Some(Source::Last(2)));
        assert_eq!(
            cmd("--from screen").unwrap(),
            cmd("").unwrap(),
            "screen is the default"
        );
        for bad in [
            "--from - --region -", // both on stdin
            "shot --from - -",     // the same, as shot's region
            "--from pic.png --region monitor",
            "shot --from pic.png active",
            "--from pic.png --region window:class:foo",
            "--from pic.png --region last-region",
            "edit pic.png --from other.png",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
        // A file's regions: the whole image, or a rect of its pixels.
        assert!(cmd("--from pic.png --region all").is_ok());
        assert!(cmd("shot --from pic.png 10,20 30x40").is_ok());
        // A region on stdin, the image in a file.
        assert!(cmd("shot --from pic.png -").is_ok());
        assert!(cmd("--from pic.png --region -").is_ok());
    }

    #[test]
    fn picker_options() {
        let Command::Pick(p) = cmd("--select-only --region-format hypr").unwrap() else {
            panic!()
        };
        assert!(p.select_only && p.output.stdout && !p.output.copies());
        assert_eq!(p.region_format, RegionFormat::Hypr);
        let Command::Pick(p) = cmd("--select-only --json").unwrap() else {
            panic!()
        };
        assert!(!p.output.stdout && !p.output.copies());
        let Command::Pick(p) = cmd("--select-only --json --copy").unwrap() else {
            panic!()
        };
        assert!(p.output.copies());
        let Command::Pick(p) = cmd("--result colour --mode insert --lock-box").unwrap() else {
            panic!()
        };
        assert_eq!(p.result, Some(screenie_core::select::EnterResult::Colour));
        assert_eq!(p.mode, Some(screenie_core::select::StartMode::Insert));
        assert!(p.lock_box);
        for bad in [
            "--region-format json",
            "--select-only --region-format xml",
            "--result sound",
            "--mode draw",
            "--mode censor --redact suggest",
            "--select-only --result text",
            "shot --select-only",
            "shot --mode censor",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn measure_scale() {
        let Command::Pick(p) = cmd("--measure-scale 84.2px=1m").unwrap() else {
            panic!()
        };
        assert_eq!(p.measure_scale.map(|s| s.px), Some(84.2));
        assert!(cmd("--measure-scale 84.2=1m").is_err());
        assert!(cmd("shot --measure-scale 1px=1m").is_err());
    }

    #[test]
    fn redact() {
        let Command::Shot(s) = cmd("shot --redact auto").unwrap() else {
            panic!()
        };
        assert_eq!(s.output.redact, Some(Redact::Auto));
        let Command::Pick(p) = cmd("--redact suggest").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.redact, Some(Redact::Suggest));
    }

    #[test]
    fn edit_and_history() {
        assert!(matches!(
            cmd("edit last").unwrap(),
            Command::Edit {
                source: Source::Last(1),
                ..
            }
        ));
        assert!(matches!(
            cmd("edit last:3").unwrap(),
            Command::Edit {
                source: Source::Last(3),
                ..
            }
        ));
        assert!(cmd("edit last:0").is_err());
        assert!(matches!(
            cmd("edit pic.png").unwrap(),
            Command::Edit {
                source: Source::File(_),
                ..
            }
        ));
        assert!(matches!(
            cmd("edit -").unwrap(),
            Command::Edit {
                source: Source::Stdin,
                ..
            }
        ));
        // Bare, the most recent image; or the clipboard's, by name.
        for (line, want) in [
            ("edit", Source::Recent),
            ("edit recent", Source::Recent),
            ("edit clipboard", Source::Clipboard),
        ] {
            let Command::Edit { source, .. } = cmd(line).unwrap() else {
                panic!("{line}")
            };
            assert_eq!(source, want, "{line}");
        }
        // The clipboard is an image: regions are its pixels.
        assert!(cmd("edit clipboard --region monitor").is_err());
        let Command::Pick(p) = cmd("--from clipboard").unwrap() else {
            panic!()
        };
        assert_eq!(p.from, Some(Source::Clipboard));
        assert_eq!(
            cmd("history -n 5 --json").unwrap(),
            Command::History {
                n: 5,
                json: true,
                quiet: false
            }
        );
        assert_eq!(
            cmd("history").unwrap(),
            Command::History {
                n: 20,
                json: false,
                quiet: false
            }
        );
        assert_eq!(
            cmd("history --quiet").unwrap(),
            Command::History {
                n: 20,
                json: false,
                quiet: true
            }
        );
    }

    #[test]
    fn every_flag_a_config_can_set_the_command_line_can_unset() {
        let Command::Pick(p) = cmd(
            "--json --no-json --stdout --pin --no-pin --save --no-save --quiet --no-quiet \
             --save-project --no-save-project --cursor --no-cursor --lock-box --no-lock-box",
        )
        .unwrap() else {
            panic!()
        };
        let o = &p.output;
        assert!(!o.json && !o.pin && o.save.is_none() && !o.quiet && !o.save_project);
        assert!(!o.cursor && !p.lock_box && o.stdout);
        assert!(cmd("shot --no-lock-box").is_ok_and(|c| matches!(c, Command::Shot(_))));
    }

    #[test]
    fn usage_names_what_the_parser_takes() {
        for want in [
            "active",
            "window:class:",
            "shot [REGION] [--from SOURCE]",
            "--save [DIR|TEMPLATE]",
        ] {
            assert!(USAGE.contains(want), "{want}");
        }
        for name in crate::help::VERBS.iter().chain(crate::help::TOPICS) {
            assert!(USAGE.contains(name), "{name}");
        }
    }

    #[test]
    fn every_verb_is_one() {
        for v in crate::help::VERBS {
            if let Err(e) = cmd(v) {
                assert!(!e.contains("unexpected argument"), "{v}: {e}");
            }
        }
    }

    #[test]
    fn ocr_lang_is_eng_unless_given() {
        assert_eq!(Output::default().ocr_lang(), "eng");
        let Command::Pick(p) = cmd("--ocr-lang deu").unwrap() else {
            panic!()
        };
        assert_eq!(p.output.ocr_lang(), "deu");
    }

    #[test]
    fn keys_and_no_ui() {
        let Command::Pick(p) =
            parse(["--keys", "x l Return", "--no-ui"].map(OsString::from)).unwrap()
        else {
            panic!()
        };
        assert_eq!(p.keys.as_deref(), Some("x l Return"));
        assert!(p.no_ui);
        let Command::Edit { pick, .. } = cmd("edit last --keys-file macro.txt").unwrap() else {
            panic!()
        };
        assert_eq!(pick.keys_file.as_deref(), Some("macro.txt"));
        assert!(cmd("--keys-file -").is_ok());
        for bad in [
            "--keys x --keys-file f",
            "--keys-file - --from -",
            "--keys-file - --region -",
            "edit - --keys-file -",
            "shot --keys-file f",
        ] {
            assert!(cmd(bad).is_err(), "{bad}");
        }
    }
}
