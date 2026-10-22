//! `screenie help keys|regions|config` and `screenie completions SHELL`:
//! the reference, made from the same data the picker and parser use.

use crate::config::{KEYS, Kind, NOT_YET};

/// Every verb.
pub const VERBS: &[&str] = &[
    "pick",
    "shot",
    "edit",
    "history",
    "pin",
    "help",
    "completions",
];

/// Every long option, and the values it completes to (empty: a free
/// value; `None`: a flag).
pub const OPTIONS: &[(&str, Option<&[&str]>)] = &[
    (
        "--from",
        Some(&["screen", "-", "clipboard", "recent", "last"]),
    ),
    (
        "--region",
        Some(&["all", "monitor", "active", "last-region"]),
    ),
    ("--keys", Some(&[])),
    ("--keys-file", Some(&[])),
    ("--no-ui", None),
    (
        "--mode",
        Some(&[
            "select", "insert", "censor", "annotate", "filter", "measure",
        ]),
    ),
    (
        "--result",
        Some(&["image", "text", "colour", "code", "pin", "measure"]),
    ),
    ("--lock-box", None),
    ("--no-lock-box", None),
    ("--select-only", None),
    ("--region-format", Some(&["slurp", "json", "hypr"])),
    ("--measure-scale", Some(&[])),
    ("--redact", Some(&["suggest", "auto"])),
    ("--delay", Some(&[])),
    ("--cursor", None),
    ("--no-cursor", None),
    ("--scale", Some(&["native", "min", "1", "2", "0.5"])),
    ("--save", None),
    ("--no-save", None),
    ("--save-project", None),
    ("--no-save-project", None),
    ("--stdout", None),
    ("--json", None),
    ("--no-json", None),
    ("--copy", None),
    ("--no-copy", None),
    ("--notify", None),
    ("--no-notify", None),
    ("--quiet", None),
    ("--no-quiet", None),
    ("--no-history", None),
    ("--format", Some(&["png", "jpeg", "webp", "ppm"])),
    ("--quality", Some(&[])),
    (
        "--png-level",
        Some(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]),
    ),
    ("--pin", None),
    ("--no-pin", None),
    ("--colour-format", Some(&["hex", "rgb", "hsl"])),
    ("--copy-uri", None),
    ("--no-copy-uri", None),
    ("--ocr-lang", Some(&[])),
    ("--at", Some(&[])),
    ("--all", None),
    ("--profile", Some(&[])),
    ("--config", Some(&[])),
    ("--no-config", None),
    ("--help", None),
    ("--version", None),
];

pub const TOPICS: &[&str] = &["keys", "regions", "config"];
pub const SHELLS: &[&str] = &["bash", "zsh", "fish"];

const REGIONS: &str = "\
REGION (--region, or shot's argument):
  active                     the focused window, from its own pixels
  window:class:REGEX         a window by class (the most recently focused of several)
  window:title:REGEX         a window by title
  window:address:0x...       an exact window
  window:pid:N               a window by its process
  monitor | monitor:NAME     the focused monitor, or one by name
  all                        everything: the whole layout, or the whole image
  X,Y WxH                    a rectangle (slurp's format), logical pixels
  last-region[:N]            a box the picker's . ring remembers
  -                          one of these, read from stdin

Over an image (--from FILE or -), only all and X,Y WxH apply, in its pixels.";

/// `help TOPIC`.
pub fn topic(name: &str) -> Result<String, String> {
    match name {
        "keys" => Ok(keys()),
        "regions" => Ok(REGIONS.into()),
        "config" => Ok(config()),
        _ => Err(format!("help {name:?}: {}", TOPICS.join(", "))),
    }
}

fn keys() -> String {
    let mut out =
        String::from("The picker's keys, by mode (? in the picker shows the current one's).\n");
    for (mode, lines) in screenie_core::legend::reference() {
        out.push_str(&format!("\n{}\n", mode.to_uppercase()));
        for l in lines {
            out.push_str(&format!("  {l}\n"));
        }
    }
    out
}

fn config() -> String {
    let mut out = String::from(
        "$XDG_CONFIG_HOME/screenie/config.toml: each key is the long option of its name,\n\
         [profile.NAME] sections override them with -p NAME, and the command line wins.\n",
    );
    let mut section = "";
    for (s, key, kind, verbs) in KEYS {
        if *s != section {
            out.push_str(&format!("\n[{s}]\n"));
            section = s;
        }
        let what = match kind {
            Kind::Bool(on, off) => format!("true or false: {on} / {off}"),
            Kind::Value(opt) => format!("{opt} VALUE"),
            Kind::Save => "a directory or template, or true or false: --save".into(),
            Kind::Keep => "0: --no-history".into(),
            Kind::Redact => "off, suggest or auto: --redact".into(),
            Kind::Only(what) => what.to_string(),
        };
        out.push_str(&format!("  {key:<14} {what}  ({})\n", verbs.join(", ")));
    }
    let not_yet: Vec<String> = NOT_YET
        .iter()
        .map(|s| match *s {
            "keys" => "[keys.*]".to_string(),
            s => format!("[{s}]"),
        })
        .collect();
    out.push_str(&format!(
        "\nReserved (warned about, skipped): {}\n",
        not_yet.join(", ")
    ));
    out
}

/// `completions SHELL`.
pub fn completions(shell: &str) -> Result<String, String> {
    let flags: Vec<&str> = OPTIONS.iter().map(|(o, _)| *o).collect();
    match shell {
        "bash" => {
            let mut cases = String::new();
            for (opt, vals) in OPTIONS {
                if let Some(v) = vals.filter(|v| !v.is_empty()) {
                    cases.push_str(&format!(
                        "        {opt}) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return;;\n",
                        v.join(" ")
                    ));
                }
            }
            Ok(format!(
                "# bash completion for screenie\n\
                 _screenie() {{\n    local cur=${{COMP_WORDS[COMP_CWORD]}} prev=${{COMP_WORDS[COMP_CWORD-1]}}\n    case $prev in\n{cases}        help) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return;;\n        completions) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return;;\n    esac\n    COMPREPLY=($(compgen -W \"{} {}\" -- \"$cur\"))\n}}\ncomplete -o default -F _screenie screenie\n",
                TOPICS.join(" "),
                SHELLS.join(" "),
                VERBS.join(" "),
                flags.join(" ")
            ))
        }
        "zsh" => {
            let mut specs = String::new();
            for (opt, vals) in OPTIONS {
                let spec = match vals {
                    None => format!("'{opt}'"),
                    Some([]) => format!("'{opt}:value:'"),
                    Some(v) => format!("'{opt}:value:({})'", v.join(" ")),
                };
                specs.push_str(&format!("    {spec} \\\n"));
            }
            Ok(format!(
                "#compdef screenie\n_arguments \\\n{specs}    '1:verb:({})' \\\n    '*:file:_files'\n",
                VERBS.join(" ")
            ))
        }
        "fish" => {
            let mut out = String::from("# fish completion for screenie\n");
            out.push_str(&format!(
                "complete -c screenie -n __fish_use_subcommand -a '{}'\n",
                VERBS.join(" ")
            ));
            out.push_str(&format!(
                "complete -c screenie -n '__fish_seen_subcommand_from help' -a '{}'\n",
                TOPICS.join(" ")
            ));
            out.push_str(&format!(
                "complete -c screenie -n '__fish_seen_subcommand_from completions' -a '{}'\n",
                SHELLS.join(" ")
            ));
            for (opt, vals) in OPTIONS {
                let name = opt.trim_start_matches("--");
                match vals {
                    None => out.push_str(&format!("complete -c screenie -l {name}\n")),
                    Some(v) => out.push_str(&format!(
                        "complete -c screenie -l {name} -r -a '{}'\n",
                        v.join(" ")
                    )),
                }
            }
            Ok(out)
        }
        _ => Err(format!("completions {shell:?}: {}", SHELLS.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn every_completed_option_is_one_the_parser_knows() {
        for (opt, vals) in OPTIONS {
            if matches!(
                *opt,
                "--profile" | "--config" | "--no-config" | "--help" | "--version"
            ) {
                continue; // taken off before the parser, or answered at once
            }
            let value = match vals {
                None => None,
                Some(v) if !v.is_empty() => Some(v[0].to_string()),
                Some(_) => Some(
                    match *opt {
                        "--keys" => "Return",
                        "--measure-scale" => "10px=1m",
                        "--delay" => "1",
                        "--quality" => "80",
                        "--ocr-lang" => "eng",
                        _ => "x",
                    }
                    .to_string(),
                ),
            };
            // pin's own options follow it.
            let mut args: Vec<OsString> = match *opt {
                "--at" | "--all" => vec!["pin".into()],
                _ => vec![],
            };
            args.push(OsString::from(*opt));
            args.extend(value.map(OsString::from));
            if let Err(e) = crate::cli::parse(args) {
                assert!(!e.contains("invalid option"), "{opt}: {e}");
            }
        }
    }

    #[test]
    fn every_option_the_parser_knows_is_completed() {
        let parser = include_str!("cli.rs");
        for long in parser.split("Arg::Long(\"").skip(1) {
            let name = &long[..long.find('"').unwrap()];
            if name == "color-format" {
                continue; // colour-format's other spelling
            }
            let opt = format!("--{name}");
            assert!(OPTIONS.iter().any(|(o, _)| *o == opt), "{opt}");
        }
    }

    #[test]
    fn topics_and_shells() {
        assert!(topic("keys").unwrap().contains("MEASURE"));
        assert!(topic("regions").unwrap().contains("window:class:REGEX"));
        let config = topic("config").unwrap();
        assert!(config.contains("[output]") && config.contains("[look], [keys.*]"));
        assert!(config.contains("a directory or template, or true or false: --save"));
        assert!(topic("nope").is_err());
        for (sh, format) in [
            ("bash", "--format"),
            ("zsh", "--format"),
            ("fish", "-l format"),
        ] {
            assert!(completions(sh).unwrap().contains(format), "{sh}");
        }
        assert!(completions("tcsh").is_err());
    }
}
