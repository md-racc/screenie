//! The config file (docs/cli.md, CONFIG): `$XDG_CONFIG_HOME/screenie/
//! config.toml`, its keys named as the long options they stand for, and
//! named profiles (`-p NAME`) over it. It becomes options put before the
//! command line's own, so the command line's parser checks it, and the
//! command line wins (later options override earlier ones).

use std::ffi::OsString;
use std::path::PathBuf;

/// The options that pick the config, taken off the command line.
#[derive(Debug, Default, PartialEq)]
pub struct Global {
    /// `--config PATH`.
    pub config: Option<PathBuf>,
    /// `--no-config`.
    pub no_config: bool,
    /// `-p NAME` / `--profile NAME`, in order.
    pub profiles: Vec<String>,
    /// Everything else, for the command line's parser.
    pub rest: Vec<OsString>,
}

/// Take `--config`, `--no-config` and `-p`/`--profile` off the arguments.
pub fn split_globals(args: Vec<OsString>) -> Result<Global, String> {
    let mut g = Global::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let s = a.to_string_lossy().to_string();
        let mut value = |name: &str| {
            it.next()
                .map(|v| v.to_string_lossy().to_string())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match s.as_str() {
            "--no-config" => g.no_config = true,
            "--config" => g.config = Some(PathBuf::from(value("--config")?)),
            "-p" | "--profile" => g.profiles.push(value(&s)?),
            _ if s.starts_with("--config=") => g.config = Some(PathBuf::from(&s[9..])),
            _ if s.starts_with("--profile=") => g.profiles.push(s[10..].to_string()),
            _ if s.starts_with("-p") && s.len() > 2 && !s.starts_with("--") => {
                g.profiles.push(s[2..].to_string());
            }
            _ => g.rest.push(a),
        }
    }
    if g.no_config && (g.config.is_some() || !g.profiles.is_empty()) {
        return Err("--no-config, and a config or a profile too?".into());
    }
    Ok(g)
}

/// The verb the command line runs, for which config applies.
pub fn verb(args: &[OsString]) -> &'static str {
    for a in args {
        match a.to_string_lossy().as_ref() {
            "pick" => return "pick",
            "shot" => return "shot",
            "edit" => return "edit",
            "history" => return "history",
            "pin" => return "pin",
            _ => {}
        }
    }
    "pick"
}

/// The default config file.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("screenie/config.toml"))
}

/// The config file's text, if there's one to read: a `--config` file must
/// exist, the default one needn't.
pub fn read(g: &Global) -> Result<Option<String>, String> {
    if g.no_config {
        return Ok(None);
    }
    match &g.config {
        Some(p) => std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| format!("{}: {e}", p.display())),
        None => match default_path() {
            Some(p) if p.exists() => std::fs::read_to_string(&p)
                .map(Some)
                .map_err(|e| format!("{}: {e}", p.display())),
            _ if !g.profiles.is_empty() => {
                Err("-p: there's no config file to have profiles".into())
            }
            _ => Ok(None),
        },
    }
}

/// How a key becomes an option.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// `true` gives the flag; `false`, the negation if there is one.
    Bool(&'static str, Option<&'static str>),
    /// A value: `--option VALUE`.
    Value(&'static str),
    /// `save`: a path, `true` (the default directory) or `false` (none).
    Save,
    /// `history.keep`: 0 is `--no-history`.
    Keep,
    /// `redact.mode`: `off`, `suggest` or `auto`.
    Redact,
}

const ALL: &[&str] = &["pick", "shot", "edit"];
const PICKERS: &[&str] = &["pick", "edit"];
const CAPTURES: &[&str] = &["pick", "shot"];

/// Every key: section, name, how, and for which verbs.
pub(crate) const KEYS: &[(&str, &str, Kind, &[&str])] = &[
    (
        "output",
        "copy",
        Kind::Bool("--copy", Some("--no-copy")),
        ALL,
    ),
    (
        "output",
        "notify",
        Kind::Bool("--notify", Some("--no-notify")),
        ALL,
    ),
    ("output", "quiet", Kind::Bool("--quiet", None), ALL),
    ("output", "json", Kind::Bool("--json", None), ALL),
    ("output", "save", Kind::Save, ALL),
    (
        "output",
        "save-project",
        Kind::Bool("--save-project", None),
        ALL,
    ),
    ("output", "format", Kind::Value("--format"), ALL),
    ("output", "quality", Kind::Value("--quality"), ALL),
    ("output", "png-level", Kind::Value("--png-level"), ALL),
    ("output", "pin", Kind::Bool("--pin", None), ALL),
    (
        "output",
        "colour-format",
        Kind::Value("--colour-format"),
        ALL,
    ),
    (
        "output",
        "copy-uri",
        Kind::Bool("--copy-uri", Some("--no-copy-uri")),
        ALL,
    ),
    ("ocr", "lang", Kind::Value("--ocr-lang"), ALL),
    ("pick", "region", Kind::Value("--region"), &["pick"]),
    ("pick", "mode", Kind::Value("--mode"), PICKERS),
    ("pick", "result", Kind::Value("--result"), PICKERS),
    ("pick", "lock-box", Kind::Bool("--lock-box", None), PICKERS),
    (
        "pick",
        "measure-scale",
        Kind::Value("--measure-scale"),
        PICKERS,
    ),
    ("capture", "delay", Kind::Value("--delay"), CAPTURES),
    ("capture", "cursor", Kind::Bool("--cursor", None), CAPTURES),
    ("capture", "scale", Kind::Value("--scale"), ALL),
    ("history", "keep", Kind::Keep, ALL),
    ("redact", "mode", Kind::Redact, ALL),
];

/// Keys the design has that aren't built yet: warned about, not refused.
pub(crate) const NOT_YET: &[&str] = &[
    "redact.detectors",
    "redact.literal",
    "redact.custom",
    "redact.prescan",
];

/// The options a config (and its profiles, in order) stands for, for
/// `verb`, and warnings about keys it skipped.
pub fn options(
    text: &str,
    verb: &str,
    profiles: &[String],
) -> Result<(Vec<OsString>, Vec<String>), String> {
    let mut table: toml::Table = text.parse().map_err(|e| format!("config: {e}"))?;
    let profile_table = match table.remove("profile") {
        None => toml::Table::new(),
        Some(toml::Value::Table(t)) => t,
        Some(_) => return Err("config: [profile.NAME] sections, please".into()),
    };
    for name in profiles {
        let p = match profile_table.get(name) {
            Some(toml::Value::Table(t)) => t,
            _ => return Err(format!("config: no profile {name:?}")),
        };
        merge(&mut table, p);
    }
    let mut args = Vec::new();
    let mut warnings = Vec::new();
    for (section, value) in &table {
        let toml::Value::Table(keys) = value else {
            return Err(format!("config: {section} should be a [section]"));
        };
        if section == "look" || section == "keys" {
            warnings.push(format!(
                "config: [{section}] isn't built yet, so it's skipped"
            ));
            continue;
        }
        for (key, v) in keys {
            let name = format!("{section}.{key}");
            let Some(&(_, _, kind, verbs)) =
                KEYS.iter().find(|(s, k, ..)| *s == section && *k == key)
            else {
                if NOT_YET.contains(&name.as_str()) {
                    warnings.push(format!("config: {name} isn't built yet, so it's skipped"));
                    continue;
                }
                return Err(format!("config: unknown key {name}"));
            };
            if !verbs.contains(&verb) {
                continue; // not for this verb
            }
            args.extend(option(&name, kind, v, verb)?);
        }
    }
    Ok((args.into_iter().map(OsString::from).collect(), warnings))
}

/// A profile's keys over the base's, section by section.
fn merge(base: &mut toml::Table, over: &toml::Table) {
    for (k, v) in over {
        match (base.get_mut(k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            _ => {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

fn option(name: &str, kind: Kind, v: &toml::Value, verb: &str) -> Result<Vec<String>, String> {
    let as_bool = || {
        v.as_bool()
            .ok_or_else(|| format!("config: {name}: true or false"))
    };
    let as_text = || match v {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(n) => Ok(n.to_string()),
        toml::Value::Float(x) => Ok(x.to_string()),
        _ => Err(format!("config: {name}: a value, like the option takes")),
    };
    Ok(match kind {
        Kind::Bool(on, off) => match (as_bool()?, off) {
            (true, _) => vec![on.into()],
            (false, Some(off)) => vec![off.into()],
            (false, None) => vec![],
        },
        Kind::Value(opt) => vec![opt.into(), as_text()?],
        Kind::Save => match v {
            toml::Value::Boolean(true) => vec!["--save".into()],
            toml::Value::Boolean(false) => vec![],
            toml::Value::String(s) => vec![format!("--save={s}")],
            _ => return Err(format!("config: {name}: a directory, or true or false")),
        },
        Kind::Keep => match v.as_integer() {
            Some(0) => vec!["--no-history".into()],
            Some(n) if n > 0 => vec![], // the default keeps 20; others aren't built
            _ => return Err(format!("config: {name}: a count, 0 for none")),
        },
        Kind::Redact => match as_text()?.as_str() {
            "off" => vec![],
            // Suggesting needs the picker; shot only auto-redacts.
            "suggest" if verb == "shot" => vec![],
            m @ ("suggest" | "auto") => vec!["--redact".into(), m.into()],
            m => return Err(format!("config: {name}: off, suggest or auto, not {m:?}")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str, verb: &str, profiles: &[&str]) -> Result<Vec<String>, String> {
        let p: Vec<String> = profiles.iter().map(|s| s.to_string()).collect();
        options(text, verb, &p)
            .map(|(a, _)| a.iter().map(|s| s.to_string_lossy().to_string()).collect())
    }

    const CONFIG: &str = r#"
[output]
copy = false
save = "~/pics"
format = "png"
png-level = 1

[pick]
region = "active"
mode = "censor"

[capture]
delay = 2

[profile.share]
output.format = "webp"
output.save = false
redact.mode = "suggest"

[profile.loud]
output.notify = true
"#;

    #[test]
    fn keys_become_their_options_for_the_verb() {
        let a = args(CONFIG, "pick", &[]).unwrap();
        for want in [
            "--no-copy",
            "--save=~/pics",
            "--format",
            "png",
            "--region",
            "active",
            "--mode",
            "censor",
            "--delay",
            "2",
        ] {
            assert!(a.contains(&want.to_string()), "{want}: {a:?}");
        }
        // shot has no picker, and edit's regions are its image's.
        let a = args(CONFIG, "shot", &[]).unwrap();
        assert!(!a.contains(&"--mode".to_string()) && a.contains(&"--delay".to_string()));
        let a = args(CONFIG, "edit", &[]).unwrap();
        assert!(!a.contains(&"--region".to_string()) && !a.contains(&"--delay".to_string()));
    }

    #[test]
    fn profiles_apply_over_the_base_in_order() {
        let a = args(CONFIG, "pick", &["share", "loud"]).unwrap();
        assert!(a.windows(2).any(|w| w == ["--format", "webp"]), "{a:?}");
        assert!(
            !a.iter().any(|s| s.starts_with("--save")),
            "save = false: {a:?}"
        );
        assert!(a.windows(2).any(|w| w == ["--redact", "suggest"]));
        assert!(a.contains(&"--notify".to_string()));
        // On shot, suggesting means nothing (it has no picker).
        let a = args(CONFIG, "shot", &["share"]).unwrap();
        assert!(!a.contains(&"--redact".to_string()));
        assert!(
            args(CONFIG, "pick", &["nope"])
                .unwrap_err()
                .contains("no profile")
        );
    }

    #[test]
    fn bad_and_unbuilt_keys() {
        assert!(
            args("[output]\ncolour = 1", "pick", &[])
                .unwrap_err()
                .contains("unknown key output.colour")
        );
        assert!(args("[output]\ncopy = \"yes\"", "pick", &[]).is_err());
        assert!(args("[redact]\nmode = \"maybe\"", "pick", &[]).is_err());
        assert!(args("not toml [", "pick", &[]).is_err());
        let (a, warnings) =
            options("[look]\ndim = 0.4\n[redact]\nprescan = true", "pick", &[]).unwrap();
        assert!(a.is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(
            args("[output]\ncopy-uri = false", "pick", &[]).unwrap(),
            ["--no-copy-uri"]
        );
        assert_eq!(
            args("[history]\nkeep = 0", "shot", &[]).unwrap(),
            ["--no-history"]
        );
    }

    #[test]
    fn the_globals_come_off_the_command_line() {
        let g = split_globals(
            [
                "shot",
                "-p",
                "share",
                "--profile=docs",
                "-pwork",
                "--config",
                "/c.toml",
                "all",
            ]
            .map(OsString::from)
            .to_vec(),
        )
        .unwrap();
        assert_eq!(g.profiles, ["share", "docs", "work"]);
        assert_eq!(g.config, Some(PathBuf::from("/c.toml")));
        assert_eq!(g.rest, ["shot", "all"].map(OsString::from));
        assert_eq!(verb(&g.rest), "shot");
        assert!(split_globals(["--no-config", "-p", "x"].map(OsString::from).to_vec()).is_err());
        assert!(split_globals(["-p"].map(OsString::from).to_vec()).is_err());
    }
}
