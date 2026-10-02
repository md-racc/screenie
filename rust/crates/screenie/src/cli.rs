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
}

impl Output {
    pub fn copies(&self) -> bool {
        self.copy.unwrap_or(!self.stdout)
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
    Help,
    Version,
    /// Internal: serve the clipboard (see platform::clipboard).
    ServeClipboard,
}

pub const USAGE: &str = "\
usage: screenie [pick] [--region REGION] [--keys KEYS] [--no-ui] [OUTPUT OPTIONS]
       screenie shot [REGION] [OUTPUT OPTIONS]

output options: --save [DIR]  --stdout  --copy | --no-copy  --png-level 0-9
                --notify | --no-notify  --quiet  --no-history

REGION: all | monitor | monitor:NAME | X,Y WxH | last-region[:N] | -
See README.md for the picker's keys, and docs/cli.md for the rest.";

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
            Arg::Long("copy") => o.copy = Some(true),
            Arg::Long("no-copy") => o.copy = Some(false),
            Arg::Long("notify") => o.notify = Some(true),
            Arg::Long("no-notify") => o.notify = Some(false),
            Arg::Long("quiet") => o.quiet = true,
            Arg::Long("no-history") => o.no_history = true,
            Arg::Long("png-level") => {
                let n: u8 = p.value().map_err(err)?.parse().map_err(err)?;
                if n > 9 {
                    return Err("--png-level is 0-9".into());
                }
                o.png_level = n;
            }
            Arg::Value(v) => {
                let v = v.string().map_err(err)?;
                if verb.is_none() && positional.is_empty() && (v == "pick" || v == "shot") {
                    verb = Some(v);
                } else {
                    positional.push(v);
                }
            }
            other => return Err(other.unexpected().to_string()),
        }
    }
    match verb.as_deref() {
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
