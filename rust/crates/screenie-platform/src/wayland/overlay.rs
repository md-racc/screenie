//! The overlays: one layer-shell surface per monitor, keyboard-exclusive,
//! showing whatever the app draws, and forwarding raw input to it.
//!
//! Every overlay is keyboard-exclusive. While an exclusive layer is up,
//! Hyprland routes the pointer only to exclusive layers, so an overlay
//! without the keyboard would never see a click. The focused monitor's is
//! created last, so it should end up with the keyboard.

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::FrameCallbackData,
    reexports::client::{
        Connection, QueueHandle,
        protocol::{wl_pointer, wl_shm, wl_surface},
    },
    reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape,
    seat::{
        keyboard::{KeyEvent, Modifiers},
        pointer::{PointerEvent, PointerEventKind, cursor_shape::CursorShapeManager},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface},
    },
    shm::slot::{Buffer, SlotPool},
};

use screenie_core::geometry::LogicalPoint;
use screenie_core::keys::Mods;
use screenie_core::layout::Layout;
use screenie_core::session::{Button, RawEvent};
use screenie_image::Image;

use super::{State, Wayland, keys};
use crate::{Error, Result};

/// What the overlay host drives. The host knows nothing of screenie's
/// commands: it delivers raw input and asks for pictures.
pub trait OverlayApp {
    /// One raw event, at `now` seconds on the host's monotonic clock.
    fn input(&mut self, ev: RawEvent, now: f64);
    /// Advance held-key repeat. Returns whether the picture changed.
    fn tick(&mut self, now: f64) -> bool;
    /// Whether `tick` should be called (a key is held).
    fn wants_ticks(&self) -> bool;
    /// Whether the session is over: the host then takes the overlays down.
    fn finished(&self) -> bool;
    /// Draw monitor `index` of the layout into `into`, which covers that
    /// monitor at its device scale.
    fn draw(&mut self, index: usize, into: &mut Image);
}

/// How the overlay run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayEnd {
    /// The app finished.
    Finished,
    /// The compositor closed an overlay.
    Closed,
    /// The deadline passed (a safety net for test runs).
    Deadline,
}

struct Surf {
    layer: LayerSurface,
    monitor: usize,
    origin: LogicalPoint,
    /// Logical size, from the configure.
    size: (u32, u32),
    scale: i32,
    configured: bool,
    frame_pending: bool,
    dirty: bool,
    buffer: Option<Buffer>,
    scratch: Image,
}

pub(crate) struct Overlay {
    surfaces: Vec<Surf>,
    pool: SlotPool,
    /// Whether the compositor takes R,G,B,X bytes as they are (else they
    /// are swizzled into XRGB8888).
    rgbx: bool,
    start: Instant,
    mods: Mods,
    events: Vec<(RawEvent, f64)>,
    /// Wheel motion from continuous sources, toward the next step.
    wheel: f64,
    closed: bool,
}

/// Logical pixels of continuous scrolling per wheel step.
const SCROLL_STEP: f64 = 15.0;

/// Repeat ticks this often while a key is held.
const TICK: Duration = Duration::from_millis(8);

impl Overlay {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn push(&mut self, ev: RawEvent) {
        let t = self.now();
        self.events.push((ev, t));
    }

    fn surface(&self, s: &wl_surface::WlSurface) -> Option<&Surf> {
        self.surfaces.iter().find(|x| x.layer.wl_surface() == s)
    }

    pub(crate) fn frame_done(&mut self, s: &wl_surface::WlSurface) {
        if let Some(x) = self.surfaces.iter_mut().find(|x| x.layer.wl_surface() == s) {
            x.frame_pending = false;
        }
    }

    pub(crate) fn configure(&mut self, layer: &LayerSurface, size: (u32, u32)) {
        if let Some(x) = self.surfaces.iter_mut().find(|x| &x.layer == layer) {
            x.size = size;
            x.configured = true;
            x.dirty = true;
        }
    }

    pub(crate) fn closed(&mut self, _layer: &LayerSurface) {
        self.closed = true;
    }

    pub(crate) fn key_repeat(&mut self, (rate, delay): (f64, f64)) {
        self.push(RawEvent::RepeatInfo { rate, delay });
    }

    pub(crate) fn focus_lost(&mut self) {
        self.push(RawEvent::FocusLost);
    }

    pub(crate) fn modifiers(&mut self, m: Modifiers) {
        self.mods = Mods {
            shift: m.shift,
            ctrl: m.ctrl,
            alt: m.alt,
        };
        let mods = self.mods;
        self.push(RawEvent::Modifiers { mods });
    }

    pub(crate) fn key(&mut self, ev: KeyEvent, pressed: bool) {
        let (code, key, mods) = (ev.raw_code, keys::key(ev.keysym), self.mods);
        self.push(if pressed {
            RawEvent::KeyPress { code, key, mods }
        } else {
            RawEvent::KeyRelease { code, key, mods }
        });
    }

    pub(crate) fn pointer(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<State>,
        pointer: &wl_pointer::WlPointer,
        shapes: Option<&CursorShapeManager>,
        ev: &PointerEvent,
    ) {
        let Some(s) = self.surface(&ev.surface) else {
            return;
        };
        let at = LogicalPoint::new(s.origin.x + ev.position.0, s.origin.y + ev.position.1);
        let button = |b: u32| match b {
            0x110 => Button::Left,
            0x111 => Button::Right,
            0x112 => Button::Middle,
            other => Button::Other(other),
        };
        match ev.kind {
            PointerEventKind::Enter { serial } => {
                if let Some(m) = shapes {
                    m.get_shape_device(pointer, qh)
                        .set_shape(serial, Shape::Default);
                }
                self.push(RawEvent::Motion { at });
            }
            PointerEventKind::Leave { .. } => {}
            PointerEventKind::Motion { .. } => self.push(RawEvent::Motion { at }),
            PointerEventKind::Press { button: b, .. } => self.push(RawEvent::ButtonPress {
                button: button(b),
                at,
            }),
            PointerEventKind::Release { button: b, .. } => self.push(RawEvent::ButtonRelease {
                button: button(b),
                at,
            }),
            PointerEventKind::Axis { vertical, .. } => {
                // Wheel up is negative, and up is `+`.
                self.wheel -= if vertical.value120 != 0 {
                    f64::from(vertical.value120) / 120.0
                } else if vertical.discrete != 0 {
                    f64::from(vertical.discrete)
                } else {
                    vertical.absolute / SCROLL_STEP
                };
                let steps = self.wheel.trunc();
                self.wheel -= steps;
                if steps != 0.0 {
                    self.push(RawEvent::Wheel {
                        steps: steps as i32,
                    });
                }
            }
        }
    }

    fn mark_dirty(&mut self) {
        for s in &mut self.surfaces {
            s.dirty = true;
        }
    }

    /// Draw every surface that's due: configured, changed, and not waiting
    /// on a frame callback.
    fn draw_due(&mut self, app: &mut dyn OverlayApp, qh: &QueueHandle<State>) -> Result<()> {
        let format = if self.rgbx {
            wl_shm::Format::Xbgr8888
        } else {
            wl_shm::Format::Xrgb8888
        };
        for s in &mut self.surfaces {
            if !s.configured || !s.dirty || s.frame_pending || s.size.0 == 0 || s.size.1 == 0 {
                continue;
            }
            let (w, h) = (s.size.0 * s.scale as u32, s.size.1 * s.scale as u32);
            if (s.scratch.width(), s.scratch.height()) != (w, h) {
                s.scratch = Image::new(w, h);
                s.buffer = None;
            }
            app.draw(s.monitor, &mut s.scratch);
            let stride = w as i32 * 4;
            if s.buffer.is_none() {
                let (b, _) = self
                    .pool
                    .create_buffer(w as i32, h as i32, stride, format)
                    .map_err(|e| Error::Protocol(format!("shm buffer: {e}")))?;
                s.buffer = Some(b);
            }
            let buffer = s.buffer.as_mut().expect("just made");
            let canvas = match self.pool.canvas(buffer) {
                Some(c) => c,
                None => {
                    // The compositor still holds it: draw into a second one.
                    let (b, c) = self
                        .pool
                        .create_buffer(w as i32, h as i32, stride, format)
                        .map_err(|e| Error::Protocol(format!("shm buffer: {e}")))?;
                    *buffer = b;
                    c
                }
            };
            let src = s.scratch.data();
            if self.rgbx {
                canvas[..src.len()].copy_from_slice(src);
            } else {
                for (d, p) in canvas.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    d.copy_from_slice(&[p[2], p[1], p[0], 255]);
                }
            }
            let surface = s.layer.wl_surface();
            surface.set_buffer_scale(s.scale);
            surface.damage_buffer(0, 0, w as i32, h as i32);
            surface.frame(qh, FrameCallbackData(surface.clone()));
            buffer
                .attach_to(surface)
                .map_err(|e| Error::Protocol(format!("attach: {e}")))?;
            s.layer.commit();
            s.frame_pending = true;
            s.dirty = false;
        }
        Ok(())
    }
}

impl Wayland {
    /// Show an overlay on every monitor of `layout` and run until the app
    /// finishes, the compositor closes an overlay, or `deadline` passes.
    /// The overlays are gone from the screen when this returns.
    pub fn run_overlay(
        &mut self,
        layout: &Layout,
        app: &mut dyn OverlayApp,
        deadline: Option<Duration>,
    ) -> Result<OverlayEnd> {
        let qh = self.qh();
        let shell =
            self.state.layer_shell.as_ref().ok_or_else(|| {
                Error::Unavailable("the compositor has no wlr-layer-shell".into())
            })?;
        let pool = SlotPool::new(1 << 22, &self.state.shm)
            .map_err(|e| Error::Protocol(format!("shm pool: {e}")))?;
        let rgbx = self.state.shm.formats().contains(&wl_shm::Format::Xbgr8888);

        // One surface per monitor, matched to its output by name; the
        // focused monitor's last.
        let mut order: Vec<usize> = (0..layout.monitors.len()).collect();
        order.sort_by_key(|&i| layout.monitors[i].id == layout.focused);
        let outputs: Vec<_> = self.state.outputs.outputs().collect();
        let mut surfaces = Vec::new();
        for i in order {
            let m = &layout.monitors[i];
            let found = outputs.iter().find_map(|o| {
                let info = self.state.outputs.info(o)?;
                (info.name.as_deref() == Some(m.name.as_str())).then(|| (o.clone(), info))
            });
            let Some((output, info)) = found else {
                continue;
            };
            let surface = self.state.compositor.create_surface(&qh);
            let layer = shell.create_layer_surface(
                &qh,
                surface,
                Layer::Overlay,
                Some("screenie"),
                Some(&output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
            layer.set_size(0, 0);
            layer.commit();
            surfaces.push(Surf {
                layer,
                monitor: i,
                origin: LogicalPoint::new(m.rect.x, m.rect.y),
                size: (0, 0),
                scale: info.scale_factor.max(1),
                configured: false,
                frame_pending: false,
                dirty: true,
                buffer: None,
                scratch: Image::new(0, 0),
            });
        }
        if surfaces.is_empty() {
            return Err(Error::Unavailable("no output matched the layout".into()));
        }
        let start = Instant::now();
        self.state.overlay = Some(Overlay {
            surfaces,
            pool,
            rgbx,
            start,
            mods: Mods::NONE,
            events: Vec::new(),
            wheel: 0.0,
            closed: false,
        });
        if let Some(r) = self.state.key_repeat {
            self.state
                .overlay
                .as_mut()
                .expect("just made")
                .key_repeat(r);
        }

        let end = loop {
            let timeout = if app.wants_ticks() {
                Some(TICK)
            } else {
                deadline.map(|d| d.saturating_sub(start.elapsed()))
            };
            self.wait(timeout)?;
            let overlay = self.state.overlay.as_mut().expect("running");
            let mut changed = false;
            for (ev, t) in overlay.events.drain(..) {
                app.input(ev, t);
                changed = true;
            }
            if app.wants_ticks() {
                changed |= app.tick(overlay.now());
            }
            if app.finished() {
                break OverlayEnd::Finished;
            }
            if overlay.closed {
                break OverlayEnd::Closed;
            }
            if deadline.is_some_and(|d| start.elapsed() >= d) {
                break OverlayEnd::Deadline;
            }
            if changed {
                overlay.mark_dirty();
            }
            overlay.draw_due(app, &qh)?;
        };

        // Take the overlays down, and make sure the compositor has.
        self.state.overlay = None;
        self.queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Protocol(format!("roundtrip: {e}")))?;
        Ok(end)
    }
}
