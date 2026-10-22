//! Capturing every output with wlr-screencopy, then composing one frame of
//! the whole layout at the highest scale.

use smithay_client_toolkit::{
    dispatch2::Dispatch2,
    reexports::client::{
        Connection, Proxy, QueueHandle, WEnum,
        protocol::{wl_buffer::WlBuffer, wl_output, wl_shm},
    },
    reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{
        self, Flags, ZwlrScreencopyFrameV1,
    },
    shm::{Shm, raw::RawPool},
};

use std::time::{Duration, Instant};

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;
use screenie_image::Image;
use screenie_image::compose::{self, SourceFormat, Transform};

use super::{NoEvents, State, Wayland};
use crate::{Error, Result};

/// How long a capture may take.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum Progress {
    /// Waiting for the compositor to describe the buffer it wants.
    Describing,
    /// The copy is under way.
    Copying,
    Ready(Image),
    Failed(String),
}

/// An shm buffer the compositor offered.
#[derive(Clone, Copy)]
struct Offer {
    /// How to read the pixels.
    source: SourceFormat,
    /// The format to create the buffer in.
    wl: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

pub(crate) struct Pending {
    name: Option<String>,
    transform: Transform,
    frame: ZwlrScreencopyFrameV1,
    /// The first offer in a format screenie can read.
    offer: Option<Offer>,
    pool: Option<RawPool>,
    buffer: Option<WlBuffer>,
    y_invert: bool,
    progress: Progress,
}

/// Which pending capture a frame's events are for.
pub(crate) struct FrameData(usize);

/// How to read an shm format, if screenie can.
pub(super) fn source_format(f: wl_shm::Format) -> Option<SourceFormat> {
    use wl_shm::Format as F;
    Some(match f {
        F::Argb8888 | F::Xrgb8888 => SourceFormat::Bgra8,
        F::Abgr8888 | F::Xbgr8888 => SourceFormat::Rgba8,
        F::Xrgb2101010 | F::Argb2101010 => SourceFormat::Xrgb2101010,
        F::Xbgr2101010 | F::Abgr2101010 => SourceFormat::Xbgr2101010,
        _ => return None,
    })
}

fn transform(t: wl_output::Transform) -> Transform {
    Transform(u32::from(t) as u8)
}

impl Pending {
    fn waiting(&self) -> bool {
        matches!(self.progress, Progress::Describing | Progress::Copying)
    }

    /// Its name and how far it got, releasing a frame that never finished.
    fn end(mut self) -> (Option<String>, Progress) {
        if !matches!(self.progress, Progress::Ready(_)) {
            if let Some(b) = self.buffer.take() {
                b.destroy();
            }
            self.frame.destroy();
        }
        (self.name, self.progress)
    }

    /// The compositor described the buffer: allocate it and start the copy.
    fn copy(&mut self, shm: &Shm, qh: &QueueHandle<State>) {
        let Some(o) = self.offer else {
            self.progress = Progress::Failed("no shm buffer format we can read".into());
            return;
        };
        let mut pool = match RawPool::new((o.stride * o.height) as usize, shm) {
            Ok(p) => p,
            Err(e) => {
                self.progress = Progress::Failed(format!("shm pool: {e}"));
                return;
            }
        };
        let buffer = pool.create_buffer(
            0,
            o.width as i32,
            o.height as i32,
            o.stride as i32,
            o.wl,
            NoEvents,
            qh,
        );
        self.frame.copy(&buffer);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        self.progress = Progress::Copying;
    }

    fn finish(&mut self) {
        let (Some(o), Some(pool)) = (self.offer, self.pool.as_mut()) else {
            self.progress = Progress::Failed("ready without a buffer".into());
            return;
        };
        let data = &pool.mmap()[..];
        self.progress = match compose::convert(
            data,
            o.width,
            o.height,
            o.stride as usize,
            o.source,
            self.y_invert,
        ) {
            Some(img) => Progress::Ready(compose::orient(img, self.transform)),
            None => Progress::Failed("buffer too small".into()),
        };
        if let Some(b) = self.buffer.take() {
            b.destroy();
        }
        self.frame.destroy();
    }
}

impl Dispatch2<ZwlrScreencopyFrameV1, State> for FrameData {
    fn event(
        &self,
        state: &mut State,
        frame: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        use zwlr_screencopy_frame_v1::Event as E;
        let Some(p) = state.capture.get_mut(self.0) else {
            return;
        };
        match event {
            E::Buffer {
                format: WEnum::Value(f),
                width,
                height,
                stride,
            } => {
                if let Some(sf) = source_format(f) {
                    p.offer.get_or_insert(Offer {
                        source: sf,
                        wl: f,
                        width,
                        height,
                        stride,
                    });
                }
                if frame.version() < 3 {
                    p.copy(&state.shm, qh); // no buffer_done before v3
                }
            }
            E::BufferDone => p.copy(&state.shm, qh),
            E::Flags {
                flags: WEnum::Value(f),
            } => p.y_invert = f.contains(Flags::YInvert),
            E::Ready { .. } => p.finish(),
            E::Failed => p.progress = Progress::Failed("the compositor refused".into()),
            _ => {}
        }
    }
}

impl Wayland {
    /// Ask for a frame of every output. Returns at once; the copies arrive
    /// while the caller does other work (the desktop query).
    pub fn start_capture(&mut self) -> Result<()> {
        let qh = self.qh();
        let manager = self
            .state
            .screencopy
            .clone()
            .ok_or_else(|| Error::Unavailable("the compositor has no wlr-screencopy".into()))?;
        let outputs: Vec<_> = self.state.outputs.outputs().collect();
        if outputs.is_empty() {
            return Err(Error::Unavailable("no outputs".into()));
        }
        for output in outputs {
            let info = self.state.outputs.info(&output);
            let i = self.state.capture.len();
            let overlay_cursor = i32::from(self.state.cursor);
            let frame = manager.capture_output(overlay_cursor, &output, &qh, FrameData(i));
            self.state.capture.push(Pending {
                name: info.as_ref().and_then(|i| i.name.clone()),
                transform: info.map_or(Transform::NORMAL, |i| transform(i.transform)),
                frame,
                offer: None,
                pool: None,
                buffer: None,
                y_invert: false,
                progress: Progress::Describing,
            });
        }
        self.conn
            .flush()
            .map_err(|e| Error::Protocol(format!("flush: {e}")))?;
        Ok(())
    }

    /// Wait for the copies and compose them into one frame of `layout`, at
    /// its highest monitor scale. Outputs are matched to monitors by name.
    pub fn finish_capture(&mut self, layout: &Layout) -> Result<Image> {
        let start = Instant::now();
        while self.state.capture.iter().any(Pending::waiting) {
            // A compositor only copies a frame it renders: a monitor that's
            // asleep sends none.
            let left = CAPTURE_TIMEOUT.saturating_sub(start.elapsed());
            if left.is_zero() {
                break;
            }
            self.wait(Some(left))?;
        }
        let results = self.state.capture.drain(..).map(Pending::end).collect();
        let parts = assemble(results, layout)?;
        let scale = layout
            .monitors
            .iter()
            .map(|m| m.scale)
            .fold(1.0_f64, f64::max);
        Ok(compose::compose(layout.bounds, scale, &parts))
    }
}

/// The parts of `layout` the outputs delivered. Outputs the layout doesn't
/// know are ignored. A monitor that sent no frame in time (asleep) is left
/// blank, unless none sent one; a monitor whose capture failed fails it.
fn assemble(
    results: Vec<(Option<String>, Progress)>,
    layout: &Layout,
) -> Result<Vec<(LogicalRect, Image)>> {
    let mut parts = Vec::new();
    let mut waiting = Vec::new();
    for (name, progress) in results {
        let Some(m) = layout
            .monitors
            .iter()
            .find(|m| name.as_ref() == Some(&m.name))
        else {
            continue;
        };
        match progress {
            Progress::Ready(img) => parts.push((m.rect, img)),
            Progress::Failed(why) => {
                return Err(Error::Unavailable(format!(
                    "capture of {} failed: {why}",
                    m.name
                )));
            }
            Progress::Describing => waiting.push(format!("{} (no buffer offered)", m.name)),
            Progress::Copying => waiting.push(format!("{} (no frame)", m.name)),
        }
    }
    if !parts.is_empty() {
        Ok(parts)
    } else if !waiting.is_empty() {
        Err(Error::Unavailable(format!(
            "the capture timed out waiting on {} (are the monitors asleep?)",
            waiting.join(", ")
        )))
    } else {
        Err(Error::Unavailable("no output matched the layout".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::layout::{Monitor, MonitorId};

    fn layout() -> Layout {
        let mon = |id: u32, name: &str, x: f64| Monitor {
            id: MonitorId(id),
            name: name.into(),
            rect: LogicalRect::new(x, 0.0, 100.0, 100.0),
            scale: 1.0,
        };
        Layout::new(
            vec![mon(0, "HDMI-A-1", 0.0), mon(1, "DP-3", 100.0)],
            vec![],
            MonitorId(0),
        )
    }

    fn ready() -> Progress {
        Progress::Ready(Image::new(100, 100))
    }

    fn named(name: &str, p: Progress) -> (Option<String>, Progress) {
        (Some(name.into()), p)
    }

    #[test]
    fn shm_formats_and_transforms() {
        use wl_shm::Format as F;
        assert_eq!(source_format(F::Xrgb8888), Some(SourceFormat::Bgra8));
        assert_eq!(
            source_format(F::Xbgr2101010),
            Some(SourceFormat::Xbgr2101010)
        );
        assert_eq!(source_format(F::Rgb565), None);
        assert_eq!(transform(wl_output::Transform::_90), Transform(1));
        assert_eq!(transform(wl_output::Transform::Flipped270), Transform(7));
    }

    #[test]
    fn a_monitor_that_sends_no_frame_is_left_blank() {
        let parts = assemble(
            vec![
                named("HDMI-A-1", ready()),
                named("DP-3", Progress::Describing),
            ],
            &layout(),
        )
        .unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, LogicalRect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn no_monitor_sending_a_frame_times_out() {
        let err = assemble(
            vec![
                named("HDMI-A-1", Progress::Copying),
                named("DP-3", Progress::Describing),
            ],
            &layout(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("timed out waiting on HDMI-A-1"),
            "{err}"
        );
    }

    #[test]
    fn outputs_outside_the_layout_are_ignored() {
        let parts = assemble(
            vec![
                named("HEADLESS-1", Progress::Failed("no".into())),
                (None, Progress::Describing),
                named("DP-3", ready()),
            ],
            &layout(),
        )
        .unwrap();
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn a_failed_monitor_fails_the_capture() {
        let err = assemble(
            vec![
                named("HDMI-A-1", ready()),
                named("DP-3", Progress::Failed("the compositor refused".into())),
            ],
            &layout(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("capture of DP-3 failed"), "{err}");
    }
}
