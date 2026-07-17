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
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::Interp;
use crate::spool::Watcher;

const FRAME: Duration = Duration::from_millis(16);
/// How often spool mode looks at its directory while idle. Also the
/// spacing between the two sightings a new file must survive, so it
/// doubles as the settling time for files still being copied in.
const POLL: Duration = Duration::from_millis(400);

pub struct WindowOptions {
    pub title: String,
    /// Interpreter steps (executed objects) per frame — the "watchability"
    /// knob. ~60 frames/second, so 100 steps/frame ≈ 6000 objects/second.
    pub steps_per_frame: usize,
    /// Screen the canvas like a mono laser printer (`--halftone`).
    pub halftone: bool,
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
        back: 0,
        spool: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    if app.had_error {
        return Err("program stopped with an error (canvas left on screen)".to_string());
    }
    Ok(())
}

/// Spool mode: the window idles like the printer in the corner of the
/// lab, polling `watcher`'s directory, and prints each job that lands
/// there in a fresh interpreter (jobs must not poison each other).
/// Runs until the window closes; job errors go to stderr and printing
/// continues, so they don't fail the session.
pub fn run_spool(
    watcher: Watcher,
    page: (u32, u32),
    scale: f32,
    options: WindowOptions,
) -> Result<(), String> {
    let interp = Interp::with_page_scaled(page.0, page.1, scale)
        .ok_or_else(|| format!("unusable page size {}x{}", page.0, page.1))?;
    let event_loop = EventLoop::new().map_err(|e| format!("cannot create event loop: {e}"))?;
    let base_title = options.title.clone();
    let mut app = App {
        interp,
        options,
        view: None,
        running: false,
        had_error: false,
        back: 0,
        spool: Some(SpoolState {
            watcher,
            page,
            scale,
            base_title,
        }),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    Ok(())
}

struct SpoolState {
    watcher: Watcher,
    page: (u32, u32),
    scale: f32,
    /// Title while idle; jobs append their file name to it.
    base_title: String,
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
    /// Page navigation (arrow keys): how many pages *back* from the
    /// live canvas is being viewed. 0 = the live canvas itself.
    back: usize,
    /// Present in spool mode; the interpreter is replaced per job.
    spool: Option<SpoolState>,
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

    /// Idle-time spool work: check the directory, start the next job.
    /// One job at a time — anything else queued waits its turn.
    fn poll_spool(&mut self) {
        if self.running {
            return;
        }
        let (page, scale, base_title) = {
            let Some(spool) = &mut self.spool else { return };
            if let Err(e) = spool.watcher.poll() {
                eprintln!(
                    "pscat: spool: cannot read {}: {e}",
                    spool.watcher.dir().display()
                );
                return;
            }
            (spool.page, spool.scale, spool.base_title.clone())
        };
        loop {
            let Some(job) = self.spool.as_mut().and_then(|s| s.watcher.next_job()) else {
                return;
            };
            let source = match std::fs::read(&job) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("pscat: spool: cannot read {}: {e}", job.display());
                    continue; // skip this job, try the next
                }
            };
            let Some(mut interp) = Interp::with_page_scaled(page.0, page.1, scale) else {
                return; // page size was validated at startup; unreachable in practice
            };
            interp.begin_source(&source);
            let name = job
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| job.display().to_string());
            self.options.title = format!("{base_title} — {name}");
            self.interp = interp;
            self.running = true;
            self.had_error = false;
            self.back = 0;
            if let Some(view) = &self.view {
                view.window.set_title(&self.options.title);
                view.window.request_redraw();
            }
            return;
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

        let gfx = self.interp.gfx();
        let pixmap = if self.back > 0 {
            let pages = gfx.pages();
            &pages[pages.len() - self.back]
        } else {
            &gfx.pixmap
        };
        let screened;
        let pixmap = if self.options.halftone {
            screened = crate::halftone::screen(pixmap);
            &screened
        } else {
            pixmap
        };
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
                } else if self.spool.is_some() {
                    // Spool mode never truly idles: keep the poll
                    // timer armed for the next job to land.
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            }
            // Left/Right browse completed pages; the live canvas is
            // the newest "page".
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(key @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                let npages = self.interp.gfx().pages().len();
                self.back = match key {
                    NamedKey::ArrowLeft => (self.back + 1).min(npages),
                    _ => self.back.saturating_sub(1),
                };
                if let Some(view) = &self.view {
                    let total = npages + 1;
                    let showing = total - self.back;
                    view.window
                        .set_title(&format!("{} — page {showing}/{total}", self.options.title));
                    view.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.running {
            if let Some(view) = &self.view {
                view.window.request_redraw();
            }
            return;
        }
        if self.spool.is_some() {
            self.poll_spool();
            if self.running {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            } else {
                // Re-arm the timer each pass: a stored WaitUntil
                // instant in the past would spin the loop.
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
            }
        }
    }
}
