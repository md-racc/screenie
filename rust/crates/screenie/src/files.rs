//! Where screenie keeps things (docs/cli.md, FILES).

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Monitor;
use screenie_core::recall::Ring;

/// Shots kept in the runtime directory.
pub const KEEP: usize = 20;

fn env_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(fallback))
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// `~` at the start of a path is the home directory.
pub fn expand(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => home().join(rest),
        Err(_) => p.to_path_buf(),
    }
}

/// `$XDG_RUNTIME_DIR/screenie`: tmpfs, private, cleared at logout.
/// Without it, `screenie-UID` in the temp directory.
pub fn runtime_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    // The process's own directory is its user's.
    let uid = fs::metadata("/proc/self")?.uid();
    let d = runtime_base(std::env::var_os("XDG_RUNTIME_DIR"), uid);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&d)?;
    ensure_private(&d, uid)?;
    Ok(d)
}

/// Where the runtime directory goes: under `XDG_RUNTIME_DIR` if it's an
/// absolute path (the XDG spec ignores others), else the temp directory.
fn runtime_base(xdg: Option<std::ffi::OsString>, uid: u32) -> PathBuf {
    match xdg.map(PathBuf::from).filter(|p| p.is_absolute()) {
        Some(base) => base.join("screenie"),
        None => std::env::temp_dir().join(format!("screenie-{uid}")),
    }
}

/// A runtime directory someone else could have made first (in a shared
/// temp directory) must be a real directory, `uid`'s, and only theirs.
fn ensure_private(d: &Path, uid: u32) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = fs::symlink_metadata(d)?;
    if !meta.file_type().is_dir() || meta.uid() != uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} isn't a directory of this user's", d.display()),
        ));
    }
    if meta.mode() & 0o077 != 0 {
        fs::set_permissions(d, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// `$XDG_STATE_HOME/screenie/regions.json`: the recall ring.
pub fn ring_path() -> PathBuf {
    env_dir("XDG_STATE_HOME", ".local/state")
        .join("screenie")
        .join("regions.json")
}

/// The default `--save` directory: `$XDG_PICTURES_DIR/Screenshots`.
pub fn pictures_dir() -> PathBuf {
    env_dir("XDG_PICTURES_DIR", "Pictures").join("Screenshots")
}

pub fn load_ring() -> Ring {
    fs::read_to_string(ring_path()).map_or_else(|_| Ring::default(), |s| Ring::parse(&s))
}

/// Put an accepted box at the front of the ring. A failed write never
/// fails the shot.
pub fn remember(monitors: &[Monitor], rect: LogicalRect) {
    let mut ring = load_ring();
    ring.remember(monitors, rect);
    let p = ring_path();
    let write = || -> io::Result<()> {
        fs::create_dir_all(p.parent().expect("has a parent"))?;
        let tmp = p.with_extension("json.tmp");
        fs::write(&tmp, ring.to_json())?;
        fs::rename(tmp, &p)
    };
    let _ = write();
}

/// `dir/stamp.ext`, or `stamp-1.ext`, `stamp-2.ext`... if taken, created
/// exclusively so two shots in one second never overwrite each other.
/// With `beside`, a name whose file of that extension is taken (a
/// project to go with it) counts as taken too.
pub fn create_unique(
    dir: &Path,
    stamp: &str,
    ext: &str,
    mode: u32,
    beside: Option<&str>,
) -> io::Result<(PathBuf, fs::File)> {
    fs::create_dir_all(dir)?;
    for n in 0u32.. {
        let name = if n == 0 {
            format!("{stamp}.{ext}")
        } else {
            format!("{stamp}-{n}.{ext}")
        };
        let path = dir.join(name);
        if beside.is_some_and(|b| fs::symlink_metadata(path.with_extension(b)).is_ok()) {
            continue;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&path)
        {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// The shots in `dir` (their PNGs), newest first.
pub fn shots_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    files.into_iter().map(|(_, p)| p).collect()
}

/// The runtime shots, newest first.
pub fn shots() -> Vec<PathBuf> {
    runtime_dir().map_or_else(|_| vec![], |d| shots_in(&d.join("shots")))
}

/// A shot's project file.
pub fn project_of(shot: &Path) -> PathBuf {
    shot.with_extension("screenie")
}

/// The `n`th newest shot in `dir` (from 1), and its project if it kept one.
pub fn last_in(dir: &Path, n: usize) -> Option<(PathBuf, Option<PathBuf>)> {
    let shot = shots_in(dir).into_iter().nth(n.checked_sub(1)?)?;
    let project = Some(project_of(&shot)).filter(|p| p.is_file());
    Some((shot, project))
}

/// Keep the newest `keep` shots in `dir`, with their projects.
pub fn prune(dir: &Path, keep: usize) {
    for old in shots_in(dir).into_iter().skip(keep) {
        let _ = fs::remove_file(project_of(&old));
        let _ = fs::remove_file(old);
    }
}

/// How shots are named: the local time.
const STAMP: &str = "%Y-%m-%d_%H-%M-%S";

/// The local time as a file name stamp.
pub fn stamp() -> String {
    jiff::Zoned::now().strftime(STAMP).to_string()
}

/// What a `--save` template's fields stand for.
#[derive(Clone, Debug, Default)]
pub struct Naming {
    pub class: Option<String>,
    pub title: Option<String>,
    pub monitor: Option<String>,
    pub w: u32,
    pub h: u32,
    /// The result's kind (`image`).
    pub kind: String,
}

/// The extensions a `--save` file name may give (its format still wins).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "ppm"];

/// Whether a name ends in one of `IMAGE_EXTENSIONS`.
pub fn has_image_extension(s: &str) -> bool {
    s.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// Whether `--save`'s argument names the file (a template, or a plain
/// file name like `out.png`) rather than a directory. A directory that
/// exists is one, whatever its name.
pub fn is_template(s: &str) -> bool {
    s.contains('%') || s.contains('{') || (has_image_extension(s) && !expand(Path::new(s)).is_dir())
}

/// The most of a `--save` field's text a name takes, in bytes (a name
/// may be 255).
const FIELD_MAX: usize = 100;

/// A `--save` template filled in: strftime fields (at `now`) and `{class}`
/// `{title}` `{monitor}` `{w}` `{h}` `{kind}` `{n}` (the count of files in
/// its directory, plus one). The directory, the file's stem, and its
/// extension if it gave one. One ending in `/` names a directory, and the
/// file in it is named by the time, as without a template.
pub fn render_template(
    template: &str,
    now: &jiff::Zoned,
    naming: &Naming,
) -> Result<(PathBuf, String, Option<String>), String> {
    // A field's text can't make a path or a strftime field of its own,
    // nor a name too long for the file system; an empty one is unknown.
    let clean = |v: &Option<String>| {
        let v = v.as_deref().filter(|v| !v.is_empty()).unwrap_or("unknown");
        let mut end = v.len().min(FIELD_MAX);
        while !v.is_char_boundary(end) {
            end -= 1;
        }
        v[..end].replace(['/', '\0'], "-").replace('%', "%%")
    };
    let mut t = template
        .replace("{class}", &clean(&naming.class))
        .replace("{title}", &clean(&naming.title))
        .replace("{monitor}", &clean(&naming.monitor))
        .replace("{w}", &naming.w.to_string())
        .replace("{h}", &naming.h.to_string())
        .replace("{kind}", &naming.kind.replace('%', "%%"));
    t = jiff::fmt::strtime::format(&t, now).map_err(|e| format!("--save template: {e}"))?;
    if t.ends_with('/') {
        return Ok((expand(Path::new(&t)), now.strftime(STAMP).to_string(), None));
    }
    let path = expand(Path::new(&t));
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("--save template: it needs a file name")?;
    let name = if name.contains("{n}") {
        let count = fs::read_dir(&dir).map_or(0, |d| d.count());
        name.replace("{n}", &(count + 1).to_string())
    } else {
        name
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) => {
            (stem.to_string(), Some(ext.to_string()))
        }
        _ => (name, None),
    };
    Ok((dir, stem, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_templates_fill_in_their_fields() {
        let now: jiff::Zoned = "2026-10-03T14:02:11+01:00[Europe/London]".parse().unwrap();
        let naming = Naming {
            class: Some("firefox".into()),
            title: Some("a/b 100%".into()),
            monitor: Some("DP-3".into()),
            w: 800,
            h: 600,
            kind: "image".into(),
        };
        let (dir, stem, ext) = render_template(
            "/tmp/s/%Y-%m/{monitor}_{class}_{w}x{h}_%H%M.png",
            &now,
            &naming,
        )
        .unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/s/2026-10"));
        assert_eq!(
            (stem.as_str(), ext.as_deref()),
            ("DP-3_firefox_800x600_1402", Some("png"))
        );
        // A directory of its own: the file is named by the time.
        assert_eq!(
            render_template("/tmp/s/%Y-%m/", &now, &naming).unwrap(),
            (
                PathBuf::from("/tmp/s/2026-10/"),
                "2026-10-03_14-02-11".into(),
                None
            )
        );
        // A title's slash and percent are just text; no extension: none given.
        let (_, stem, ext) = render_template("/tmp/{title}", &now, &naming).unwrap();
        assert_eq!((stem.as_str(), ext), ("a-b 100%", None));
        // A long one is cut, on a character; an empty one is unknown.
        let titled = |t: &str| Naming {
            title: Some(t.into()),
            ..naming.clone()
        };
        let long = "あ".repeat(100);
        let (_, stem, _) = render_template("/tmp/{title}.png", &now, &titled(&long)).unwrap();
        assert!(stem.len() <= FIELD_MAX && stem.len() > 90 && long.starts_with(&stem));
        let (_, stem, _) = render_template("/tmp/{title}.png", &now, &titled("")).unwrap();
        assert_eq!(stem, "unknown");
        assert!(is_template("~/s/%Y.png") && is_template("{class}") && !is_template("~/shots"));
        // A plain file name is one too, unless it's a directory already.
        assert!(is_template("out.png") && is_template("/tmp/x/Shot.JPG"));
        assert!(!is_template(".png") && !is_template("shots.d"));
        let dir = std::env::temp_dir().join(format!("screenie-dir-{}.png", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_template(&dir.to_string_lossy()));
        let _ = std::fs::remove_dir(&dir);
        let (d, stem, ext) = render_template("out.png", &now, &naming).unwrap();
        assert_eq!(
            (d, stem.as_str(), ext.as_deref()),
            (PathBuf::from("."), "out", Some("png"))
        );
    }

    #[test]
    fn same_second_shots_get_their_own_names() {
        let d = std::env::temp_dir().join(format!("screenie-test-{}", std::process::id()));
        let (a, _) = create_unique(&d, "stamp", "png", 0o600, None).unwrap();
        let (b, _) = create_unique(&d, "stamp", "png", 0o600, None).unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("stamp-1.png"));
        // A project left by another format's save takes its name too.
        fs::write(d.join("out.screenie"), b"").unwrap();
        let (c, _) = create_unique(&d, "out", "jpg", 0o600, Some("screenie")).unwrap();
        assert!(c.ends_with("out-1.jpg") && !d.join("out.jpg").exists());
        let (e, _) = create_unique(&d, "out", "jpg", 0o600, None).unwrap();
        assert!(e.ends_with("out.jpg"));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn pruning_keeps_the_newest_shots_and_their_projects() {
        let d = std::env::temp_dir().join(format!("screenie-prune-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        for i in 0..4 {
            fs::write(d.join(format!("s{i}.png")), b"").unwrap();
            fs::write(d.join(format!("s{i}.screenie")), b"").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        prune(&d, 2);
        let mut left: Vec<String> = fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["s2.png", "s2.screenie", "s3.png", "s3.screenie"]);
        assert_eq!(shots_in(&d)[0].file_name().unwrap(), "s3.png");
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_last_shot_may_have_no_project() {
        let d = std::env::temp_dir().join(format!("screenie-last-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("a.png"), b"").unwrap();
        fs::write(d.join("a.screenie"), b"").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
        fs::write(d.join("b.png"), b"").unwrap();
        assert_eq!(last_in(&d, 1), Some((d.join("b.png"), None)));
        assert_eq!(
            last_in(&d, 2),
            Some((d.join("a.png"), Some(d.join("a.screenie"))))
        );
        assert_eq!((last_in(&d, 3), last_in(&d, 0)), (None, None));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn the_runtime_dir_is_private() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let tmp = std::env::temp_dir();
        for unset in [None, Some("".into()), Some("run/user".into())] {
            assert_eq!(runtime_base(unset, 1000), tmp.join("screenie-1000"));
        }
        assert_eq!(
            runtime_base(Some("/run/user/1000".into()), 1000),
            PathBuf::from("/run/user/1000/screenie")
        );
        let d = tmp.join(format!("screenie-private-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let uid = fs::metadata(&d).unwrap().uid();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private(&d, uid).unwrap();
        assert_eq!(fs::metadata(&d).unwrap().mode() & 0o777, 0o700);
        assert!(ensure_private(&d, uid + 1).is_err(), "someone else's");
        let link = tmp.join(format!("screenie-link-{}", std::process::id()));
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&d, &link).unwrap();
        assert!(ensure_private(&link, uid).is_err(), "a symlink");
        fs::remove_file(link).unwrap();
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn tilde_is_home() {
        assert_eq!(expand(Path::new("~/x")), home().join("x"));
        assert_eq!(expand(Path::new("/abs")), PathBuf::from("/abs"));
    }
}
