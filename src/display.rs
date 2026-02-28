use std::{mem, num::NonZeroU32, rc::Rc};

use anyhow::{Context, Result, anyhow};
use fast_image_resize::{
    ResizeAlg, ResizeOptions, Resizer,
    images::{TypedImage, TypedImageRef},
    pixels::U8x4,
};
use parking_lot::{Condvar, Mutex};
use softbuffer::Surface;
use tracing::{debug, trace};
use winit::{
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, OwnedDisplayHandle},
    window::{Theme, Window, WindowId},
};

use crate::{
    DisplayCtx,
    canvas::{BUFSZ, CANVAS, Canvas, HEIGHT, Rect, WIDTH},
};

pub static DISPLAY: Mutex<SimDisplay> = Mutex::new(SimDisplay::new());
const SIZE: LogicalSize<f64> = LogicalSize::new(480.0, 272.0);
static FRAME_NOTIFY: Condvar = Condvar::new();

/// A simulated VEX V5 display.
pub struct SimDisplayWindow {
    window: Rc<Window>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,

    /// Used for drawing the program header.
    ///
    /// This is effectively drawn on a separate layer from the default user canvas.
    header_canvas: Canvas,

    // A frame has been explicitly requested by the app; the next redraw should autorender the
    // canvas, update the program header, notify vexDisplayRender callers, etc. instead of just
    // scaling the previous rendered frame.
    has_scheduled_frame: bool,
}

impl SimDisplayWindow {
    pub fn open(event_loop: &ActiveEventLoop, context: &DisplayCtx) -> Result<Self> {
        debug!("Opening V5 display window");

        #[cfg(target_os = "macos")]
        crate::macos::init_app();

        let attrs = Window::default_attributes()
            .with_resizable(false)
            .with_min_inner_size(SIZE)
            .with_inner_size(SIZE)
            .with_theme(Some(Theme::Dark))
            .with_title("VEX V5 Display");

        let window = Rc::new(event_loop.create_window(attrs)?);

        #[cfg(target_os = "macos")]
        {
            window.set_resizable(true);
            crate::macos::notify_aspect_ratio(&window);
        }

        let surface = Surface::new(context, window.clone())
            .map_err(|e| anyhow!(e.to_string()))
            .context("Failed to create V5 display rendering surface")?;

        Ok(Self {
            surface,
            window,
            header_canvas: Canvas::new(),
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
                crate::macos::notify_aspect_ratio(&self.window);

                // Maintain the proper aspect ratio.
                let dims = self.window.inner_size();
                let mut fb_dims = dims;

                let current_aspect_ratio = dims.width as f64 / dims.height as f64;
                let desired_aspect_ratio = SIZE.width / SIZE.height;

                if current_aspect_ratio > desired_aspect_ratio {
                    fb_dims.width = (desired_aspect_ratio * dims.height as f64) as u32;
                } else {
                    fb_dims.height = (1.0 / desired_aspect_ratio * dims.width as f64) as u32;
                }

                if dims != fb_dims && !self.window.is_maximized() {
                    _ = self.window.request_inner_size(fb_dims);
                }

                // Scale the framebuffer to the window.
                self.surface
                    .resize(
                        NonZeroU32::new(fb_dims.width).unwrap(),
                        NonZeroU32::new(fb_dims.height).unwrap(),
                    )
                    .unwrap();
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
            if disp.autorender {
                let canvas = CANVAS.lock();
                disp.render_user_canvas(&canvas);
            }

            if !disp.fullscreen {
                self.header_canvas.draw_header();
                disp.blit_rect(self.header_canvas.buffer(), Rect::HEADER_CLIP);
            }
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
            FRAME_NOTIFY.notify_all();
        }

        // Unlock after sending the frame notification because locking this mutex should ensure that
        // any subsequent FRAME_NOTIFY notification includes the most recent changes to sim_buffer.
        drop(disp);

        // Swap buffers.
        self.window.pre_present_notify();
        framebuffer.present().unwrap();
    }
}

/// The shared state for a simulated display.
pub struct SimDisplay {
    buffer: [u32; BUFSZ],

    /// Indicates whether the header canvas should be hidden from the display, expanding the user
    /// canvas mask to the full contents of the window.
    fullscreen: bool,

    /// Indicates whether redraws should automatically render the user canvas without calls to
    /// [`vexDisplayRender`](crate::sdk::vexDisplayRender).
    autorender: bool,
}

impl SimDisplay {
    pub const fn new() -> Self {
        Self {
            buffer: [0; _],
            fullscreen: false,
            autorender: true,
        }
    }

    /// Copy a rectangle of pixels from the source onto the display.
    pub fn blit_rect(&mut self, source: &[u32; BUFSZ], mask: Rect) {
        for pixel in mask.pixels() {
            let idx = (pixel.y * WIDTH + pixel.x) as usize;
            self.buffer[idx] = source[idx];
        }
    }

    pub fn render_user_canvas(&mut self, canvas: &Canvas) {
        let mask = if self.fullscreen {
            Rect::FULL_CLIP
        } else {
            Rect::USER_CLIP
        };
        self.blit_rect(canvas.buffer(), mask);
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    pub fn set_autorender(&mut self, autorender: bool) {
        self.autorender = autorender;
    }

    /// Runs a callback after the in-progress frame, then waits for the next frame to be committed.
    ///
    /// Any changes made to the display in `cb` are guaranteed to be acknowledged by the
    /// window renderer by the time this function returns (but they might not be visible yet).
    /// Changes made *before* `cb` will also be included, but then it's not guaranteed exactly which
    /// frame after those changes this function is waiting for.
    pub fn run_synced<R>(cb: impl FnOnce(&mut Self) -> R) -> R {
        // Locking the display buffer essentially flushes out any in-progress frame that's operating
        // on old data.
        let mut frame = DISPLAY.lock();
        let ret = cb(&mut frame);
        FRAME_NOTIFY.wait(&mut frame);
        ret
    }
}

impl AsRef<[u32]> for SimDisplay {
    fn as_ref(&self) -> &[u32] {
        &self.buffer
    }
}
