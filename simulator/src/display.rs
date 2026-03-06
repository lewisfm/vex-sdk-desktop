use std::{mem, num::NonZeroU32, rc::Rc, sync::LazyLock, time::Instant};

use parking_lot::{Condvar, Mutex};
use tracing::{debug, trace};
use vex_sdk::{V5_TouchEvent, V5_TouchStatus};

use crate::{
    canvas::{BUFSZ, CANVAS, Canvas, HEIGHT, Point, Rect, WIDTH, img::SimImage},
};

/// The shared V5 display instance.
///
/// Responsible for managing the actual display data shown to the user including the program header
/// and rendered canvas. Compared to [`CANVAS`], this static holds completed frames rather than
/// work-in-progress ones. It is updated at 60Hz by the output method.
pub static DISPLAY: LazyLock<Mutex<SimDisplay>> = LazyLock::new(|| Mutex::new(SimDisplay::new()));

/// Notification sent when a new frame is has been rendered to the [`DISPLAY`] _and_ recorded to
/// the active output method.
///
/// Listening to this notification enables waiting for at least one frame to be committed
/// before making further changes to the display, enabling a VSync effect.
pub static FRAME_FINISHED: Condvar = Condvar::new();

/// Icon of a V5 brain.
static DEVICE_IMAGE: LazyLock<SimImage> =
    LazyLock::new(|| SimImage::from_png(include_bytes!("../assets/brain.png")));

/// The shared state for a simulated display.
pub struct SimDisplay {
    pub buffer: [u32; BUFSZ],

    /// Indicates whether the header canvas should be hidden from the display, expanding the user
    /// canvas mask to the full contents of the window.
    pub header_hidden: bool,

    /// Indicates whether redraws should automatically render the user canvas without calls to
    /// [`vexDisplayRender`](crate::sdk::vexDisplayRender).
    pub autorender: bool,

    /// Used for drawing the program header.
    ///
    /// This is effectively drawn on a separate layer from the default user canvas.
    system_canvas: Option<Box<Canvas>>,
    program_start: Instant,
    program_display_name: String,

    pub mouse_down: bool,
    pub mouse_coords: Point,

    pub touch: V5_TouchStatus,
}

impl SimDisplay {
    pub fn new() -> Self {
        debug!("Initializing shared display");

        Self {
            buffer: [0; _],
            header_hidden: false,
            autorender: true,
            mouse_coords: Point::new(0, 0),
            system_canvas: Some(Canvas::new().into()),
            program_display_name: String::new(),
            program_start: Instant::now(),
            mouse_down: false,
            touch: V5_TouchStatus {
                lastEvent: V5_TouchEvent::kTouchEventRelease,
                lastXpos: 0,
                lastYpos: 0,
                pressCount: 0,
                releaseCount: 0,
            },
        }
    }

    /// Render a new frame by drawing the display's header and, if autorender is enabled, drawing
    /// the user canvas.
    pub fn render(&mut self) {
        trace!("Rendering frame");
        if self.autorender {
            let canvas = CANVAS.lock();
            self.render_user_canvas(&canvas);
        }

        if !self.header_hidden {
            self.render_header();
        }
    }

    /// Copy a rectangle of pixels from the source onto the display.
    pub fn blit_rect(&mut self, source: &[u32; BUFSZ], mut mask: Rect) {
        mask.clip_to(Rect::FULL_CLIP);

        for pixel in mask.pixels() {
            let idx = (pixel.y * WIDTH as i32 + pixel.x) as usize;
            self.buffer[idx] = source[idx];
        }
    }

    /// Copy the given canvas onto the display using the correct mask for the global user canvas.
    pub fn render_user_canvas(&mut self, canvas: &Canvas) {
        let mask = if self.header_hidden {
            Rect::FULL_CLIP
        } else {
            Rect::USER_CLIP
        };
        self.blit_rect(canvas.buffer(), mask);
    }

    /// Set the program name shown in the display header.
    pub fn set_program_name(&mut self, name: &str) {
        let display_name = if name.len() > 15 {
            format!("{}...", &name[0..=12])
        } else {
            name.to_string()
        };

        self.program_display_name = display_name;
    }

    /// Get the current touch status of the display.
    pub fn touch(&self) -> V5_TouchStatus {
        self.touch
    }

    /// Runs a callback after the previous frame, then waits for the next frame to be committed.
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
        FRAME_FINISHED.wait(&mut frame);
        ret
    }

    /// Draw the program header on the header canvas.
    fn render_header(&mut self) {
        let mut canvas = self.system_canvas.take().unwrap();

        canvas.state.fg_color = 0x00_99_CC;
        canvas.fill_rect(Rect::HEADER_CLIP);

        canvas.state.fg_color = 0x00_00_00;

        canvas.state.set_named_font("proportional");
        canvas.state.font_scale = (2, 5);
        canvas.draw_string(Point::new(8, 0), &self.program_display_name, false);

        let elapsed = self.program_start.elapsed().as_secs();
        let minutes = elapsed / 60;
        let seconds = elapsed % 60;
        let elapsed_time = format!("{minutes}:{seconds:02}");

        canvas.state.set_named_font("NotoMono_39pt");
        canvas.state.font_scale = (3, 5);
        canvas.draw_string(Point::new(246, 3), &elapsed_time, false);

        let device = &*DEVICE_IMAGE;
        let device_coords = Point::new(WIDTH as i32 - device.width() - 4, -1);
        DEVICE_IMAGE.draw(&mut canvas, device_coords);

        let battery = Rect::sized(452, 23, 13, 9);
        canvas.state.fg_color = 0x93_C8_3F;
        canvas.fill_rect(battery);

        canvas.state.fg_color = 0x00_00_00;
        canvas.draw_rect(battery);
        canvas.fill_rect(Rect::sized(battery.left() + 13, battery.top() + 3, 2, 3));

        self.blit_rect(canvas.buffer(), Rect::HEADER_CLIP);
        self.system_canvas = Some(canvas);
    }
}

/// Access to the underlying pixel buffer.
impl AsRef<[u32]> for SimDisplay {
    fn as_ref(&self) -> &[u32] {
        &self.buffer
    }
}
