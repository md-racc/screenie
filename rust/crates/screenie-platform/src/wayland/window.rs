//! Capturing one window from its own pixels, even when it's covered or on
//! another workspace: ext-image-copy-capture with a foreign-toplevel
//! source (what `grim -T` does). The window is named by its
//! ext-foreign-toplevel-list identifier, which is Hyprland's `stableId`.

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    dispatch2::Dispatch2,
    foreign_toplevel_list::{ForeignToplevelList, ForeignToplevelListHandler},
    reexports::client::{
        Connection, Proxy, QueueHandle, WEnum,
        protocol::{wl_buffer::WlBuffer, wl_shm},
    },
    reexports::protocols::ext::{
        foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        image_capture_source::v1::client::{
            ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
            ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        },
        image_copy_capture::v1::client::{
            ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
            ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
            ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
        },
    },
    shm::raw::RawPool,
};

use screenie_image::Image;
use screenie_image::compose::{self, SourceFormat, Transform};

use super::{NoEvents, State, Wayland};
use crate::{Error, Result};

/// How long a window capture may take.
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub(crate) struct WindowCapture {
    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
    described: bool,
    transform: u8,
    ready: bool,
    failed: Option<String>,
}

/// The user data for the session's and frame's events.
pub(crate) struct WindowData;

impl ForeignToplevelListHandler for State {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelList {
        self.toplevels.as_mut().expect("bound before use")
    }
    fn new_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: ExtForeignToplevelHandleV1,
    ) {
    }
    fn update_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: ExtForeignToplevelHandleV1,
    ) {
    }
    fn toplevel_closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: ExtForeignToplevelHandleV1,
    ) {
    }
}

macro_rules! no_events {
    ($($t:ty),*) => {$(
        impl Dispatch2<$t, State> for NoEvents {
            fn event(&self, _: &mut State, _: &$t, _: <$t as Proxy>::Event, _: &Connection, _: &QueueHandle<State>) {}
        }
    )*};
}

no_events!(
    ExtImageCopyCaptureManagerV1,
    ExtForeignToplevelImageCaptureSourceManagerV1,
    ExtImageCaptureSourceV1
);

impl Dispatch2<ExtImageCopyCaptureSessionV1, State> for WindowData {
    fn event(
        &self,
        state: &mut State,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use ext_image_copy_capture_session_v1::Event as E;
        let Some(w) = &mut state.window else { return };
        match event {
            E::BufferSize { width, height } => w.size = Some((width, height)),
            E::ShmFormat {
                format: WEnum::Value(f),
            } => w.formats.push(f),
            E::Done => w.described = true,
            E::Stopped => w.failed = Some("the window went away".into()),
            _ => {}
        }
    }
}

impl Dispatch2<ExtImageCopyCaptureFrameV1, State> for WindowData {
    fn event(
        &self,
        state: &mut State,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use ext_image_copy_capture_frame_v1::Event as E;
        let Some(w) = &mut state.window else { return };
        match event {
            E::Transform {
                transform: WEnum::Value(t),
            } => w.transform = u32::from(t) as u8,
            E::Ready => w.ready = true,
            E::Failed { reason } => w.failed = Some(format!("the capture failed ({reason:?})")),
            _ => {}
        }
    }
}

fn readable(f: wl_shm::Format) -> Option<SourceFormat> {
    use wl_shm::Format as F;
    Some(match f {
        F::Argb8888 | F::Xrgb8888 => SourceFormat::Bgra8,
        F::Abgr8888 | F::Xbgr8888 => SourceFormat::Rgba8,
        F::Xrgb2101010 | F::Argb2101010 => SourceFormat::Xrgb2101010,
        F::Xbgr2101010 | F::Abgr2101010 => SourceFormat::Xbgr2101010,
        _ => return None,
    })
}

impl Wayland {
    /// Dispatch until `done` says so, or fail after `TIMEOUT`.
    fn wait_for(&mut self, what: &str, done: impl Fn(&State) -> bool) -> Result<()> {
        let start = Instant::now();
        while !done(&self.state) {
            let left = TIMEOUT.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Err(Error::Unavailable(format!("{what} timed out")));
            }
            self.wait(Some(left))?;
        }
        Ok(())
    }

    /// A window's own pixels, by its toplevel identifier.
    pub fn capture_window(&mut self, identifier: &str) -> Result<Image> {
        let qh = self.qh();
        let unavailable = |what: &str| Error::Unavailable(format!("window capture needs {what}"));
        let manager: ExtImageCopyCaptureManagerV1 = self
            .globals
            .bind(&qh, 1..=1, NoEvents)
            .map_err(|_| unavailable("ext-image-copy-capture"))?;
        let sources: ExtForeignToplevelImageCaptureSourceManagerV1 = self
            .globals
            .bind(&qh, 1..=1, NoEvents)
            .map_err(|_| unavailable("ext-foreign-toplevel-image-capture-source"))?;
        self.state.toplevels = Some(ForeignToplevelList::new(&self.globals, &qh));
        // Two round trips: the toplevels, then their details.
        for _ in 0..2 {
            self.queue
                .roundtrip(&mut self.state)
                .map_err(|e| Error::Protocol(format!("roundtrip: {e}")))?;
        }
        let list = self.state.toplevels.as_ref().expect("just bound");
        let handle = list
            .toplevels()
            .iter()
            .find(|t| list.info(t).is_some_and(|i| i.identifier == identifier))
            .cloned()
            .ok_or_else(|| Error::Unavailable(format!("no window with identifier {identifier}")))?;

        self.state.window = Some(WindowCapture::default());
        let source = sources.create_source(&handle, &qh, NoEvents);
        let options = if self.state.cursor {
            Options::PaintCursors
        } else {
            Options::empty()
        };
        let session = manager.create_session(&source, options, &qh, WindowData);
        self.wait_for("describing the window", |s| {
            s.window
                .as_ref()
                .is_some_and(|w| w.described || w.failed.is_some())
        })?;
        let w = self.state.window.as_ref().expect("set above");
        if let Some(why) = &w.failed {
            return Err(Error::Unavailable(why.clone()));
        }
        let (width, height) = w
            .size
            .ok_or_else(|| Error::Protocol("no buffer size".into()))?;
        let (wl_format, format) = w
            .formats
            .iter()
            .find_map(|&f| readable(f).map(|sf| (f, sf)))
            .ok_or_else(|| Error::Protocol("no shm format we can read".into()))?;
        let stride = width * 4;
        let mut pool = RawPool::new((stride * height) as usize, &self.state.shm)
            .map_err(|e| Error::Protocol(format!("shm pool: {e}")))?;
        let buffer: WlBuffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_format,
            NoEvents,
            &qh,
        );
        let frame = session.create_frame(&qh, WindowData);
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        let result = self.wait_for("capturing the window", |s| {
            s.window
                .as_ref()
                .is_some_and(|w| w.ready || w.failed.is_some())
        });
        let w = self.state.window.take().expect("set above");
        frame.destroy();
        session.destroy();
        source.destroy();
        buffer.destroy();
        result?;
        if let Some(why) = w.failed {
            return Err(Error::Unavailable(why));
        }
        let img = compose::convert(
            &pool.mmap()[..],
            width,
            height,
            stride as usize,
            format,
            false,
        )
        .ok_or_else(|| Error::Protocol("the buffer is too small".into()))?;
        Ok(compose::orient(img, Transform(w.transform)))
    }
}
