//! The binary on its headless paths (`shot --from`, `history`): no picker,
//! and no display to reach. Each run gets its own runtime, state, config
//! and pictures directories, so nothing of the user's is read or written.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use screenie_core::geometry::{LogicalPoint, LogicalRect, PixelPoint};
use screenie_core::layout::{Layout, Monitor, MonitorId};
use screenie_core::model::{
    CalloutOp, Layer, LayerId, Op, OpData, OpId, Project, ProjectFile, TextOp,
};
use screenie_image::{Image, Rgb, encode};

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// screenie with `args`, in `dir`'s directories, with no display.
fn screenie(dir: &Path, args: &[&str]) -> Output {
    let sub = |s: &str| {
        let p = dir.join(s);
        std::fs::create_dir_all(&p).unwrap();
        p
    };
    Command::new(env!("CARGO_BIN_EXE_screenie"))
        .args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env("XDG_RUNTIME_DIR", sub("run"))
        .env("XDG_STATE_HOME", sub("state"))
        .env("XDG_CONFIG_HOME", sub("config"))
        .env("XDG_PICTURES_DIR", sub("pictures"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// What every image run here passes: nothing copied, notified or kept.
const QUIET: [&str; 4] = ["--no-config", "--no-copy", "--no-notify", "--no-history"];

fn shot(dir: &Path, rest: &[&str]) -> Output {
    let mut args = vec!["shot"];
    args.extend(rest);
    args.extend(QUIET);
    screenie(dir, &args)
}

fn write_png(dir: &Path, name: &str, img: &Image) -> String {
    let p = dir.join(name);
    std::fs::write(&p, encode::png(img, 1).unwrap()).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn a_region_of_an_image_to_stdout() {
    let dir = scratch("region");
    let file = write_png(&dir, "in.png", &Image::filled(64, 48, Rgb(10, 20, 30)));
    let out = shot(&dir, &["--from", &file, "10,10 20x20", "--stdout"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(encode::png_size(&out.stdout), Some((20, 20)));
}

#[test]
fn errors_are_exit_codes_and_one_json_line() {
    let dir = scratch("errors");
    assert_eq!(shot(&dir, &["--no-such-flag"]).status.code(), Some(2));
    let out = shot(&dir, &["--from", "/nonexistent/x.png", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    let line = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(
        (v["status"].as_str(), v["code"].as_u64()),
        (Some("error"), Some(3))
    );
    assert_eq!(line.lines().count(), 1);
}

/// A 100x40 white capture on one monitor, with a text layer and a callout.
fn annotated() -> (Image, ProjectFile) {
    let mon = Monitor {
        id: MonitorId(0),
        name: "A".into(),
        rect: LogicalRect::new(0.0, 0.0, 100.0, 40.0),
        scale: 1.0,
    };
    let mut project = Project::new(mon.rect);
    let text = OpData::Text(TextOp {
        origin: LogicalPoint::new(4.0, 4.0),
        text: "WWW".into(),
        rgb: [0.0; 3],
        size: 20.0,
        chip: false,
    });
    let callout = OpData::Callout(CalloutOp {
        centre: LogicalPoint::new(80.0, 20.0),
        n: 8,
        rgb: [1.0, 0.0, 0.0],
        size: 30.0,
    });
    project.layers.push(Layer {
        id: LayerId(1),
        clip: mon.rect,
        hidden: false,
        ops: vec![
            Op {
                id: OpId(2),
                data: text,
            },
            Op {
                id: OpId(3),
                data: callout,
            },
        ],
    });
    let layout = Layout::new(vec![mon], vec![], MonitorId(0));
    let white = Image::filled(100, 40, Rgb(255, 255, 255));
    (white, ProjectFile::new(project, layout))
}

fn project_png(dir: &Path, img: &Image, json: &str) -> String {
    let p = dir.join("project.png");
    let png = encode::png_with_text(img, 1, Some(("screenie-project", json))).unwrap();
    std::fs::write(&p, png).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn a_project_is_delivered_with_its_text_drawn_in() {
    if screenie_platform::fonts::monospace().is_none() {
        return;
    }
    let dir = scratch("project");
    let (img, file) = annotated();
    let path = project_png(&dir, &img, &file.to_json());
    let out = shot(&dir, &["--from", &path, "--stdout"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let got = screenie_image::decode::decode(&out.stdout, Rgb(0, 0, 0)).unwrap();
    let differ = |x0, x1, than| {
        (x0..x1).any(|x| (4..36).any(|y| got.rgb(PixelPoint::new(x, y)) != Some(than)))
    };
    assert!(differ(4, 50, Rgb(255, 255, 255)), "the text");
    // Inside the disc (radius 15 round 80,20), the number's white ink.
    let inked = (74..87).any(|x| {
        (12..28).any(|y| {
            matches!(got.rgb(PixelPoint::new(x, y)),
                Some(Rgb(r, g, b)) if r > 200 && g > 128 && b > 128)
        })
    });
    assert!(inked, "the callout's number");
}

#[test]
fn a_project_from_a_newer_screenie_is_refused() {
    let dir = scratch("newer");
    let (img, file) = annotated();
    let mut json: serde_json::Value = serde_json::from_str(&file.to_json()).unwrap();
    json["project"]["version"] = (screenie_core::model::VERSION + 1).into();
    let path = project_png(&dir, &img, &json.to_string());
    let out = shot(&dir, &["--from", &path, "--stdout"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty(), "never the bare capture");
}

#[test]
fn the_last_shot_comes_back_without_a_project() {
    let dir = scratch("last");
    let file = write_png(&dir, "in.png", &Image::filled(30, 20, Rgb(1, 2, 3)));
    // Kept in history (an opened image keeps no project), then again.
    let args = [
        "shot",
        "--from",
        &file,
        "--no-config",
        "--no-copy",
        "--no-notify",
    ];
    assert_eq!(screenie(&dir, &args).status.code(), Some(0));
    let history = screenie(&dir, &["history", "--json"]);
    let line = String::from_utf8(history.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(
        (v["w"].as_u64(), v.get("project").is_none()),
        (Some(30), true)
    );
    let out = shot(&dir, &["--from", "last", "--stdout"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(encode::png_size(&out.stdout), Some((30, 20)));
}

#[test]
fn the_last_shot_delivered_again_keeps_its_project() {
    use std::os::unix::fs::DirBuilderExt;
    let dir = scratch("last-project");
    // A past screen shot, with its project, in history.
    let shots = dir.join("run/screenie/shots");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&shots)
        .unwrap();
    let (img, mut file) = annotated();
    file.project.selection = LogicalRect::new(10.0, 5.0, 60.0, 30.0);
    let shot_png = write_png(&shots, "2000-01-01_00-00-00.png", &img);
    project_png(&dir, &img, &file.to_json());
    let project = shots.join("2000-01-01_00-00-00.screenie");
    std::fs::rename(dir.join("project.png"), &project).unwrap();
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for f in [Path::new(&shot_png), &project] {
        std::fs::File::options()
            .write(true)
            .open(f)
            .unwrap()
            .set_modified(past)
            .unwrap();
    }
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let out_arg = out_dir.to_string_lossy().into_owned();
    let args = [
        "shot",
        "--from",
        "last",
        "--save",
        &out_arg,
        "--save-project",
        "--json",
        "--no-config",
        "--no-copy",
        "--no-notify",
    ];
    let out = screenie(&dir, &args);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let saved = v["project"].as_str().expect("a project is reported");
    assert!(Path::new(saved).starts_with(&out_dir), "{saved}");
    assert!(Path::new(saved).is_file());
    // The new history entry is editable too.
    let history = screenie(&dir, &["history", "--json"]);
    let text = String::from_utf8(history.stdout).unwrap();
    let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert!(first["project"].is_string(), "{text}");
    // And it reopens with its layers, over the box it delivered.
    let again = shot(&dir, &["--from", "last", "--stdout"]);
    assert_eq!(again.status.code(), Some(0), "{again:?}");
    assert_eq!(encode::png_size(&again.stdout), Some((60, 30)));
}

#[test]
fn an_empty_history_prints_nothing() {
    let dir = scratch("history");
    let out = screenie(&dir, &["history", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}
