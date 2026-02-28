use std::{num::NonZeroU32, rc::Rc};

use anyhow::{Context, Result, anyhow};
use fast_image_resize::{
    ResizeAlg, ResizeOptions, Resizer,
    images::{TypedImage, TypedImageRef},
    pixels::U8x4,
};
use parking_lot::Mutex;
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

static DISPLAY_BUFFER: Mutex<SimDisplayBuf> = Mutex::new(SimDisplayBuf::new());
const SIZE: LogicalSize<f64> = LogicalSize::new(480.0, 272.0);

/// A simulated VEX V5 display.
pub struct SimDisplay {
    window: Rc<Window>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,

    /// Indicates whether redraws will automatically render the user canvas without calls to
    /// [`vexDisplayRender`](crate::sdk::vexDisplayRender).
    autorender: bool,

    /// Used for drawing the program header.
    ///
    /// This is effectively drawn on a separate layer from the default user canvas.
    header_canvas: Canvas,

    /// Indicates whether the header canvas should be drawn to the display.
    fullscreen: bool,
}

impl SimDisplay {
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
            autorender: true,
            header_canvas: Canvas::new(),
            fullscreen: false,
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

    pub fn set_autorender(&mut self, autorender: bool) {
        self.autorender = autorender;
    }

    pub fn queue_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn render_user_canvas(&self, buf: &mut SimDisplayBuf) {
        let canvas = CANVAS.lock();

        let mask = if self.fullscreen {
            Rect::FULL_CLIP
        } else {
            Rect::USER_CLIP
        };
        buf.blit_rect(canvas.buffer(), mask);
    }

    /// Scale the display's contents to the size of the window, then write them to the framebuffer.
    pub fn redraw(&mut self) {
        let mut sim_buffer = DISPLAY_BUFFER.lock();

        if self.autorender {
            self.render_user_canvas(&mut sim_buffer);
        }

        if !self.fullscreen {
            self.header_canvas.draw_header();
            sim_buffer.blit_rect(self.header_canvas.buffer(), Rect::HEADER_CLIP);
        }

        let mut framebuffer = self.surface.buffer_mut().unwrap();
        let width = framebuffer.width().get();
        let height = framebuffer.height().get();

        trace!(
            fb.width = width,
            fb.height = height,
            autorender = self.autorender,
            "Drawing the VEX V5 display to framebuffer"
        );

        // Scale the contents to the window size so the entire thing is filled.
        // The destination of the scaled image is the framebuffer itself.

        let buffer_pixels: &[U8x4] = bytemuck::must_cast_slice(sim_buffer.as_ref());
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

        // Swap buffers.
        self.window.pre_present_notify();
        framebuffer.present().unwrap();
    }
}

/// The buffer for a simulated display.
pub struct SimDisplayBuf {
    buffer: [u32; BUFSZ],
}

impl SimDisplayBuf {
    pub const fn new() -> Self {
        Self { buffer: [0; _] }
    }

    /// Copy a rectangle of pixels from the source onto the display.
    pub fn blit_rect(&mut self, source: &[u32; BUFSZ], mask: Rect) {
        for pixel in mask.pixels() {
            let idx = (pixel.y * WIDTH + pixel.x) as usize;
            self.buffer[idx] = source[idx];
        }
    }
}

impl AsRef<[u32]> for SimDisplayBuf {
    fn as_ref(&self) -> &[u32] {
        &self.buffer
    }
}
