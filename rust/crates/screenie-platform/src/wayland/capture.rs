//! Capturing every output with wlr-screencopy, then composing one frame of
//! the whole layout at the highest scale.

use smithay_client_toolkit::{
    dispatch2::Dispatch2,
    reexports::client::{
        Connection, Proxy, QueueHandle, WEnum,
        protocol::{wl_buffer::WlBuffer, wl_output, wl_shm},
    },
    reexports::protocols_wlr::screencopy::v1::client::{
        zwlr_screencopy_frame_v1::{self, Flags, ZwlrScreencopyFrameV1},
        zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    },
    shm::raw::RawPool,
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

pub(crate) struct Pending {
    name: Option<String>,
    transform: Transform,
    frame: ZwlrScreencopyFrameV1,
    /// The shm buffer the compositor offered: format, width, height, stride.
    offer: Option<(SourceFormat, wl_shm::Format, u32, u32, u32)>,
    pool: Option<RawPool>,
    buffer: Option<WlBuffer>,
    y_invert: bool,
    progress: Progress,
}

/// Which pending capture a frame's events are for.
pub(crate) struct FrameData(usize);

fn source_format(f: wl_shm::Format) -> Option<SourceFormat> {
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
    use wl_output::Transform as T;
    Transform(match t {
        T::Normal => 0,
        T::_90 => 1,
        T::_180 => 2,
        T::_270 => 3,
        T::Flipped => 4,
        T::Flipped90 => 5,
        T::Flipped180 => 6,
        T::Flipped270 => 7,
        _ => 0,
    })
}

impl Dispatch2<ZwlrScreencopyManagerV1, State> for NoEvents {
    fn event(
        &self,
        _: &mut State,
        _: &ZwlrScreencopyManagerV1,
        _: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch2<WlBuffer, State> for NoEvents {
    fn event(
        &self,
        _: &mut State,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Pending {
    /// The compositor described the buffer: allocate it and start the copy.
    fn copy(&mut self, state_shm: &smithay_client_toolkit::shm::Shm, qh: &QueueHandle<State>) {
        let Some((_, wl_format, w, h, stride)) = self.offer else {
            self.progress = Progress::Failed("no shm buffer format we can read".into());
            return;
        };
        let mut pool = match RawPool::new((stride * h) as usize, state_shm) {
            Ok(p) => p,
            Err(e) => {
                self.progress = Progress::Failed(format!("shm pool: {e}"));
                return;
            }
        };
        let buffer = pool.create_buffer(
            0,
            w as i32,
            h as i32,
            stride as i32,
            wl_format,
            NoEvents,
            qh,
        );
        self.frame.copy(&buffer);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        self.progress = Progress::Copying;
    }

    fn finish(&mut self) {
        let (Some((format, _, w, h, stride)), Some(pool)) = (self.offer, self.pool.as_mut()) else {
            self.progress = Progress::Failed("ready without a buffer".into());
            return;
        };
        let data = &pool.mmap()[..];
        self.progress = match compose::convert(data, w, h, stride as usize, format, self.y_invert) {
            Some(img) => Progress::Ready(compose::orient(&img, self.transform)),
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
                    p.offer.get_or_insert((sf, f, width, height, stride));
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
        while self
            .state
            .capture
            .iter()
            .any(|p| matches!(p.progress, Progress::Describing | Progress::Copying))
        {
            // A compositor only copies a frame it renders: with the
            // monitors asleep, none comes.
            let left = CAPTURE_TIMEOUT.saturating_sub(start.elapsed());
            if left.is_zero() {
                let waiting: Vec<String> = self
                    .state
                    .capture
                    .iter()
                    .filter(|p| matches!(p.progress, Progress::Describing | Progress::Copying))
                    .map(|p| {
                        let stage = match p.progress {
                            Progress::Describing => "no buffer offered",
                            _ => "no frame",
                        };
                        format!("{} ({stage})", p.name.as_deref().unwrap_or("an output"))
                    })
                    .collect();
                self.state.capture.clear();
                return Err(Error::Unavailable(format!(
                    "the capture timed out waiting on {} (are the monitors asleep?)",
                    waiting.join(", ")
                )));
            }
            self.wait(Some(left))?;
        }
        let mut parts: Vec<(LogicalRect, Image)> = Vec::new();
        for p in self.state.capture.drain(..) {
            let monitor = p
                .name
                .as_ref()
                .and_then(|n| layout.monitors.iter().find(|m| &m.name == n));
            match (p.progress, monitor) {
                (Progress::Ready(img), Some(m)) => parts.push((m.rect, img)),
                (Progress::Ready(_), None) => {} // an output the layout doesn't know
                (Progress::Failed(why), _) => {
                    return Err(Error::Unavailable(format!(
                        "capture of {} failed: {why}",
                        p.name.as_deref().unwrap_or("an output")
                    )));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            return Err(Error::Unavailable("no output matched the layout".into()));
        }
        let scale = layout
            .monitors
            .iter()
            .map(|m| m.scale)
            .fold(1.0_f64, f64::max);
        Ok(compose::compose(layout.bounds, scale, &parts))
    }
}
