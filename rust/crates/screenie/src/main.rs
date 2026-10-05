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

use std::process::ExitCode;
use std::time::Duration;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;
use screenie_core::model::{Project, ProjectFile};
use screenie_core::session::{FinishedAction, Outcome, Session, Setup};
use screenie_image::ui::Ui;
use screenie_image::{Frame, Picture};
use screenie_platform::desktop::{DesktopInfo, hyprland::Hyprland};
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
/// be in the capture, so there's no countdown to look at).
fn wait(out: &cli::Output) {
    let Some(secs) = out.delay.filter(|&s| s > 0.0) else {
        return;
    };
    if deliver::notifies(out) {
        notify("screenie", &format!("Capturing in {secs}s"));
    }
    std::thread::sleep(Duration::from_secs_f64(secs));
}

/// A REGION, where `-` means one read from stdin (`slurp | screenie shot -`).
fn resolve_region(
    spec: &str,
    layout: &Layout,
    ring: &[LogicalRect],
) -> Result<LogicalRect, Failure> {
    let spec = if spec == "-" {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| Failure::usage(format!("reading the region from stdin: {e}")))?;
        line
    } else {
        spec.to_owned()
    };
    region::resolve(&spec, layout, ring).map_err(Failure::usage)
}

fn ring_on(layout: &Layout) -> Vec<LogicalRect> {
    files::load_ring().resolve(&layout.monitors)
}

/// `shot active`: the focused window from its own pixels, even if it's
/// covered (replacing the prototype's `--window`).
fn shot_active(out: &cli::Output) -> Result<Report, Failure> {
    let active = Hyprland
        .active_client()
        .map_err(Failure::capture)?
        .ok_or_else(|| Failure::capture("no active window"))?;
    shot_window(&active, out)
}

/// A window by `window:SPEC` (see `find_window`): no match is exit 3, a bad
/// spec exit 2.
fn window_by(spec: &str) -> Result<screenie_platform::desktop::hyprland::HyprClient, Failure> {
    let spec = spec.strip_prefix("window:").unwrap_or(spec);
    let clients = Hyprland.clients().map_err(Failure::capture)?;
    screenie_platform::desktop::hyprland::find_window(&clients, spec)
        .map_err(Failure::usage)?
        .cloned()
        .ok_or_else(|| Failure::capture(format!("no window matches window:{spec}")))
}

/// A window's own pixels (covered or on another workspace, still whole).
fn shot_window(
    active: &screenie_platform::desktop::hyprland::HyprClient,
    out: &cli::Output,
) -> Result<Report, Failure> {
    wait(out);
    let mut wl = Wayland::connect().map_err(Failure::capture)?;
    wl.set_cursor(out.cursor);
    if active.stable_id.is_empty() {
        return Err(Failure::capture("Hyprland gave the window no stable id"));
    }
    let img = wl
        .capture_window(&active.stable_id)
        .map_err(Failure::capture)?;
    drop(wl);
    let what = format!("{} window", active.class);
    // The window on its own: one monitor its size, for auto-redact.
    let rect = LogicalRect::new(0.0, 0.0, f64::from(img.width()), f64::from(img.height()));
    let layout = Layout::new(
        vec![screenie_core::layout::Monitor {
            id: screenie_core::layout::MonitorId(0),
            name: what.clone(),
            rect,
            scale: 1.0,
        }],
        vec![],
        screenie_core::layout::MonitorId(0),
    );
    let (picture, _, redacted) = redact_shot(&Frame::new(img, rect), &layout, rect, out)?;
    let window = LogicalRect::new(active.at[0], active.at[1], active.size[0], active.size[1]);
    let img = deliver::at_scale(picture.crop(rect), window, &[], out.scale);
    let mut names = naming(&img, None, None);
    (names.class, names.title) = (Some(active.class.clone()), Some(active.title.clone()));
    let d = deliver::image(&img, out, &what, None, &names).map_err(Failure::delivery)?;
    if out.pin {
        let at = LogicalRect::new(active.at[0], active.at[1], active.size[0], active.size[1]);
        float(&img, at, out)?;
    }
    Ok(Report::done("image")
        .redacted(out, &redacted)
        .set("px", json!({"w": img.width(), "h": img.height()}))
        .set(
            "window",
            json!({"class": active.class, "title": active.title, "address": active.address}),
        )
        .delivered(&d))
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
    s.start_redact(true)
        .map_err(|e| Failure::capture(format!("auto-redact: {e}")))?;
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

type RedactedShot = (Picture, Project, Vec<(String, LogicalRect)>);

fn shot(args: cli::Shot) -> Result<Report, Failure> {
    match args.from.clone() {
        Some(cli::Source::Last(n)) => return shot_last(n, &args),
        Some(source) if source.is_image() => return shot_image(&source, &args),
        _ => {}
    }
    let spec = args.region.as_deref().unwrap_or("active");
    if spec == "active" {
        return shot_active(&args.output);
    }
    if spec.starts_with("window:") {
        return shot_window(&window_by(spec)?, &args.output);
    }
    let (_wl, layout, frame) = capture(&args.output)?;
    let rect = resolve_region(spec, &layout, &ring_on(&layout))?;
    if frame.map().to_pixels(rect).is_empty() {
        return Err(Failure::capture("the region is empty"));
    }
    let (picture, project, redacted) = redact_shot(&frame, &layout, rect, &args.output)?;
    let keep = || project_png(frame.image(), &project, &layout);
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
        float(&img, rect, &args.output)?;
    }
    Ok(Report::done("image")
        .rect(rect, &layout)
        .size(&img)
        .redacted(&args.output, &redacted)
        .delivered(&d))
}

/// `shot --from FILE|-`: a region of an image (all of it by default),
/// delivered as it is; a project PNG with its layers drawn in.
fn shot_image(source: &cli::Source, args: &cli::Shot) -> Result<Report, Failure> {
    let bytes = open::read(source).map_err(Failure::capture)?;
    let img = match open::project_in(&bytes) {
        Some((img, file)) => open::flatten(img, &file),
        None => {
            screenie_image::decode::decode(&bytes, open::BACKGROUND).map_err(Failure::capture)?
        }
    };
    let monitor = open::its_own(&img);
    let (frame, image) = screenie_image::compose::place(img, monitor.rect, 1.0, open::BACKGROUND);
    let layout = open::one_monitor(&monitor);
    let rect = open::region(args.region.as_deref().unwrap_or("all"), image, 1.0)
        .map_err(Failure::usage)?;
    let (picture, _, redacted) = redact_shot(&frame, &layout, rect, &args.output)?;
    let img = deliver::at_scale(picture.crop(rect), rect, &[], args.output.scale);
    let d = deliver::image(&img, &args.output, "Image", None, &naming(&img, None, None))
        .map_err(Failure::delivery)?;
    if args.output.pin {
        float(&img, middle_of_focused(&img), &args.output)?;
    }
    Ok(Report::done("image")
        .set(
            "rect",
            json!({"x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h}),
        )
        .size(&img)
        .redacted(&args.output, &redacted)
        .delivered(&d))
}

/// `shot --from last[:N]`: a past shot delivered again, its layers drawn
/// in, over its own box unless a region says otherwise.
fn shot_last(which: usize, args: &cli::Shot) -> Result<Report, Failure> {
    let (img, file) = last_project(which)?;
    let layout = file.layout.clone();
    let rect = match args.region.as_deref() {
        Some(spec) => resolve_region(spec, &layout, &ring_on(&layout))?,
        None => file.project.selection,
    };
    let mut picture = Picture::new(Frame::new(img, layout.bounds));
    picture.sync(&file.project.layers, None, None);
    let img = deliver::at_scale(
        picture.crop(rect),
        rect,
        &layout.monitors,
        args.output.scale,
    );
    let names = naming(&img, Some(&layout), Some(rect));
    let d = deliver::image(&img, &args.output, "Area", None, &names).map_err(Failure::delivery)?;
    if args.output.pin {
        float(&img, rect, &args.output)?;
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
    out: &cli::Output,
) -> Result<(u32, std::path::PathBuf), Failure> {
    let png = screenie_image::encode::png(img, out.png_level)
        .map_err(|e| Failure::delivery(e.to_string()))?;
    let dir = files::runtime_dir().map_err(|e| Failure::delivery(e.to_string()))?;
    let (path, mut f) =
        files::create_unique(&dir, &format!("pin-{}", files::stamp()), "png", 0o600)
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

/// `pin [FILE|-|last[:N]] [--at X,Y]`: float an image, where it came from
/// if screenie took it, else at `--at`, else the focused monitor's middle.
fn pin_cmd(source: &cli::Source, at: Option<(f64, f64)>) -> Result<Report, Failure> {
    let (img, from) = match source {
        cli::Source::Last(n) => {
            let (img, file) = last_project(*n)?;
            let mut picture = Picture::new(Frame::new(img, file.layout.bounds));
            picture.sync(&file.project.layers, None, None);
            let rect = file.project.selection;
            (picture.crop(rect), Some(rect))
        }
        source => {
            let bytes = open::read(source).map_err(Failure::capture)?;
            let img = match open::project_in(&bytes) {
                Some((img, file)) => open::flatten(img, &file),
                None => screenie_image::decode::decode(&bytes, open::BACKGROUND)
                    .map_err(Failure::capture)?,
            };
            (img, None)
        }
    };
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let rect = match (at, from) {
        (Some((x, y)), Some(r)) => LogicalRect::new(x, y, r.w, r.h),
        (Some((x, y)), None) => LogicalRect::new(x, y, w, h),
        (None, Some(r)) => r,
        (None, None) => middle_of_focused(&img),
    };
    let out = cli::Output {
        png_level: 1,
        ..cli::Output::default()
    };
    let (id, path) = float(&img, rect, &out)?;
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
            println!(
                "{}",
                json!({"id": p.id, "size": p.size, "at": p.at, "path": p.path})
            );
        } else {
            let at = p.at.map_or(String::new(), |[x, y]| format!("  at {x},{y}"));
            println!(
                "{}  {}x{}{at}  {}",
                p.id,
                p.size[0],
                p.size[1],
                p.path.display()
            );
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
    let at = Hyprland.layout().ok().and_then(|l| {
        l.monitors
            .iter()
            .find(|m| m.id == l.focused)
            .map(|m| m.rect)
    });
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
    let secs: f64 = std::env::var("SCREENIE_DEADLINE").ok()?.parse().ok()?;
    Some(Duration::from_secs_f64(secs))
}

fn pick(args: cli::Pick) -> Result<Report, Failure> {
    match args.from.clone() {
        Some(cli::Source::Last(n)) => return edit_last(n, args),
        Some(source) if source.is_image() => return pick_image(&source, args),
        _ => {}
    }
    let (wl, layout, frame) = capture(&args.output)?;
    let ring = ring_on(&layout);
    let start = match &args.region {
        // The box on a window, wherever it is (as Hyprland places it).
        Some(spec) if spec.starts_with("window:") => {
            let w = window_by(spec)?;
            Some(LogicalRect::new(w.at[0], w.at[1], w.size[0], w.size[1]))
        }
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

/// `edit FILE|-|last[:N]`: `pick --from SOURCE`, the box on all of it.
fn edit(source: cli::Source, args: cli::Pick) -> Result<Report, Failure> {
    match source {
        cli::Source::Last(n) => edit_last(n, args),
        source => pick_image(&source, args),
    }
}

/// A past shot's project: the capture, and the project with its layout.
fn last_project(which: usize) -> Result<(screenie_image::Image, ProjectFile), Failure> {
    let shot = files::shots()
        .into_iter()
        .nth(which - 1)
        .ok_or_else(|| Failure::usage(format!("there's no shot last:{which}")))?;
    let path = files::project_of(&shot);
    let bytes =
        std::fs::read(&path).map_err(|e| Failure::capture(format!("{}: {e}", path.display())))?;
    open::project_in(&bytes)
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
/// capture, its layers still editable. The monitors must be the ones it
/// was taken on.
fn edit_last(which: usize, args: cli::Pick) -> Result<Report, Failure> {
    let (img, file) = last_project(which)?;
    let wl = Wayland::connect().map_err(Failure::capture)?;
    let now = Hyprland.layout().map_err(Failure::capture)?;
    if !same_monitors(&now, &file.layout) {
        return Err(Failure::capture(
            "the monitors have changed since that shot, and edit needs the same ones",
        ));
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
    let img = match open::project_in(&bytes) {
        Some((img, file)) if same_monitors(&now, &file.layout) => {
            return edit_project(wl, img, file, args);
        }
        Some((img, file)) => open::flatten(img, &file),
        None => {
            screenie_image::decode::decode(&bytes, open::BACKGROUND).map_err(Failure::capture)?
        }
    };
    let monitor = now
        .monitors
        .iter()
        .find(|m| m.id == now.focused)
        .or(now.monitors.first())
        .ok_or_else(|| Failure::capture("no monitor to show it on"))?
        .clone();
    let (frame, image) =
        screenie_image::compose::place(img, monitor.rect, monitor.scale, open::BACKGROUND);
    let layout = open::one_monitor(&monitor);
    let k = frame.map().scale();
    let start = match &args.region {
        Some(spec) => open::region(spec, image, k).map_err(Failure::usage)?,
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
    let font = screenie_platform::fonts::monospace();
    if let Some(f) = &font {
        // Text and callouts are drawn in it, in the picker and the output.
        let _ = screenie_image::render::set_font(f);
    }
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
    if let Some(how) = args.output.redact {
        session
            .start_redact(how == cli::Redact::Auto)
            .map_err(|e| Failure::capture(format!("auto-redact: {e}")))?;
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
        let ui = Ui::new(&font).map_err(Failure::capture)?;
        let (mut ocr, jobs) = analysis::Ocr::threaded(Picture::new(frame.clone()), lang);
        if redact_rules().prescan {
            let monitors: Vec<_> = layout
                .monitors
                .iter()
                .map(|m| frame.map().to_pixels(m.rect))
                .collect();
            ocr.prescan(&monitors);
        }
        session.set_analyzer(Box::new(ocr));
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

/// `--select-only`: a box the picker finished with, as its geometry.
fn select_only(
    outcome: Option<&Outcome>,
    layout: &Layout,
    args: &cli::Pick,
    screen: bool,
) -> Option<Result<Report, Failure>> {
    if !args.select_only {
        return None;
    }
    let rect = match outcome? {
        Outcome::Done(
            FinishedAction::Image { rect }
            | FinishedAction::Ocr { rect }
            | FinishedAction::Pin { rect }
            | FinishedAction::Code { rect, .. },
        ) => *rect,
        _ => return None,
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

/// The PNG text keyword a project travels under.
const PROJECT_KEY: &str = "screenie-project";

/// A project file: the uncensored capture, with the project and layout.
fn project_png(
    base: &screenie_image::Image,
    project: &Project,
    layout: &Layout,
) -> Result<Vec<u8>, String> {
    let json = ProjectFile::new(project.clone(), layout.clone()).to_json();
    screenie_image::encode::png_with_text(base, 1, Some((PROJECT_KEY, &json)))
        .map_err(|e| e.to_string())
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
            println!(
                "{}",
                json!({"time": time, "w": size.0, "h": size.1, "path": shot.display().to_string(), "project": project})
            );
        } else {
            println!("{time}  {}×{}  {}", size.0, size.1, shot.display());
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
            let keep = || project_png(base, &project, layout);
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
                float(&img, rect, out)?;
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
            let (id, path) = float(&picture.crop(rect), rect, out)?;
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

fn notify(title: &str, body: &str) {
    screenie_platform::notify::notify(title, body);
}

/// The command line with the config's options before it (none for the
/// clipboard server, which is screenie talking to itself).
/// `[redact]`'s rules from the config, which aren't options; unset, the
/// defaults.
static REDACT_RULES: std::sync::OnceLock<screenie_core::redact::Rules> = std::sync::OnceLock::new();

/// Auto-redact's rules (config), and what it knows of this machine.
fn redact_rules() -> screenie_core::redact::Rules {
    screenie_core::redact::Rules {
        env: screenie_core::redact::Env::from_system(),
        ..REDACT_RULES.get().cloned().unwrap_or_default()
    }
}

fn with_config() -> Result<Vec<std::ffi::OsString>, String> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| {
        a == screenie_platform::clipboard::SERVE_ARG || a == screenie_platform::pins::SERVE_ARG
    }) {
        return Ok(args);
    }
    let g = config::split_globals(args)?;
    let Some(text) = config::read(&g)? else {
        return Ok(g.rest);
    };
    let (mut full, warnings) = config::options(&text, config::verb(&g.rest), &g.profiles)?;
    let _ = REDACT_RULES.set(config::redact(&text, &g.profiles)?);
    for w in warnings {
        eprintln!("screenie: {w}");
    }
    full.extend(g.rest);
    Ok(full)
}

fn main() -> ExitCode {
    let cmd = match with_config().and_then(cli::parse) {
        Ok(c) => c,
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
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Command::HelpTopic(t) => {
            return match help::topic(&t) {
                Ok(text) => {
                    print!("{text}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("screenie: {e}");
                    ExitCode::from(Exit::Usage as u8)
                }
            };
        }
        Command::Completions(sh) => {
            return match help::completions(&sh) {
                Ok(text) => {
                    print!("{text}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("screenie: {e}");
                    ExitCode::from(Exit::Usage as u8)
                }
            };
        }
        Command::Version => {
            println!("screenie {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Command::ServePin(args) => return serve_pin(&args),
        Command::Pin { source, at, json } => {
            let r = pin_cmd(&source, at);
            if let (Ok(report), false) = (&r, json) {
                if let Some(id) = report.get("id") {
                    println!("{id}");
                }
            }
            r
        }
        Command::PinList { json } => pin_list(json),
        Command::PinClose { ids, .. } => pin_close(&ids),
        Command::ServeClipboard => {
            return match screenie_platform::clipboard::serve() {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(Exit::Delivery as u8),
            };
        }
        Command::Shot(args) => shot(args),
        Command::Pick(args) => pick(args),
        Command::Edit { source, pick } => edit(source, pick),
        Command::History { n, json } => {
            return match history(n, json) {
                Ok(_) => ExitCode::SUCCESS,
                Err(Failure(code, msg)) => {
                    eprintln!("screenie: {msg}");
                    ExitCode::from(code as u8)
                }
            };
        }
    };
    match result {
        Ok(report) => {
            if json {
                println!("{}", report.line());
            }
            ExitCode::from(if report.cancelled {
                Exit::Cancelled
            } else {
                Exit::Done
            } as u8)
        }
        Err(Failure(code, msg)) => {
            if json {
                println!("{}", Report::error(code as u8, &msg));
            }
            eprintln!("screenie: {msg}");
            ExitCode::from(code as u8)
        }
    }
}
