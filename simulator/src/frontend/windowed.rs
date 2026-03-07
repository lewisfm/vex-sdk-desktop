//! Display renderer implementation which writes data to a GUI window.

use std::{
    mem,
    num::NonZeroU32,
    path::Path,
    rc::Rc,
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow};
use fast_image_resize::{
    ResizeAlg, ResizeOptions, Resizer,
    images::{TypedImage, TypedImageRef},
    pixels::U8x4,
};
use parking_lot::{Condvar, Mutex};
use softbuffer::{Context, Surface};
use tracing::{debug, error, trace};
use vex_sdk::{V5_TouchEvent, V5_TouchStatus};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    window::{Theme, Window, WindowId},
};

use crate::{
    canvas::{BUFSZ, CANVAS, Canvas, HEIGHT, Point, Rect, WIDTH, img::SimImage},
    display::{DISPLAY, FRAME_FINISHED},
};

#[cfg(target_os = "macos")]
mod macos;

const WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(480.0, 272.0);

type DisplayCtx = Context<OwnedDisplayHandle>;

pub fn start(name: &str, on_ready: impl FnOnce() + Send + 'static) -> Result<()> {
    let event_loop = EventLoop::with_user_event().build().unwrap();

    let display = event_loop.owned_display_handle();
    let mut simulator = SimulatorApp::new(name.to_string(), display, on_ready)?;
    event_loop.run_app(&mut simulator)?;

    Ok(())
}

pub struct SimulatorApp<E> {
    sim_display: Option<SimDisplayWindow>,
    context: DisplayCtx,
    entrypoint: Option<E>,
    last_frame_time: Option<Instant>,
    program_name: String,
}

impl<E: FnOnce() + Send + 'static> SimulatorApp<E> {
    fn new(name: String, display: OwnedDisplayHandle, on_ready: E) -> Result<Self> {
        let context = DisplayCtx::new(display)
            .map_err(|e| anyhow!(e.to_string()))
            .context("Failed to create display rendering context")?;

        Ok(Self {
            sim_display: None,
            context,
            entrypoint: Some(on_ready),
            last_frame_time: None,
            program_name: name,
        })
    }

    fn schedule_render(&mut self, event_loop: &ActiveEventLoop, last_render: Instant) {
        let frame_period = Duration::from_secs(1) / 60;
        let now = Instant::now();

        let mut next_render = last_render + frame_period;
        if next_render < now {
            next_render = now + frame_period;
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(next_render));
    }
}

impl<E: FnOnce() + Send + 'static> ApplicationHandler<()> for SimulatorApp<E> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.sim_display.is_none() {
            match SimDisplayWindow::open(event_loop, &self.context, &self.program_name) {
                Ok(sim_display) => self.sim_display = Some(sim_display),
                Err(error) => error!(%error, "Failed to open VEX V5 Display window"),
            }
        }

        if let Some(run_app) = self.entrypoint.take() {
            thread::spawn(run_app);
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            StartCause::Init => {
                // Start a timer for rendering the display at 60 fps.
                self.schedule_render(event_loop, Instant::now());
            }
            StartCause::ResumeTimeReached {
                requested_resume, ..
            } => {
                // 60Hz render timer has triggered, so render a frame.
                self.schedule_render(event_loop, requested_resume);

                let now = Instant::now();
                if let Some(last) = self.last_frame_time.replace(now) {
                    trace!(measured_period = ?now - last, "Frame time");
                }

                if let Some(d) = &mut self.sim_display {
                    d.queue_redraw();
                }
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(sim_display) = &mut self.sim_display
            && window_id == sim_display.window_id()
        {
            sim_display.handle_event(event_loop, event);
        }
    }
}

/// A simulated VEX V5 display.
pub struct SimDisplayWindow {
    window: Rc<Window>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,

    scale_factor: f64,

    // A frame has been explicitly requested by the app; the next redraw should autorender the
    // canvas, update the program header, notify vexDisplayRender callers, etc. instead of just
    // scaling the previous rendered frame.
    has_scheduled_frame: bool,
}

impl SimDisplayWindow {
    pub fn open(event_loop: &ActiveEventLoop, context: &DisplayCtx, name: &str) -> Result<Self> {
        debug!("Opening V5 display window");

        #[cfg(target_os = "macos")]
        self::macos::init_app();

        let attrs = Window::default_attributes()
            .with_resizable(false)
            .with_min_inner_size(WINDOW_SIZE)
            .with_inner_size(WINDOW_SIZE)
            .with_theme(Some(Theme::Dark))
            .with_title(format!("VEX V5 Simulator (Program: {name})"));

        let window = Rc::new(event_loop.create_window(attrs)?);

        #[cfg(target_os = "macos")]
        {
            window.set_resizable(true);
            self::macos::notify_aspect_ratio(&window);
        }

        let surface = Surface::new(context, window.clone())
            .map_err(|e| anyhow!(e.to_string()))
            .context("Failed to create V5 display rendering surface")?;

        DISPLAY.lock().set_program_name(name);

        Ok(Self {
            surface,
            window,
            scale_factor: 1.0,
            has_scheduled_frame: true,
        })
    }

    /// Handle an event sent to this window.
    pub fn handle_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            WindowEvent::Resized(_) => {
                // Tell the window manager that we have a certain aspect ratio set if possible.
                // This makes dragging the left side of the window resize properly instead of
                // just shifting the window to the left.
                #[cfg(target_os = "macos")]
                self::macos::notify_aspect_ratio(&self.window);

                // Maintain the proper aspect ratio.
                let dims = self.window.inner_size();
                let mut fb_dims = dims;

                let current_aspect_ratio = dims.width as f64 / dims.height as f64;
                let desired_aspect_ratio = WINDOW_SIZE.width / WINDOW_SIZE.height;

                if current_aspect_ratio > desired_aspect_ratio {
                    fb_dims.width = (desired_aspect_ratio * dims.height as f64) as u32;
                } else {
                    fb_dims.height = (1.0 / desired_aspect_ratio * dims.width as f64) as u32;
                }

                if dims != fb_dims && !self.window.is_maximized() {
                    _ = self.window.request_inner_size(fb_dims);
                }

                self.scale_factor = WINDOW_SIZE.width / fb_dims.width as f64;

                // Scale the framebuffer to the window.
                self.surface
                    .resize(
                        NonZeroU32::new(fb_dims.width).unwrap(),
                        NonZeroU32::new(fb_dims.height).unwrap(),
                    )
                    .unwrap();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let mut display = DISPLAY.lock();
                display.mouse_coords = Point {
                    x: (position.x * self.scale_factor) as i32,
                    y: (position.y * self.scale_factor) as i32,
                };
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let mut display = DISPLAY.lock();
                display.mouse_down = state == ElementState::Pressed;
            }
            _ => {}
        }
    }

    pub fn queue_redraw(&mut self) {
        self.has_scheduled_frame = true;
        self.window.request_redraw();
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Scale the display's contents to the size of the window, then write them to the framebuffer.
    pub fn redraw(&mut self) {
        let mut disp = DISPLAY.lock();

        let is_scheduled = mem::take(&mut self.has_scheduled_frame);

        // Only do updates on 60fps frames to maintain hardware FPS simulation
        if is_scheduled {
            disp.render();
        }

        let mut framebuffer = self.surface.buffer_mut().unwrap();
        let width = framebuffer.width().get();
        let height = framebuffer.height().get();

        trace!(
            fb.width = width,
            fb.height = height,
            autorender = disp.autorender,
            "Drawing the VEX V5 display to framebuffer"
        );

        // Scale the contents to the window size so the entire thing is filled.
        // The destination of the scaled image is the framebuffer itself.

        let buffer_pixels: &[U8x4] = bytemuck::must_cast_slice(disp.as_ref());
        let fb_pixels: &mut [U8x4] = bytemuck::must_cast_slice_mut(&mut framebuffer);

        let screen = TypedImageRef::new(WIDTH, HEIGHT, buffer_pixels).unwrap();
        let mut fb_image = TypedImage::from_pixels_slice(width, height, fb_pixels).unwrap();

        let mut resizer = Resizer::new();
        resizer
            .resize_typed::<U8x4>(
                &screen,
                &mut fb_image,
                &ResizeOptions::new()
                    .resize_alg(ResizeAlg::Nearest)
                    .use_alpha(false),
            )
            .unwrap();

        // Only notify on 60fps frames so vexDisplayRender with bVsyncWait doesn't run too quickly.
        if is_scheduled {
            FRAME_FINISHED.notify_all();
        }

        // Unlock after sending the frame notification because locking this mutex should ensure that
        // any subsequent FRAME_NOTIFY notification includes the most recent changes to sim_buffer.
        drop(disp);

        // Swap buffers.
        self.window.pre_present_notify();
        framebuffer.present().unwrap();
    }
}
