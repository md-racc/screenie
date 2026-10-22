//! screenie: keyboard-driven screenshots.

mod analysis;
mod cli;
mod config;
mod deliver;
mod files;
mod help;
mod open;
mod picker;
mod region;
mod report;

use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;
use screenie_core::model::{Project, ProjectFile};
use screenie_core::session::{FinishedAction, Outcome, Session, Setup};
use screenie_image::ui::Ui;
use screenie_image::{Frame, Picture};
use screenie_platform::desktop::DesktopInfo;
use screenie_platform::desktop::hyprland::{HyprClient, Hyprland};
use screenie_platform::notify::notify;
use screenie_platform::wayland::Wayland;

use cli::Command;
use report::Report;
use serde_json::json;

/// Exit statuses (docs/cli.md, EXIT STATUS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    Done = 0,
    Cancelled = 1,
    Usage = 2,
    Capture = 3,
    Delivery = 4,
}

struct Failure(Exit, String);

impl Failure {
    fn usage(s: impl Into<String>) -> Self {
        Failure(Exit::Usage, s.into())
    }
    fn capture(s: impl std::fmt::Display) -> Self {
        Failure(Exit::Capture, s.to_string())
    }
    fn delivery(s: impl Into<String>) -> Self {
        Failure(Exit::Delivery, s.into())
    }
}

/// The frozen frame and the layout it was taken on. Capture starts before
/// the desktop query, so the picture is of the moment screenie started.
fn capture(out: &cli::Output) -> Result<(Wayland, Layout, Frame), Failure> {
    wait(out);
    let mut wl = Wayland::connect().map_err(Failure::capture)?;
    wl.set_cursor(out.cursor);
    wl.start_capture().map_err(Failure::capture)?;
    let layout = Hyprland.layout().map_err(Failure::capture)?;
    let img = wl.finish_capture(&layout).map_err(Failure::capture)?;
    let frame = Frame::new(img, layout.bounds);
    Ok((wl, layout, frame))
}

/// `--delay`: wait before capturing, saying so (anything on screen would
/// be in the capture, so there's no countdown to look at, and the
/// notification closes before the capture).
fn wait(out: &cli::Output) {
    let Some(secs) = out.delay.filter(|&s| s > 0.0) else {
        return;
    };
    let wait = Duration::from_secs_f64(secs);
    if deliver::notifies(out) {
        let body = format!("Capturing in {secs}s");
        screenie_platform::notify::notify_during("screenie", &body, wait);
    } else {
        std::thread::sleep(wait);
    }
}

/// A REGION as written, where `-` means one read from stdin
/// (`slurp | screenie shot -`).
fn region_spec(spec: &str) -> Result<String, Failure> {
    if spec.trim() != "-" {
        return Ok(spec.to_owned());
    }
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| Failure::usage(format!("reading the region from stdin: {e}")))?;
    Ok(line)
}

/// A REGION on a layout: a malformed one is exit 2, one with nothing
/// there exit 3.
fn resolve_region(
    spec: &str,
    layout: &Layout,
    ring: &[LogicalRect],
) -> Result<LogicalRect, Failure> {
    region::resolve(&region_spec(spec)?, layout, ring).map_err(unresolved)
}

/// A region over an image (`open::region`), as `resolve_region` fails.
fn image_region(spec: &str, image: LogicalRect, k: f64) -> Result<LogicalRect, Failure> {
    open::region(&region_spec(spec)?, image, k).map_err(unresolved)
}

/// A region that didn't resolve: a usage error, or nothing to capture.
fn unresolved(e: region::Unresolved) -> Failure {
    match e {
        region::Unresolved::Bad(e) => Failure::usage(e),
        region::Unresolved::Missing(e) => Failure::capture(e),
    }
}

fn ring_on(layout: &Layout) -> Vec<LogicalRect> {
    files::load_ring().resolve(&layout.monitors)
}

/// `shot active`: the focused window from its own pixels, even if it's
/// covered.
fn shot_active(out: &cli::Output) -> Result<Report, Failure> {
    wait(out); // focus may move to another window meanwhile
    let active = Hyprland
        .active_client()
        .map_err(Failure::capture)?
        .ok_or_else(|| Failure::capture("no active window"))?;
    shot_window(&active, out)
}

/// A window by `window:SPEC` (see `find_window`): no match is exit 3, a bad
/// spec exit 2.
fn window_by(spec: &str) -> Result<HyprClient, Failure> {
    let spec = spec.strip_prefix("window:").unwrap_or(spec);
    let clients = Hyprland.clients().map_err(Failure::capture)?;
    screenie_platform::desktop::hyprland::find_window(&clients, spec)
        .map_err(Failure::usage)?
        .cloned()
        .ok_or_else(|| Failure::capture(format!("no window matches window:{spec}")))
}

/// A window's own pixels (covered or on another workspace, still whole).
fn shot_window(window: &HyprClient, out: &cli::Output) -> Result<Report, Failure> {
    let mut wl = Wayland::connect().map_err(Failure::capture)?;
    wl.set_cursor(out.cursor);
    if window.stable_id.is_empty() {
        return Err(Failure::capture("Hyprland gave the window no stable id"));
    }
    let img = wl
        .capture_window(&window.stable_id)
        .map_err(Failure::capture)?;
    drop(wl);
    let what = format!("{} window", window.class);
    // The window on its own: one monitor its size, for auto-redact.
    let monitor = screenie_core::layout::Monitor {
        name: what.clone(),
        ..open::its_own(&img)
    };
    let (rect, layout) = (monitor.rect, open::one_monitor(&monitor));
    let (picture, _, redacted) = redact_shot(&Frame::new(img, rect), &layout, rect, out)?;
    let placed = placed(window);
    let img = deliver::at_scale(picture.crop(rect), placed, &[], out.scale);
    let mut names = naming(&img, None, None);
    (names.class, names.title) = (Some(window.class.clone()), Some(window.title.clone()));
    let d = deliver::image(&img, out, &what, None, &names).map_err(Failure::delivery)?;
    if out.pin {
        float(&img, placed, out.png_level)?;
    }
    Ok(Report::done("image")
        .redacted(out, &redacted)
        .size(&img)
        .set(
            "window",
            json!({"class": window.class, "title": window.title, "address": window.address}),
        )
        .delivered(&d))
}

/// Where Hyprland places a window, in logical pixels.
fn placed(w: &HyprClient) -> LogicalRect {
    LogicalRect::new(w.at[0], w.at[1], w.size[0], w.size[1])
}

/// `shot --redact auto`: the shot with the ticked suggestions censored,
/// its project, and what was censored. Without it, the shot as captured.
fn redact_shot(
    frame: &Frame,
    layout: &Layout,
    rect: LogicalRect,
    out: &cli::Output,
) -> Result<RedactedShot, Failure> {
    let mut picture = Picture::new(frame.clone());
    if out.redact != Some(cli::Redact::Auto) {
        return Ok((picture, Project::new(rect), Vec::new()));
    }
    let mut s = Session::new(Setup {
        layout: layout.clone(),
        pixels: *frame.map(),
        start: Some(rect),
        recall: vec![],
    });
    s.set_redact_rules(redact_rules());
    s.set_analyzer(Box::new(analysis::Ocr::inline(
        Picture::new(frame.clone()),
        out.ocr_lang(),
    )));
    s.start_redact(true).map_err(redact_failed)?;
    picture.sync(s.layers(), None, None);
    let mut project = s.project();
    project.selection = rect;
    let redacted = s
        .redacted()
        .into_iter()
        .map(|(w, r)| (w.to_string(), r))
        .collect();
    Ok((picture, project, redacted))
}

/// Auto-redact couldn't run (OCR failed, no tesseract): exit 4, like the
/// OCR result.
fn redact_failed(e: impl std::fmt::Display) -> Failure {
    Failure::delivery(format!("auto-redact: {e}"))
}

type RedactedShot = (Picture, Project, Vec<(String, LogicalRect)>);

/// `recent`: the clipboard's image if it holds one, else the last shot.
fn resolve(source: cli::Source) -> cli::Source {
    match source {
        cli::Source::Recent if screenie_platform::clipboard::holds_image() => {
            cli::Source::Clipboard
        }
        cli::Source::Recent => cli::Source::Last(1),
        other => other,
    }
}

fn shot(args: cli::Shot) -> Result<Report, Failure> {
    match args.from.clone().map(resolve) {
        Some(cli::Source::Last(n)) => return shot_last(n, &args),
        Some(source) if source.is_image() => return shot_image(&source, &args),
        _ => {}
    }
    let spec = args.region.as_deref().unwrap_or("active");
    if spec == "active" {
        return shot_active(&args.output);
    }
    if spec.starts_with("window:") {
        wait(&args.output);
        return shot_window(&window_by(spec)?, &args.output);
    }
    let (_wl, layout, frame) = capture(&args.output)?;
    let rect = resolve_region(spec, &layout, &ring_on(&layout))?;
    capturable(rect, &frame, &layout)?;
    let (picture, project, redacted) = redact_shot(&frame, &layout, rect, &args.output)?;
    let keep = || open::project_png(frame.image(), &project, &layout);
    let img = deliver::at_scale(
        picture.crop(rect),
        rect,
        &layout.monitors,
        args.output.scale,
    );
    let names = naming(&img, Some(&layout), Some(rect));
    let d = deliver::image(&img, &args.output, "Area", Some(&keep), &names)
        .map_err(Failure::delivery)?;
    if args.output.pin {
        float(&img, rect, args.output.png_level)?;
    }
    Ok(Report::done("image")
        .rect(rect, &layout)
        .size(&img)
        .redacted(&args.output, &redacted)
        .delivered(&d))
}

/// A box with nothing to capture (no pixels, or on no monitor) is exit 3.
fn capturable(rect: LogicalRect, frame: &Frame, layout: &Layout) -> Result<(), Failure> {
    if frame.map().to_pixels(rect).is_empty() {
        return Err(Failure::capture("the region is empty"));
    }
    if !region::on_screen(rect, layout) {
        return Err(Failure::capture("the region is outside every monitor"));
    }
    Ok(())
}

/// `shot --from FILE|-`: a region of an image (all of it by default),
/// delivered as it is; a project PNG with its layers drawn in.
fn shot_image(source: &cli::Source, args: &cli::Shot) -> Result<Report, Failure> {
    let bytes = open::read(source).map_err(Failure::capture)?;
    let img = open::image(&bytes).map_err(Failure::capture)?;
    let monitor = open::its_own(&img);
    let (frame, image) = screenie_image::compose::place(img, monitor.rect, 1.0, open::BACKGROUND);
    let layout = open::one_monitor(&monitor);
    let rect = image_region(args.region.as_deref().unwrap_or("all"), image, 1.0)?;
    let (picture, _, redacted) = redact_shot(&frame, &layout, rect, &args.output)?;
    let img = deliver::at_scale(picture.crop(rect), rect, &[], args.output.scale);
    let d = deliver::image(&img, &args.output, "Image", None, &naming(&img, None, None))
        .map_err(Failure::delivery)?;
    if args.output.pin {
        float(&img, middle_of_focused(&img), args.output.png_level)?;
    }
    // Not .rect(): the image's own "monitor" isn't one to report.
    Ok(Report::done("image")
        .set("rect", report::rect_json(rect))
        .size(&img)
        .redacted(&args.output, &redacted)
        .delivered(&d))
}

/// `shot --from last[:N]`: a past shot delivered again, its layers drawn
/// in, over its own box unless a region says otherwise. It keeps its
/// project (over the box delivered), in history and with `--save-project`.
fn shot_last(which: usize, args: &cli::Shot) -> Result<Report, Failure> {
    let (img, file) = match last_shot(which)? {
        Last::Project(img, file) => (img, *file),
        Last::Plain(shot) => return shot_image(&cli::Source::File(shot), args),
    };
    let layout = file.layout.clone();
    let rect = match args.region.as_deref() {
        Some(spec) => resolve_region(spec, &layout, &ring_on(&layout))?,
        None => file.project.selection,
    };
    // The project goes with it again, so `last` stays editable.
    let base = img.clone();
    let mut project = file.project.clone();
    project.selection = rect;
    let keep = || open::project_png(&base, &project, &layout);
    let frame = open::flattened(img, &file);
    capturable(rect, &frame, &layout)?;
    let img = deliver::at_scale(frame.crop(rect), rect, &layout.monitors, args.output.scale);
    let names = naming(&img, Some(&layout), Some(rect));
    let d = deliver::image(&img, &args.output, "Area", Some(&keep), &names)
        .map_err(Failure::delivery)?;
    if args.output.pin {
        float(&img, rect, args.output.png_level)?;
    }
    Ok(Report::done("image")
        .rect(rect, &layout)
        .size(&img)
        .delivered(&d))
}

/// Float an image (a pin, or `--pin`) at `rect`, on screen.
fn float(
    img: &screenie_image::Image,
    rect: LogicalRect,
    png_level: u8,
) -> Result<(u32, std::path::PathBuf), Failure> {
    let png = screenie_image::encode::png(img, png_level)
        .map_err(|e| Failure::delivery(e.to_string()))?;
    let dir = files::runtime_dir().map_err(|e| Failure::delivery(e.to_string()))?;
    let (path, mut f) =
        files::create_unique(&dir, &format!("pin-{}", files::stamp()), "png", 0o600, None)
            .map_err(|e| Failure::delivery(e.to_string()))?;
    std::io::Write::write_all(&mut f, &png).map_err(|e| Failure::delivery(e.to_string()))?;
    let exe = std::env::current_exe().map_err(|e| Failure::delivery(e.to_string()))?;
    let scale = f64::from(img.width()) / rect.w;
    let id = screenie_platform::pins::pin(
        &exe,
        &pins_dir()?,
        &path,
        [img.width(), img.height()],
        scale,
        Some(rect),
    )
    .map_err(Failure::delivery)?;
    Ok((id, path))
}

/// Where the pins' records are.
fn pins_dir() -> Result<std::path::PathBuf, Failure> {
    Ok(files::runtime_dir()
        .map_err(|e| Failure::delivery(e.to_string()))?
        .join("pins"))
}

/// `pin [FILE|-|clipboard|recent|last[:N]] [--at X,Y]`: float an image at `--at`, else
/// where it came from if screenie took it, else the focused monitor's
/// middle.
fn pin_cmd(source: &cli::Source, at: Option<(f64, f64)>) -> Result<Report, Failure> {
    let mut source = resolve(source.clone());
    let mut last = None;
    if let cli::Source::Last(n) = source {
        match last_shot(n)? {
            Last::Project(img, file) => last = Some((img, *file)),
            Last::Plain(shot) => source = cli::Source::File(shot),
        }
    }
    let (img, from) = match last {
        Some((img, file)) => {
            let rect = file.project.selection;
            (open::flattened(img, &file).crop(rect), Some(rect))
        }
        None => {
            let bytes = open::read(&source).map_err(Failure::capture)?;
            (open::image(&bytes).map_err(Failure::capture)?, None)
        }
    };
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let rect = match (at, from) {
        (Some((x, y)), Some(r)) => LogicalRect::new(x, y, r.w, r.h),
        (Some((x, y)), None) => LogicalRect::new(x, y, w, h),
        (None, Some(r)) => r,
        (None, None) => middle_of_focused(&img),
    };
    let (id, path) = float(&img, rect, cli::PNG_LEVEL)?;
    Ok(Report::done("pin")
        .set("id", id)
        .set("path", path.display().to_string())
        .set("text", id.to_string()))
}

/// `__serve-pin ID PATH SCALE [X,Y]`: a pin's window, until it's closed.
fn serve_pin(args: &[String]) -> ExitCode {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let parsed = (|| {
        let id: u32 = args.first()?.parse().ok()?;
        let path = std::path::PathBuf::from(args.get(1)?);
        let scale: f64 = args.get(2)?.parse().ok()?;
        let at = args.get(3).and_then(|a| {
            let (x, y) = a.split_once(',')?;
            Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?))
        });
        Some((id, path, scale, at))
    })();
    let Some((id, path, scale, at)) = parsed else {
        return ExitCode::from(Exit::Usage as u8);
    };
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(sig, stop.clone());
    }
    let shown = || -> Result<(), String> {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let img = screenie_image::decode::decode(&bytes, open::BACKGROUND)?;
        let mut wl = Wayland::connect().map_err(|e| e.to_string())?;
        let pid = std::process::id();
        wl.run_pin(
            img,
            scale.round() as i32,
            &format!("screenie pin {id}"),
            &stop,
            || {
                // The rule floats and pins it; this puts it where it came
                // from, once Hyprland has the window (a moment after it's
                // drawn).
                if let Some((x, y)) = at {
                    std::thread::spawn(move || {
                        use screenie_platform::desktop::hyprland;
                        for _ in 0..40 {
                            let known = Hyprland
                                .clients()
                                .is_ok_and(|cs| cs.iter().any(|c| c.pid == i64::from(pid)));
                            if known {
                                let _ =
                                    hyprland::move_window(pid, x.round() as i32, y.round() as i32);
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    });
                }
            },
        )
        .map_err(|e| e.to_string())
    };
    let result = shown();
    if let Ok(dir) = pins_dir() {
        screenie_platform::pins::forget(&dir, id, &path);
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(Exit::Delivery as u8),
    }
}

/// `pin list`.
fn pin_list(json: bool) -> Result<Report, Failure> {
    for p in screenie_platform::pins::list(&pins_dir()?) {
        if json {
            say(json!({"id": p.id, "size": p.size, "at": p.at, "path": p.path}));
        } else {
            let at = p.at.map_or(String::new(), |[x, y]| format!("  at {x},{y}"));
            say(format!(
                "{}  {}x{}{at}  {}",
                p.id,
                p.size[0],
                p.size[1],
                p.path.display()
            ));
        }
    }
    Ok(Report::done("pins"))
}

/// `pin close ID...|--all`.
fn pin_close(ids: &[u32]) -> Result<Report, Failure> {
    let dir = pins_dir()?;
    let closed = screenie_platform::pins::close(&dir, (!ids.is_empty()).then_some(ids));
    let missing: Vec<String> = ids
        .iter()
        .filter(|i| !closed.contains(i))
        .map(u32::to_string)
        .collect();
    if !missing.is_empty() {
        return Err(Failure::usage(format!("no pin {}", missing.join(", "))));
    }
    Ok(Report::done("pins").set("closed", closed))
}

/// Where an image that isn't on screen floats: the focused monitor's
/// middle, at its size.
fn middle_of_focused(img: &screenie_image::Image) -> LogicalRect {
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let at = Hyprland
        .layout()
        .ok()
        .and_then(|l| l.monitor(l.focused).map(|m| m.rect));
    match at {
        Some(m) => LogicalRect::new(m.x + (m.w - w) / 2.0, m.y + (m.h - h) / 2.0, w, h),
        None => LogicalRect::new(0.0, 0.0, w, h),
    }
}

/// What `--save` template fields are for an image (the monitor its box is
/// on, if it's of the screen).
fn naming(
    img: &screenie_image::Image,
    layout: Option<&Layout>,
    rect: Option<LogicalRect>,
) -> files::Naming {
    files::Naming {
        monitor: layout
            .zip(rect)
            .and_then(|(l, r)| l.monitor_at(r.centre()).map(|m| m.name.clone())),
        w: img.width(),
        h: img.height(),
        kind: "image".into(),
        ..files::Naming::default()
    }
}

/// A safety net for development runs: `SCREENIE_DEADLINE=SECS` takes the
/// overlays down after that long, whatever happens.
fn deadline() -> Option<Duration> {
    parse_deadline(&std::env::var("SCREENIE_DEADLINE").ok()?)
}

/// Seconds as a duration; anything else (negative, NaN, too big) is none.
fn parse_deadline(s: &str) -> Option<Duration> {
    Duration::try_from_secs_f64(s.trim().parse().ok()?).ok()
}

fn pick(args: cli::Pick) -> Result<Report, Failure> {
    match args.from.clone().map(resolve) {
        Some(cli::Source::Last(n)) => return edit_last(n, args),
        Some(source) if source.is_image() => return pick_image(&source, args),
        _ => {}
    }
    let (wl, layout, frame) = capture(&args.output)?;
    let ring = ring_on(&layout);
    let start = match &args.region {
        // The box on a window, wherever it is (as Hyprland places it).
        Some(spec) if spec.starts_with("window:") => Some(placed(&window_by(spec)?)),
        Some(spec) => Some(resolve_region(spec, &layout, &ring)?),
        None => None,
    };
    let session = Session::new(Setup {
        layout: layout.clone(),
        pixels: *frame.map(),
        start,
        recall: ring,
    });
    run_picker(wl, layout, frame, session, &args, true)
}

/// `--keys-file FILE|-`: its keys, as if given with `--keys`.
fn keys_from_file(mut args: cli::Pick) -> Result<cli::Pick, Failure> {
    let Some(path) = args.keys_file.take() else {
        return Ok(args);
    };
    let keys = if path == "-" {
        std::io::read_to_string(std::io::stdin())
    } else {
        std::fs::read_to_string(&path)
    };
    args.keys = Some(keys.map_err(|e| Failure::usage(format!("--keys-file {path}: {e}")))?);
    Ok(args)
}

/// `edit [FILE|-|clipboard|recent|last[:N]]`: `pick --from SOURCE`, the
/// box on all of it; with none, the most recent image.
fn edit(source: cli::Source, args: cli::Pick) -> Result<Report, Failure> {
    match resolve(source) {
        cli::Source::Last(n) => edit_last(n, args),
        source => pick_image(&source, args),
    }
}

/// A past shot (`last:N`).
enum Last {
    /// Its capture, and the project with its layout.
    Project(screenie_image::Image, Box<ProjectFile>),
    /// A shot that kept no project (a window's, an opened image's): its
    /// PNG, which reopens as any image does.
    Plain(std::path::PathBuf),
}

fn last_shot(which: usize) -> Result<Last, Failure> {
    let (shot, project) = files::runtime_dir()
        .ok()
        .and_then(|d| files::last_in(&d.join("shots"), which))
        .ok_or_else(|| Failure::capture(format!("there's no shot last:{which}")))?;
    let Some(path) = project else {
        return Ok(Last::Plain(shot));
    };
    let bytes =
        std::fs::read(&path).map_err(|e| Failure::capture(format!("{}: {e}", path.display())))?;
    open::project_in(&bytes)
        .map_err(|e| Failure::capture(format!("{}: {e}", path.display())))?
        .map(|(img, file)| Last::Project(img, Box::new(file)))
        .ok_or_else(|| Failure::capture(format!("{}: no project in it", path.display())))
}

/// Whether two layouts have the same monitors (a project reopens editable
/// only on the monitors it was taken on).
fn same_monitors(a: &Layout, b: &Layout) -> bool {
    let key = |l: &Layout| {
        l.monitors
            .iter()
            .map(|m| (m.name.clone(), m.rect, m.scale))
            .collect::<Vec<_>>()
    };
    key(a) == key(b)
}

/// `edit last[:N]` (`--from last[:N]`): the picker over a past shot's
/// capture, its layers still editable on the monitors it was taken on;
/// on others, it opens with them drawn in, as a project PNG does.
fn edit_last(which: usize, args: cli::Pick) -> Result<Report, Failure> {
    let (img, file) = match last_shot(which)? {
        Last::Project(img, file) => (img, *file),
        Last::Plain(shot) => return pick_image(&cli::Source::File(shot), args),
    };
    let wl = Wayland::connect().map_err(Failure::capture)?;
    let now = Hyprland.layout().map_err(Failure::capture)?;
    if !same_monitors(&now, &file.layout) {
        return pick_flat(wl, &now, open::flatten(img, &file), args);
    }
    edit_project(wl, img, file, args)
}

/// The picker over a project's capture, its layers restored.
fn edit_project(
    wl: Wayland,
    img: screenie_image::Image,
    file: ProjectFile,
    args: cli::Pick,
) -> Result<Report, Failure> {
    let layout = file.layout;
    let frame = Frame::new(img, layout.bounds);
    let ring = ring_on(&layout);
    let start = match &args.region {
        Some(spec) => Some(resolve_region(spec, &layout, &ring)?),
        None => Some(file.project.selection),
    };
    let mut session = Session::new(Setup {
        layout: layout.clone(),
        pixels: *frame.map(),
        start,
        recall: ring,
    });
    let selection = session.rect();
    let mut project = file.project;
    project.selection = selection;
    session.restore(project);
    run_picker(wl, layout, frame, session, &args, true)
}

/// `--from FILE|-` (`edit FILE|-`): the picker over an image, on the
/// focused monitor, the box on all of it unless `--region` says. A
/// screenie project PNG reopens with its layers when its monitors are
/// still there, else with them drawn in.
fn pick_image(source: &cli::Source, args: cli::Pick) -> Result<Report, Failure> {
    let bytes = open::read(source).map_err(Failure::capture)?;
    let wl = Wayland::connect().map_err(Failure::capture)?;
    let now = Hyprland.layout().map_err(Failure::capture)?;
    let img = match open::project_in(&bytes).map_err(Failure::capture)? {
        Some((img, file)) if same_monitors(&now, &file.layout) => {
            return edit_project(wl, img, file, args);
        }
        Some((img, file)) => open::flatten(img, &file),
        None => {
            screenie_image::decode::decode(&bytes, open::BACKGROUND).map_err(Failure::capture)?
        }
    };
    pick_flat(wl, &now, img, args)
}

/// The picker over a flat image, on the focused monitor, the box on all of
/// it unless `--region` says (in the image's pixels).
fn pick_flat(
    wl: Wayland,
    now: &Layout,
    img: screenie_image::Image,
    args: cli::Pick,
) -> Result<Report, Failure> {
    let monitor = now
        .monitor(now.focused)
        .or(now.monitors.first())
        .ok_or_else(|| Failure::capture("no monitor to show it on"))?
        .clone();
    let (frame, image) =
        screenie_image::compose::place(img, monitor.rect, monitor.scale, open::BACKGROUND);
    let layout = open::one_monitor(&monitor);
    let k = frame.map().scale();
    let start = match &args.region {
        Some(spec) => image_region(spec, image, k)?,
        None => image,
    };
    let session = Session::new(Setup {
        layout: layout.clone(),
        pixels: *frame.map(),
        start: Some(start),
        recall: vec![],
    });
    run_picker(wl, layout, frame, session, &args, false)
}

/// Play the keys, show the picker if they didn't finish it, and deliver.
/// `screen`: from the screen or a past shot, so the box joins the recall
/// ring and the shot keeps a project; not for an opened image.
fn run_picker(
    mut wl: Wayland,
    layout: Layout,
    frame: Frame,
    mut session: Session,
    args: &cli::Pick,
    screen: bool,
) -> Result<Report, Failure> {
    let font = open::font();
    session.set_sampler(Box::new(screenie_image::picture::PictureSampler::new(
        frame.clone(),
    )));
    session.set_code_reader(Box::new(picker::Codes(Picture::new(frame.clone()))));
    session.set_redact_rules(redact_rules());
    session.set_edge_finder(Box::new(screenie_image::picture::PictureEdges::new(
        frame.clone(),
    )));
    if let Some(scale) = args.measure_scale {
        session.set_measure_scale(scale);
    }
    if let Some(r) = args.result {
        session.set_result(r);
    }
    if args.lock_box {
        session.lock_box();
    }
    if let Some(mode) = args.mode {
        session.start_in(mode);
    }
    let lang = args.output.ocr_lang();
    // Scripted keys wait for each scan; the overlay doesn't.
    session.set_analyzer(Box::new(analysis::Ocr::inline(
        Picture::new(frame.clone()),
        lang.clone(),
    )));
    let redact_now = redact_inline(args);
    if let Some(how) = redact_now {
        session
            .start_redact(how == cli::Redact::Auto)
            .map_err(redact_failed)?;
    }
    if let Some(keys) = &args.keys {
        session
            .play(keys, 0.0)
            .map_err(|e| Failure::usage(e.to_string()))?;
    }
    if !session.is_finished() {
        if args.no_ui {
            return Err(Failure::usage("--no-ui: the keys didn't finish the picker"));
        }
        let font = font.ok_or_else(|| Failure::capture("no monospace font found"))?;
        let ui = Ui::new(font).map_err(Failure::capture)?;
        let (mut ocr, jobs) = analysis::Ocr::threaded(Picture::new(frame.clone()), lang);
        let rules = redact_rules();
        if rules.prescan {
            let monitors: Vec<_> = layout
                .monitors
                .iter()
                .map(|m| frame.map().to_pixels(m.rect))
                .collect();
            ocr.prescan(session.layers(), &monitors, rules.codes);
        }
        session.set_analyzer(Box::new(ocr));
        if args.output.redact.is_some() && redact_now.is_none() {
            session.start_redact(false).map_err(redact_failed)?;
        }
        let mut app = picker::Picker::new(session, Picture::new(frame), ui);
        app.take_jobs(jobs);
        wl.run_overlay(&layout, &mut app, deadline())
            .map_err(Failure::capture)?;
        drop(wl);
        if let Some(r) = select_only(app.session.outcome(), &layout, args, screen) {
            return r;
        }
        let project = app.session.project();
        let outcome = app.session.outcome().cloned();
        let redacted = app.session.redacted();
        return finish(
            outcome,
            &layout,
            app.picture(),
            &args.output,
            &project,
            &redacted,
            screen,
        );
    }
    drop(wl);
    let mut picture = Picture::new(frame);
    picture.sync(session.layers(), None, None);
    if let Some(r) = select_only(session.outcome(), &layout, args, screen) {
        return r;
    }
    let project = session.project();
    finish(
        session.outcome().cloned(),
        &layout,
        &picture,
        &args.output,
        &project,
        &session.redacted(),
        screen,
    )
}

/// `--redact` to run before the picker: `auto`, which needs its answer at
/// once, and any with `--keys` or `--no-ui`, which play over it. The
/// overlay's `suggest` scans on a thread instead, so the overlay opens at
/// once and the suggestions come in when they're ready.
fn redact_inline(args: &cli::Pick) -> Option<cli::Redact> {
    args.output
        .redact
        .filter(|&how| how == cli::Redact::Auto || args.keys.is_some() || args.no_ui)
}

/// `--select-only`: a box the picker finished with, as its geometry. A
/// finish without a box (a colour, a measurement) is a cancel.
fn select_only(
    outcome: Option<&Outcome>,
    layout: &Layout,
    args: &cli::Pick,
    screen: bool,
) -> Option<Result<Report, Failure>> {
    if !args.select_only {
        return None;
    }
    let Some(rect) = selected_box(outcome) else {
        return Some(Ok(Report::cancelled()));
    };
    if screen {
        files::remember(&layout.monitors, rect);
    }
    let text = region::format(rect, args.region_format);
    Some(
        deliver::text(&text, &args.output, "Region")
            .map_err(Failure::delivery)
            .map(|()| {
                Report::done("region")
                    .rect(rect, layout)
                    .set("text", text)
                    .set("copied", args.output.copies())
            }),
    )
}

/// The box a finish was about, if it had one.
fn selected_box(outcome: Option<&Outcome>) -> Option<LogicalRect> {
    match outcome? {
        Outcome::Done(action) => match action {
            FinishedAction::Image { rect }
            | FinishedAction::Ocr { rect }
            | FinishedAction::Pin { rect }
            | FinishedAction::Code { rect, .. } => Some(*rect),
            FinishedAction::Colour { .. } | FinishedAction::Measure { .. } => None,
        },
        Outcome::Cancelled => None,
    }
}

/// `history`: recent shots, newest first.
fn history(n: usize, json: bool) -> Result<Report, Failure> {
    for shot in files::shots().into_iter().take(n) {
        let meta = std::fs::metadata(&shot).ok();
        let time = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| jiff::Timestamp::try_from(t).ok())
            .map(|t| {
                t.to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let size = std::fs::read(&shot)
            .ok()
            .and_then(|b| screenie_image::encode::png_size(&b))
            .unwrap_or((0, 0));
        let project = files::project_of(&shot);
        let project = project.exists().then(|| project.display().to_string());
        if json {
            let mut line =
                json!({"time": time, "w": size.0, "h": size.1, "path": shot.display().to_string()});
            // Only when the shot kept one, as docs/cli.md says.
            if let Some(p) = project {
                line["project"] = p.into();
            }
            say(line);
        } else {
            say(format!("{time}  {}x{}  {}", size.0, size.1, shot.display()));
        }
    }
    Ok(Report::done("history"))
}

/// Deliver what the picker produced, from the committed picture.
fn finish(
    outcome: Option<Outcome>,
    layout: &Layout,
    picture: &Picture,
    out: &cli::Output,
    project: &Project,
    redacted: &[(&str, LogicalRect)],
    screen: bool,
) -> Result<Report, Failure> {
    let layers = &project.layers;
    let Some(Outcome::Done(action)) = outcome else {
        return Ok(Report::cancelled());
    };
    Ok(match action {
        FinishedAction::Image { rect } => {
            let mut project = project.clone();
            project.selection = rect;
            let base = picture.base().image();
            let keep = || open::project_png(base, &project, layout);
            // An opened image keeps no project: its "monitor" is a canvas.
            let keep: Option<&dyn Fn() -> Result<Vec<u8>, String>> =
                if screen { Some(&keep) } else { None };
            // An opened image's own pixels are its native size.
            let monitors: &[screenie_core::layout::Monitor] =
                if screen { &layout.monitors } else { &[] };
            let img = deliver::at_scale(picture.crop(rect), rect, monitors, out.scale);
            let names = naming(&img, Some(layout), Some(rect));
            let d = deliver::image(&img, out, "Area", keep, &names).map_err(Failure::delivery)?;
            if out.pin {
                float(&img, rect, out.png_level)?;
            }
            if screen {
                files::remember(&layout.monitors, rect);
            }
            let described: Vec<String> = layers
                .iter()
                .filter(|l| !l.hidden)
                .map(|l| l.describe())
                .collect();
            Report::done("image")
                .rect(rect, layout)
                .size(&img)
                .set("layers", described)
                .redacted(out, redacted)
                .delivered(&d)
        }
        FinishedAction::Colour { pixel } => {
            let rgb = picture
                .rgb(pixel)
                .ok_or_else(|| Failure::delivery("the cursor is off the frame"))?;
            let text = deliver::colour(rgb, out.colour_format);
            deliver::text(&text, out, "Colour copied").map_err(Failure::delivery)?;
            Report::done("colour")
                .set("text", text)
                .set("rgb", json!([rgb.0, rgb.1, rgb.2]))
                .set("copied", out.copies())
        }
        FinishedAction::Ocr { rect } => {
            if screen {
                files::remember(&layout.monitors, rect);
            }
            let text = ocr(picture, rect, out)?;
            if text.is_empty() {
                if deliver::notifies(out) {
                    notify("No text found", "");
                }
            } else {
                deliver::text(&text, out, "Text copied").map_err(Failure::delivery)?;
            }
            let copied = out.copies() && !text.is_empty();
            Report::done("text")
                .rect(rect, layout)
                .set("text", text)
                .set("copied", copied)
        }
        FinishedAction::Measure { text, json } => {
            deliver::text(&text, out, "Measurements copied").map_err(Failure::delivery)?;
            Report::done("measure")
                .set("text", text)
                .set("scale", json["scale"].clone())
                .set("measurements", json["measurements"].clone())
                .set("copied", out.copies())
        }
        FinishedAction::Code { text, rect } => {
            deliver::text(&text, out, "Code copied").map_err(Failure::delivery)?;
            Report::done("code")
                .rect(rect, layout)
                .set("text", text)
                .set("copied", out.copies())
        }
        FinishedAction::Pin { rect } => {
            if screen {
                files::remember(&layout.monitors, rect);
            }
            let (id, path) = float(&picture.crop(rect), rect, out.png_level)?;
            Report::done("pin")
                .rect(rect, layout)
                .set("id", id)
                .set("path", path.display().to_string())
        }
    })
}

/// OCR a box: the text, empty if none was found.
fn ocr(picture: &Picture, rect: LogicalRect, out: &cli::Output) -> Result<String, Failure> {
    let r = picture.committed().map().to_pixels(rect);
    let crop = analysis::Crop::of(picture, r).map_err(Failure::delivery)?;
    let result = crop.recognize(&out.ocr_lang(), false).map_err(|e| {
        if deliver::notifies(out) {
            notify("OCR failed", &e);
        }
        Failure::delivery(e)
    })?;
    Ok(result.text)
}

/// `[redact]`'s rules from the config, which aren't options; unset, the
/// defaults.
static REDACT_RULES: std::sync::OnceLock<screenie_core::redact::Rules> = std::sync::OnceLock::new();

/// Auto-redact's rules (config), and what it knows of this machine.
fn redact_rules() -> screenie_core::redact::Rules {
    screenie_core::redact::Rules {
        env: redact_env(),
        ..REDACT_RULES.get().cloned().unwrap_or_default()
    }
}

/// This user and machine, for the home-path and host detectors.
fn redact_env() -> screenie_core::redact::Env {
    let var = |k| std::env::var(k).unwrap_or_default();
    let hostname = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| var("HOSTNAME"));
    screenie_core::redact::Env {
        user: var("USER"),
        home: var("HOME"),
        hostname,
    }
}

/// The command line with the config's options before it (none for the
/// clipboard and pin servers, which are screenie talking to itself), and
/// warnings about what the config skipped.
fn with_config() -> Result<(Vec<std::ffi::OsString>, Vec<String>), String> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| {
        a == screenie_platform::clipboard::SERVE_ARG || a == screenie_platform::pins::SERVE_ARG
    }) {
        return Ok((args, vec![]));
    }
    let g = config::split_globals(args)?;
    // The typed line stands on its own, and says which keys apply.
    let typed = cli::parse(g.rest.clone())?;
    if config::Typed::of(&typed).is_none() {
        return Ok((g.rest, vec![])); // help and the like
    }
    let text = config::read(&g)?;
    if let Some(text) = &text {
        let _ = REDACT_RULES.set(config::redact(text, &g.profiles)?);
    }
    config::with_options(text.as_deref(), g, &typed)
}

/// `--quiet`: no notes on stderr.
fn quiet(cmd: &Command) -> bool {
    match cmd {
        Command::Pick(p) | Command::Edit { pick: p, .. } => p.output.quiet,
        Command::Shot(s) => s.output.quiet,
        Command::History { quiet, .. }
        | Command::Pin { quiet, .. }
        | Command::PinList { quiet, .. }
        | Command::PinClose { quiet, .. } => *quiet,
        _ => false,
    }
}

/// A line on stdout. A reader that's gone (`| head`) wants no more of it,
/// so that's no panic.
fn say(line: impl std::fmt::Display) {
    let _ = writeln!(std::io::stdout().lock(), "{line}");
}

/// A help text on stdout, or its error as a usage error.
fn print_or_usage(text: Result<String, String>) -> ExitCode {
    match text {
        Ok(text) => {
            let _ = std::io::stdout().lock().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("screenie: {e}");
            ExitCode::from(Exit::Usage as u8)
        }
    }
}

fn main() -> ExitCode {
    let cmd = match with_config().and_then(|(line, warnings)| Ok((cli::parse(line)?, warnings))) {
        Ok((c, warnings)) => {
            if !quiet(&c) {
                for w in warnings {
                    eprintln!("screenie: {w}");
                }
            }
            c
        }
        Err(e) => {
            eprintln!("screenie: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(Exit::Usage as u8);
        }
    };
    let json = match &cmd {
        Command::Pick(p) => p.output.json,
        Command::Shot(s) => s.output.json,
        Command::Edit { pick, .. } => pick.output.json,
        Command::Pin { json, .. } | Command::PinClose { json, .. } => *json,
        _ => false,
    };
    let result = match cmd {
        Command::Help => {
            say(cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Command::HelpTopic(t) => return print_or_usage(help::topic(&t)),
        Command::Completions(sh) => return print_or_usage(help::completions(&sh)),
        Command::Version => {
            say(format!("screenie {}", env!("CARGO_PKG_VERSION")));
            return ExitCode::SUCCESS;
        }
        Command::ServePin(args) => return serve_pin(&args),
        Command::Pin {
            source, at, json, ..
        } => {
            let r = pin_cmd(&source, at);
            if let (Ok(report), false) = (&r, json) {
                if let Some(id) = report.get("id") {
                    say(id);
                }
            }
            r
        }
        Command::PinList { json, .. } => pin_list(json),
        Command::PinClose { ids, .. } => pin_close(&ids),
        Command::ServeClipboard => {
            return match screenie_platform::clipboard::serve() {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(Exit::Delivery as u8),
            };
        }
        Command::Shot(args) => shot(args),
        Command::Pick(args) => keys_from_file(args).and_then(pick),
        Command::Edit { source, pick } => keys_from_file(pick).and_then(|p| edit(source, p)),
        Command::History { n, json, .. } => history(n, json),
    };
    match result {
        Ok(report) => {
            if json {
                say(report.line());
            }
            ExitCode::from(if report.cancelled {
                Exit::Cancelled
            } else {
                Exit::Done
            } as u8)
        }
        Err(Failure(code, msg)) => {
            if json {
                say(Report::error(code as u8, &msg));
            }
            eprintln!("screenie: {msg}");
            ExitCode::from(code as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_covers_every_verb() {
        for line in [
            "history --quiet",
            "pin list --quiet",
            "pin close --all --quiet",
            "pin pic.png --quiet",
            "shot --quiet",
            "--quiet",
        ] {
            let c = cli::parse(line.split(' ').map(std::ffi::OsString::from)).unwrap();
            assert!(quiet(&c), "{line}");
        }
        assert!(!quiet(&cli::parse(["history".into()]).unwrap()));
    }

    #[test]
    fn select_only_gives_a_box_or_nothing() {
        let rect = LogicalRect::new(1.0, 2.0, 3.0, 4.0);
        for action in [
            FinishedAction::Image { rect },
            FinishedAction::Ocr { rect },
            FinishedAction::Pin { rect },
            FinishedAction::Code {
                rect,
                text: "x".into(),
            },
        ] {
            assert_eq!(selected_box(Some(&Outcome::Done(action))), Some(rect));
        }
        let pixel = screenie_core::geometry::PixelPoint { x: 1, y: 2 };
        for outcome in [
            Outcome::Done(FinishedAction::Colour { pixel }),
            Outcome::Done(FinishedAction::Measure {
                text: "1m".into(),
                json: json!({}),
            }),
            Outcome::Cancelled,
        ] {
            assert_eq!(selected_box(Some(&outcome)), None, "{outcome:?}");
        }
        assert_eq!(selected_box(None), None);
    }

    #[test]
    fn a_region_from_stdin_reads_as_typed_over_an_image() {
        assert_eq!(region_spec("0,0 10x10").ok().as_deref(), Some("0,0 10x10"));
        // slurp's line, newline and all, as region_spec gives it.
        let image = LogicalRect::new(0.0, 0.0, 100.0, 100.0);
        assert_eq!(
            open::region("0,0 10x10\n", image, 1.0),
            Ok(LogicalRect::new(0.0, 0.0, 10.0, 10.0))
        );
    }

    #[test]
    fn a_region_with_nothing_there_is_exit_3_a_bad_one_exit_2() {
        let layout = Layout::new(
            vec![screenie_core::layout::Monitor {
                id: screenie_core::layout::MonitorId(0),
                name: "HDMI-A-1".into(),
                rect: LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
                scale: 1.0,
            }],
            vec![],
            screenie_core::layout::MonitorId(0),
        );
        let exit = |spec| {
            resolve_region(spec, &layout, &[])
                .err()
                .map(|Failure(e, _)| e)
        };
        assert_eq!(exit("all"), None);
        for missing in ["active", "monitor:DP-3", "last-region"] {
            assert_eq!(exit(missing), Some(Exit::Capture), "{missing}");
        }
        for bad in ["nonsense", "last-region:0", "window:class:x"] {
            assert_eq!(exit(bad), Some(Exit::Usage), "{bad}");
        }
    }

    #[test]
    fn an_image_region_with_nothing_there_is_exit_3_a_bad_one_exit_2() {
        let image = LogicalRect::new(0.0, 0.0, 40.0, 30.0);
        let exit = |spec| image_region(spec, image, 1.0).err().map(|Failure(e, _)| e);
        assert_eq!(exit("all"), None);
        assert_eq!(exit("0,0 10x10"), None);
        for missing in ["0,0 0x5", "50,50 5x5"] {
            assert_eq!(exit(missing), Some(Exit::Capture), "{missing}");
        }
        for bad in ["nonsense", "monitor", "38,28 5x5"] {
            assert_eq!(exit(bad), Some(Exit::Usage), "{bad}");
        }
    }

    #[test]
    fn a_box_with_nothing_to_capture_is_exit_3() {
        let monitor = open::its_own(&screenie_image::Image::new(100, 50));
        let layout = open::one_monitor(&monitor);
        let frame = Frame::new(screenie_image::Image::new(100, 50), layout.bounds);
        let exit = |x, y, w, h| {
            capturable(LogicalRect::new(x, y, w, h), &frame, &layout)
                .err()
                .map(|Failure(e, _)| e)
        };
        assert_eq!(exit(0.0, 0.0, 10.0, 10.0), None);
        assert_eq!(exit(90.0, 40.0, 20.0, 20.0), None, "partly off");
        for (x, y, w, h) in [
            (0.0, 0.0, 0.0, 0.0),
            (0.0, 0.0, 0.2, 0.2),
            (500.0, 0.0, 10.0, 10.0),
        ] {
            assert_eq!(exit(x, y, w, h), Some(Exit::Capture), "{x},{y} {w}x{h}");
        }
    }

    #[test]
    fn suggestions_wait_for_the_overlay_unless_keys_need_them() {
        use cli::Redact::{Auto, Suggest};
        let pick = |redact, keys: Option<&str>, no_ui| cli::Pick {
            output: cli::Output {
                redact,
                ..cli::Output::default()
            },
            keys: keys.map(str::to_owned),
            no_ui,
            ..cli::Pick::default()
        };
        assert_eq!(redact_inline(&pick(Some(Suggest), None, false)), None);
        assert_eq!(redact_inline(&pick(None, Some("Return"), true)), None);
        assert_eq!(redact_inline(&pick(Some(Auto), None, false)), Some(Auto));
        assert_eq!(
            redact_inline(&pick(Some(Suggest), Some("Return"), false)),
            Some(Suggest)
        );
        assert_eq!(
            redact_inline(&pick(Some(Suggest), None, true)),
            Some(Suggest)
        );
    }

    #[test]
    fn an_ocr_failure_is_exit_4_wherever_it_happens() {
        let Failure(exit, msg) = redact_failed("no tesseract");
        assert_eq!(
            (exit, msg.as_str()),
            (Exit::Delivery, "auto-redact: no tesseract")
        );
    }

    #[test]
    fn a_project_is_editable_only_on_its_own_monitors() {
        let layout = |name: &str, scale| {
            Layout::new(
                vec![screenie_core::layout::Monitor {
                    id: screenie_core::layout::MonitorId(0),
                    name: name.into(),
                    rect: LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
                    scale,
                }],
                vec![],
                screenie_core::layout::MonitorId(0),
            )
        };
        assert!(same_monitors(&layout("DP-3", 1.0), &layout("DP-3", 1.0)));
        assert!(!same_monitors(&layout("DP-3", 1.0), &layout("DP-3", 1.5)));
        assert!(!same_monitors(
            &layout("DP-3", 1.0),
            &layout("HDMI-A-1", 1.0)
        ));
    }

    #[test]
    fn a_deadline_is_seconds_or_nothing() {
        assert_eq!(parse_deadline("3"), Some(Duration::from_secs(3)));
        assert_eq!(parse_deadline("0.5"), Some(Duration::from_millis(500)));
        for bad in ["-1", "nan", "inf", "1e30", "x", ""] {
            assert_eq!(parse_deadline(bad), None, "{bad}");
        }
    }
}
