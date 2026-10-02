//! screenie: keyboard-driven screenshots.

mod cli;
mod deliver;
mod files;
mod picker;
mod region;

use std::process::ExitCode;
use std::time::Duration;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;
use screenie_core::session::{FinishedAction, Outcome, Session, Setup};
use screenie_image::ui::Ui;
use screenie_image::{Frame, Picture};
use screenie_platform::desktop::{DesktopInfo, hyprland::Hyprland};
use screenie_platform::wayland::Wayland;

use cli::Command;

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
fn capture() -> Result<(Wayland, Layout, Frame), Failure> {
    let mut wl = Wayland::connect().map_err(Failure::capture)?;
    wl.start_capture().map_err(Failure::capture)?;
    let layout = Hyprland.layout().map_err(Failure::capture)?;
    let img = wl.finish_capture(&layout).map_err(Failure::capture)?;
    let frame = Frame::new(img, layout.bounds);
    Ok((wl, layout, frame))
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

fn shot(args: cli::Shot) -> Result<Exit, Failure> {
    let (_wl, layout, frame) = capture()?;
    let spec = args.region.as_deref().unwrap_or("active");
    let rect = resolve_region(spec, &layout, &ring_on(&layout))?;
    if frame.map().to_pixels(rect).is_empty() {
        return Err(Failure::capture("the region is empty"));
    }
    deliver::image(&frame.crop(rect), &args.output, "Area").map_err(Failure::delivery)?;
    Ok(Exit::Done)
}

/// A safety net for development runs: `SCREENIE_DEADLINE=SECS` takes the
/// overlays down after that long, whatever happens.
fn deadline() -> Option<Duration> {
    let secs: f64 = std::env::var("SCREENIE_DEADLINE").ok()?.parse().ok()?;
    Some(Duration::from_secs_f64(secs))
}

fn pick(args: cli::Pick) -> Result<Exit, Failure> {
    let (mut wl, layout, frame) = capture()?;
    let ring = ring_on(&layout);
    let start = match &args.region {
        Some(spec) => Some(resolve_region(spec, &layout, &ring)?),
        None => None,
    };
    let mut session = Session::new(Setup {
        layout: layout.clone(),
        pixels: *frame.map(),
        start,
        recall: ring,
    });
    if let Some(keys) = &args.keys {
        session
            .play(keys, 0.0)
            .map_err(|e| Failure::usage(e.to_string()))?;
    }
    if !session.is_finished() {
        if args.no_ui {
            return Err(Failure::usage("--no-ui: the keys didn't finish the picker"));
        }
        let font = screenie_platform::fonts::monospace()
            .ok_or_else(|| Failure::capture("no monospace font found"))?;
        let ui = Ui::new(&font).map_err(Failure::capture)?;
        let mut app = picker::Picker::new(session, Picture::new(frame), ui);
        wl.run_overlay(&layout, &mut app, deadline())
            .map_err(Failure::capture)?;
        drop(wl);
        return finish(
            app.session.outcome().cloned(),
            &layout,
            &app.picture,
            &args.output,
        );
    }
    drop(wl);
    let mut picture = Picture::new(frame);
    picture.sync(session.layers(), None, None);
    finish(session.outcome().cloned(), &layout, &picture, &args.output)
}

/// Deliver what the picker produced, from the committed picture.
fn finish(
    outcome: Option<Outcome>,
    layout: &Layout,
    picture: &Picture,
    out: &cli::Output,
) -> Result<Exit, Failure> {
    let Some(Outcome::Done(action)) = outcome else {
        return Ok(Exit::Cancelled);
    };
    match action {
        FinishedAction::Image { rect } => {
            deliver::image(&picture.crop(rect), out, "Area").map_err(Failure::delivery)?;
            files::remember(&layout.monitors, rect);
        }
        FinishedAction::Colour { pixel } => {
            let rgb = picture
                .rgb(pixel)
                .ok_or_else(|| Failure::delivery("the cursor is off the frame"))?;
            deliver::text(&rgb.hex(), out, "Colour copied").map_err(Failure::delivery)?;
        }
        FinishedAction::Ocr { .. } | FinishedAction::Code { .. } | FinishedAction::Pin { .. } => {
            let what = match action {
                FinishedAction::Ocr { .. } => "OCR",
                FinishedAction::Code { .. } => "QR / barcode decoding",
                _ => "pinning",
            };
            screenie_platform::notify::notify(
                "screenie",
                &format!("{what} isn't in the Rust port yet (milestone 8)"),
            );
            return Err(Failure::delivery(format!(
                "{what} isn't in the Rust port yet"
            )));
        }
    }
    Ok(Exit::Done)
}

fn main() -> ExitCode {
    let cmd = match cli::parse(std::env::args_os().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("screenie: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(Exit::Usage as u8);
        }
    };
    let result = match cmd {
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(Exit::Done)
        }
        Command::Version => {
            println!("screenie {}", env!("CARGO_PKG_VERSION"));
            Ok(Exit::Done)
        }
        Command::ServeClipboard => {
            return match screenie_platform::clipboard::serve() {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(Exit::Delivery as u8),
            };
        }
        Command::Shot(args) => shot(args),
        Command::Pick(args) => pick(args),
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(Failure(code, msg)) => {
            eprintln!("screenie: {msg}");
            ExitCode::from(code as u8)
        }
    }
}
