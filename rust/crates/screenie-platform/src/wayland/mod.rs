//! Wayland: one connection for capture (wlr-screencopy) and the overlays
//! (wlr-layer-shell), so the frame is taken before any overlay exists.

mod capture;
mod keys;
mod overlay;
mod window;

pub use overlay::OverlayApp;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::client::{
        Connection, EventQueue, QueueHandle,
        globals::{GlobalList, registry_queue_init},
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    },
    reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo},
        pointer::{PointerEvent, PointerHandler, cursor_shape::CursorShapeManager},
    },
    shell::wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    shm::{Shm, ShmHandler},
};

use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use crate::{Error, Result};

/// A connection to the compositor, with what screenie binds.
pub struct Wayland {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    globals: GlobalList,
}

pub(crate) struct State {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    seat: SeatState,
    layer_shell: Option<LayerShell>,
    screencopy: Option<ZwlrScreencopyManagerV1>,
    cursor_shape: Option<CursorShapeManager>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    capture: Vec<capture::Pending>,
    overlay: Option<overlay::Overlay>,
    /// The keyboard's repeat rate (per second) and delay (seconds).
    key_repeat: Option<(f64, f64)>,
    /// Toplevels, bound only for a window capture.
    toplevels: Option<smithay_client_toolkit::foreign_toplevel_list::ForeignToplevelList>,
    window: Option<window::WindowCapture>,
}

/// The empty user data for globals with no events of interest.
pub(crate) struct NoEvents;

impl Wayland {
    /// Connect and bind everything screenie might use. Outputs are known
    /// (with their names) when this returns.
    pub fn connect() -> Result<Self> {
        let conn = Connection::connect_to_env()
            .map_err(|e| Error::Unavailable(format!("no Wayland display: {e}")))?;
        let (globals, mut queue): (GlobalList, _) =
            registry_queue_init(&conn).map_err(|e| Error::Protocol(format!("registry: {e}")))?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|e| Error::Unavailable(format!("wl_compositor: {e}")))?;
        let shm =
            Shm::bind(&globals, &qh).map_err(|e| Error::Unavailable(format!("wl_shm: {e}")))?;
        let mut state = State {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seat: SeatState::new(&globals, &qh),
            layer_shell: LayerShell::bind(&globals, &qh).ok(),
            screencopy: globals.bind(&qh, 1..=3, NoEvents).ok(),
            cursor_shape: CursorShapeManager::bind(&globals, &qh).ok(),
            compositor,
            shm,
            keyboard: None,
            pointer: None,
            capture: Vec::new(),
            overlay: None,
            key_repeat: None,
            toplevels: None,
            window: None,
        };
        queue
            .roundtrip(&mut state)
            .map_err(|e| Error::Protocol(format!("roundtrip: {e}")))?;
        Ok(Self {
            conn,
            queue,
            state,
            globals,
        })
    }

    fn qh(&self) -> QueueHandle<State> {
        self.queue.handle()
    }

    /// Wait for the connection to have events, at most `timeout`.
    fn wait(&mut self, timeout: Option<Duration>) -> Result<()> {
        use smithay_client_toolkit::reexports::client::backend::WaylandError;
        let proto = |e: &dyn std::fmt::Display| Error::Protocol(e.to_string());
        // On the non-blocking socket, "would block" means "not yet": a
        // flush that couldn't send everything waits for the socket to
        // drain, and a read with nothing there is nothing to do.
        let would_block = |e: &WaylandError| matches!(e, WaylandError::Io(io) if io.kind() == std::io::ErrorKind::WouldBlock);
        let unsent = match self.queue.flush() {
            Ok(()) => false,
            Err(e) if would_block(&e) => true,
            Err(e) => return Err(proto(&e)),
        };
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| proto(&e))?;
        if let Some(guard) = self.queue.prepare_read() {
            let ready = {
                let fd = guard.connection_fd();
                let mut flags = PollFlags::IN | PollFlags::ERR;
                if unsent {
                    flags |= PollFlags::OUT;
                }
                let mut fds = [PollFd::new(&fd, flags)];
                let ts = timeout.map(|d| Timespec {
                    tv_sec: d.as_secs() as _,
                    tv_nsec: d.subsec_nanos() as _,
                });
                match poll(&mut fds, ts.as_ref()) {
                    Ok(n) => n > 0 && fds[0].revents().intersects(PollFlags::IN | PollFlags::ERR),
                    Err(rustix::io::Errno::INTR) => false,
                    Err(e) => return Err(Error::Io(e.into())),
                }
            };
            if ready {
                match guard.read() {
                    Ok(_) => {}
                    Err(e) if would_block(&e) => {}
                    Err(e) => return Err(proto(&e)),
                }
            }
        }
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| proto(&e))?;
        Ok(())
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(State);
delegate_dispatch2!(State);

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, s: &wl_surface::WlSurface, _: u32) {
        if let Some(o) = &mut self.overlay {
            o.frame_done(s);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(o) = &mut self.overlay {
            o.closed(layer);
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(o) = &mut self.overlay {
            o.configure(layer, configure.new_size);
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(o) = &mut self.overlay {
            o.focus_lost();
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if let Some(o) = &mut self.overlay {
            o.key(event, true);
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
        // screenie does its own repeat.
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if let Some(o) = &mut self.overlay {
            o.key(event, false);
        }
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        self.repeat_info(info);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        if let Some(o) = &mut self.overlay {
            o.modifiers(modifiers);
        }
    }
}

impl State {
    fn repeat_info(&mut self, info: RepeatInfo) {
        let r = match info {
            RepeatInfo::Repeat { rate, delay } => {
                (f64::from(rate.get()), f64::from(delay) / 1000.0)
            }
            RepeatInfo::Disable => (0.0, 0.0),
        };
        self.key_repeat = Some(r);
        if let Some(o) = &mut self.overlay {
            o.key_repeat(r);
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        if let Some(o) = &mut self.overlay {
            for ev in events {
                o.pointer(conn, qh, pointer, self.cursor_shape.as_ref(), ev);
            }
        }
    }
}
