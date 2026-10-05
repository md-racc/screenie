//! A pin's window: an image floating in an ordinary xdg toplevel (app id
//! `screenie-pin`, which a window rule floats and pins). A drag with the
//! left button moves it; Esc, `q` or the middle button closes it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use smithay_client_toolkit::{
    reexports::client::{
        Connection, QueueHandle,
        protocol::{wl_seat, wl_shm},
    },
    seat::{
        keyboard::{KeyEvent, Keysym},
        pointer::{PointerEvent, PointerEventKind},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::slot::SlotPool,
};

use screenie_image::Image;

use super::{State, Wayland};
use crate::{Error, Result};

/// Linux's button codes.
const BTN_LEFT: u32 = 0x110;
const BTN_MIDDLE: u32 = 0x112;

pub(crate) struct PinWindow {
    window: Window,
    image: Image,
    /// Device pixels a logical pixel (the buffer scale).
    scale: i32,
    pool: SlotPool,
    rgbx: bool,
    drawn: bool,
    pub(crate) closed: bool,
}

impl PinWindow {
    fn draw(&mut self) -> Result<()> {
        let (w, h) = (self.image.width() as i32, self.image.height() as i32);
        let format = if self.rgbx {
            wl_shm::Format::Xbgr8888
        } else {
            wl_shm::Format::Xrgb8888
        };
        let (buffer, canvas) = self
            .pool
            .create_buffer(w, h, w * 4, format)
            .map_err(|e| Error::Protocol(format!("shm buffer: {e}")))?;
        let src = self.image.data();
        if self.rgbx {
            canvas[..src.len()].copy_from_slice(src);
        } else {
            for (d, p) in canvas.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                d.copy_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        let surface = self.window.wl_surface();
        surface.set_buffer_scale(self.scale);
        surface.damage_buffer(0, 0, w, h);
        buffer
            .attach_to(surface)
            .map_err(|e| Error::Protocol(format!("attach: {e}")))?;
        self.window.commit();
        self.drawn = true;
        Ok(())
    }

    pub(crate) fn key(&mut self, event: &KeyEvent) {
        if matches!(event.keysym, Keysym::Escape | Keysym::q | Keysym::Q) {
            self.closed = true;
        }
    }

    pub(crate) fn pointer(&mut self, seat: Option<&wl_seat::WlSeat>, ev: &PointerEvent) {
        if let PointerEventKind::Press { button, serial, .. } = ev.kind {
            match button {
                // The compositor moves it while the button's down.
                BTN_LEFT => {
                    if let Some(seat) = seat {
                        self.window.move_(seat, serial);
                    }
                }
                BTN_MIDDLE => self.closed = true,
                _ => {}
            }
        }
    }
}

impl WindowHandler for State {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        if let Some(p) = &mut self.pin {
            p.closed = true;
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        if let Some(p) = &mut self.pin {
            if !p.drawn {
                if let Err(e) = p.draw() {
                    self.pin_error = Some(e.to_string());
                    p.closed = true;
                }
            }
        }
    }
}

impl Wayland {
    /// Float `image` in a window until it's closed (by the user, the
    /// compositor, or `stop`). `scale` is device pixels a logical pixel, so
    /// it shows at the size it was on screen. `shown` is called once, when
    /// it's on screen (to place it).
    pub fn run_pin(
        &mut self,
        image: Image,
        scale: i32,
        title: &str,
        stop: &AtomicBool,
        mut shown: impl FnMut(),
    ) -> Result<()> {
        let qh = self.qh();
        let xdg = XdgShell::bind(&self.globals, &qh)
            .map_err(|_| Error::Unavailable("the compositor has no xdg-shell".into()))?;
        let pool = SlotPool::new(
            (image.width() * image.height() * 4) as usize,
            &self.state.shm,
        )
        .map_err(|e| Error::Protocol(format!("shm pool: {e}")))?;
        let rgbx = self.state.shm.formats().contains(&wl_shm::Format::Xbgr8888);
        let surface = self.state.compositor.create_surface(&qh);
        let window = xdg.create_window(surface, WindowDecorations::None, &qh);
        window.set_title(title);
        window.set_app_id(crate::pins::APP_ID);
        let scale = scale.max(1);
        let size = (
            (image.width() as i32 / scale).max(1) as u32,
            (image.height() as i32 / scale).max(1) as u32,
        );
        window.set_min_size(Some(size));
        window.set_max_size(Some(size));
        window.commit();
        self.state.pin = Some(PinWindow {
            window,
            image,
            scale,
            pool,
            rgbx,
            drawn: false,
            closed: false,
        });
        let mut announced = false;
        let result = loop {
            if let Err(e) = self.wait(Some(Duration::from_millis(250))) {
                break Err(e);
            }
            if let Some(e) = self.state.pin_error.take() {
                break Err(Error::Protocol(e));
            }
            let Some(p) = &self.state.pin else {
                break Ok(());
            };
            if p.drawn && !announced {
                announced = true;
                shown();
            }
            if p.closed || stop.load(Ordering::Relaxed) {
                break Ok(());
            }
        };
        self.state.pin = None; // the window goes with it
        let _ = self.conn.flush();
        result
    }
}
