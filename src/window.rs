//! The live rendering window.
//!
//! Single-threaded and step-driven, as planned in `ARCHITECTURE.md`: the
//! winit event loop owns the interpreter, runs a budget of interpreter
//! steps per frame, then blits the canvas. No locks, no channels, and
//! pausing/slowing execution is just a smaller budget.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::Interp;

const FRAME: Duration = Duration::from_millis(16);

pub struct WindowOptions {
    pub title: String,
    /// Interpreter steps (executed objects) per frame — the "watchability"
    /// knob. ~60 frames/second, so 100 steps/frame ≈ 6000 objects/second.
    pub steps_per_frame: usize,
}

/// Run `interp` (which must already have a program queued via
/// `begin_source`) inside a live window. Returns after the window closes;
/// `Err` carries a message suitable for stderr.
pub fn run_windowed(interp: Interp, options: WindowOptions) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("cannot create event loop: {e}"))?;
    let mut app = App {
        interp,
        options,
        view: None,
        running: true,
        had_error: false,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    if app.had_error {
        return Err("program stopped with an error (canvas left on screen)".to_string());
    }
    Ok(())
}

struct View {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    interp: Interp,
    options: WindowOptions,
    view: Option<View>,
    running: bool,
    had_error: bool,
}

impl App {
    fn tick(&mut self) {
        if !self.running {
            return;
        }
        match self.interp.step_n(self.options.steps_per_frame) {
            Ok(true) => {}
            Ok(false) => self.finish(" — done"),
            Err(e) => {
                eprintln!("{}", self.interp.error_report(&e));
                self.had_error = true;
                self.finish(" — error (see terminal)");
            }
        }
    }

    fn finish(&mut self, suffix: &str) {
        self.running = false;
        if let Some(view) = &self.view {
            view.window
                .set_title(&format!("{}{}", self.options.title, suffix));
        }
    }

    fn blit(&mut self) {
        let Some(view) = &mut self.view else { return };
        let size = view.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return; // minimized
        };
        if view.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = view.surface.buffer_mut() else {
            return;
        };

        let pixmap = &self.interp.gfx().pixmap;
        let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
        let data = pixmap.data();
        let (bw, bh) = (size.width as usize, size.height as usize);
        // Nearest-neighbor scale from the page pixmap to the physical
        // window buffer (handles HiDPI's 2x factor adequately for now).
        for dy in 0..bh {
            let sy = (dy * ph / bh).min(ph - 1);
            let row = &data[sy * pw * 4..(sy + 1) * pw * 4];
            let out = &mut buffer[dy * bw..(dy + 1) * bw];
            for (dx, px) in out.iter_mut().enumerate() {
                let sx = (dx * pw / bw).min(pw - 1);
                let p = &row[sx * 4..sx * 4 + 4];
                *px = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            }
        }
        let _ = buffer.present();
        self.interp.gfx_mut().dirty = false;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        let (pw, ph) = {
            let pm = &self.interp.gfx().pixmap;
            (pm.width(), pm.height())
        };
        let attrs = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(LogicalSize::new(pw, ph));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("pscat: cannot create window: {e}");
                event_loop.exit();
                return;
            }
        };
        let surface =
            Context::new(window.clone()).and_then(|ctx| Surface::new(&ctx, window.clone()));
        match surface {
            Ok(surface) => {
                window.request_redraw();
                self.view = Some(View { window, surface });
            }
            Err(e) => {
                eprintln!("pscat: cannot create render surface: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.tick();
                self.blit();
                if self.running {
                    // ~60fps cadence while the program runs; idle after.
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.running
            && let Some(view) = &self.view
        {
            view.window.request_redraw();
        }
    }
}
