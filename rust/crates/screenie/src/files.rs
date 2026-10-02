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
pub fn runtime_dir() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let d = base.join("screenie");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&d)?;
    Ok(d)
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
pub fn create_unique(
    dir: &Path,
    stamp: &str,
    ext: &str,
    mode: u32,
) -> io::Result<(PathBuf, fs::File)> {
    fs::create_dir_all(dir)?;
    for n in 0u32.. {
        let name = if n == 0 {
            format!("{stamp}.{ext}")
        } else {
            format!("{stamp}-{n}.{ext}")
        };
        let path = dir.join(name);
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

/// Keep the newest `keep` files in `dir`.
pub fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = fs::remove_file(p);
    }
}

/// The local time as a file name stamp, as the prototype names shots.
pub fn stamp() -> String {
    jiff::Zoned::now().strftime("%Y-%m-%d_%H-%M-%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_second_shots_get_their_own_names() {
        let d = std::env::temp_dir().join(format!("screenie-test-{}", std::process::id()));
        let (a, _) = create_unique(&d, "stamp", "png", 0o600).unwrap();
        let (b, _) = create_unique(&d, "stamp", "png", 0o600).unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("stamp-1.png"));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn tilde_is_home() {
        assert_eq!(expand(Path::new("~/x")), home().join("x"));
        assert_eq!(expand(Path::new("/abs")), PathBuf::from("/abs"));
    }
}
