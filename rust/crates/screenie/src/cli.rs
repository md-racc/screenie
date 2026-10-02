//! The command line (docs/cli.md). Only what the port has so far is
//! accepted; anything else is a usage error rather than silently ignored.

use std::ffi::OsString;
use std::path::PathBuf;

use lexopt::{Arg, Parser, ValueExt};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// `--save [DIR]`: `Some(None)` for the default directory.
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
    /// The tesseract language; `None` is `$SCREENIE_LANG`, else `eng`.
    pub ocr_lang: Option<String>,
    /// `--json`: one line describing what happened.
    pub json: bool,
    /// `--save-project`: the project beside the `--save` image too.
    pub save_project: bool,
    /// `--redact suggest|auto`.
    pub redact: Option<Redact>,
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
        self.ocr_lang
            .clone()
            .or_else(|| {
                std::env::var("SCREENIE_LANG")
                    .ok()
                    .filter(|l| !l.is_empty())
            })
            .unwrap_or_else(|| "eng".into())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pick {
    pub region: Option<String>,
    pub keys: Option<String>,
    pub no_ui: bool,
    pub output: Output,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shot {
    pub region: Option<String>,
    pub output: Output,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Pick(Pick),
    Shot(Shot),
    /// `edit last[:N]`: the picker over a past shot's project.
    Edit {
        which: usize,
        pick: Pick,
    },
    /// `history [-n N] [--json]`.
    History {
        n: usize,
        json: bool,
    },
    Help,
    Version,
    /// Internal: serve the clipboard (see platform::clipboard).
    ServeClipboard,
}

pub const USAGE: &str = "\
usage: screenie [pick] [--region REGION] [--keys KEYS] [--no-ui] [OUTPUT OPTIONS]
       screenie shot [REGION] [OUTPUT OPTIONS]
       screenie edit last[:N] [--keys KEYS] [--no-ui] [OUTPUT OPTIONS]
       screenie history [-n N] [--json]

output options: --save [DIR]  --stdout  --copy | --no-copy  --png-level 0-9
                --notify | --no-notify  --quiet  --no-history  --ocr-lang LANG  --json
                --save-project  --redact suggest|auto

REGION: all | monitor | monitor:NAME | X,Y WxH | last-region[:N] | -
--redact auto is best effort: OCR misses small, stylised, low-contrast and
rotated text, and text in images. --json lists what it redacted.
See README.md for the picker's keys, and docs/cli.md for the rest.";

/// `last` (1) or `last:N`. Plain image files aren't editable yet.
fn parse_last(s: &str) -> Result<usize, String> {
    match s.strip_prefix("last") {
        Some("") => Ok(1),
        Some(n) => n
            .strip_prefix(':')
            .and_then(|n| n.parse().ok())
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("bad shot {s:?}: last or last:N")),
        None => Err("only past shots (last, last:N) can be edited so far".into()),
    }
}

fn looks_like_a_path(s: &str) -> bool {
    s.contains('/') || s.starts_with('~') || s.starts_with('.')
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
    let mut p = Parser::from_args(args);
    let mut verb: Option<String> = None;
    let mut pick = Pick {
        output: Output {
            png_level: 1,
            ..Output::default()
        },
        ..Pick::default()
    };
    let mut positional: Vec<String> = Vec::new();
    let mut history_n: usize = 20;
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
            Arg::Long("no-ui") => pick.no_ui = true,
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
            Arg::Long("stdout") => o.stdout = true,
            Arg::Long("json") => o.json = true,
            Arg::Long("save-project") => o.save_project = true,
            Arg::Long("redact") => {
                o.redact = Some(
                    match p.value().map_err(err)?.string().map_err(err)?.as_str() {
                        "suggest" => Redact::Suggest,
                        "auto" => Redact::Auto,
                        other => return Err(format!("--redact {other:?}: suggest or auto")),
                    },
                )
            }
            Arg::Short('n') if verb.as_deref() == Some("history") => {
                history_n = p.value().map_err(err)?.parse().map_err(err)?;
            }
            Arg::Long("copy") => o.copy = Some(true),
            Arg::Long("no-copy") => o.copy = Some(false),
            Arg::Long("notify") => o.notify = Some(true),
            Arg::Long("no-notify") => o.notify = Some(false),
            Arg::Long("quiet") => o.quiet = true,
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
                    && matches!(v.as_str(), "pick" | "shot" | "edit" | "history")
                {
                    verb = Some(v);
                } else {
                    positional.push(v);
                }
            }
            other => return Err(other.unexpected().to_string()),
        }
    }
    if pick.output.stdout && pick.output.json {
        return Err("--stdout and --json both claim stdout: pick one".into());
    }
    if pick.output.redact == Some(Redact::Suggest)
        && (pick.no_ui || verb.as_deref() == Some("shot"))
    {
        return Err("--redact suggest needs the picker: use --redact auto".into());
    }
    match verb.as_deref() {
        Some("history") => {
            if let Some(extra) = positional.first() {
                return Err(format!("unexpected argument {extra:?}"));
            }
            Ok(Command::History {
                n: history_n,
                json: pick.output.json,
            })
        }
        Some("edit") => {
            let which = match positional.as_slice() {
                [w] => parse_last(w)?,
                [] => return Err("edit what? (edit last, or last:N)".into()),
                _ => return Err("edit takes one shot".into()),
            };
            Ok(Command::Edit { which, pick })
        }
        Some("shot") => {
            if pick.keys.is_some() || pick.no_ui {
                return Err("shot has no picker: --keys and --no-ui are for pick".into());
            }
            if pick.region.is_some() && !positional.is_empty() {
                return Err("give the region once".into());
            }
            let region = pick
                .region
                .or_else(|| (!positional.is_empty()).then(|| positional.join(" ")));
            Ok(Command::Shot(Shot {
                region,
                output: pick.output,
            }))
        }
        _ => {
            if let Some(extra) = positional.first() {
                return Err(format!("unexpected argument {extra:?}"));
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
        assert_eq!(p.output.png_level, 1);
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
            Command::Edit { which: 1, .. }
        ));
        assert!(matches!(
            cmd("edit last:3").unwrap(),
            Command::Edit { which: 3, .. }
        ));
        assert!(cmd("edit last:0").is_err());
        assert!(cmd("edit pic.png").is_err());
        assert_eq!(
            cmd("history -n 5 --json").unwrap(),
            Command::History { n: 5, json: true }
        );
        assert_eq!(
            cmd("history").unwrap(),
            Command::History { n: 20, json: false }
        );
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
    }
}
