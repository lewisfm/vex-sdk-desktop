use std::{
    fmt::{self, Formatter},
    mem,
    ops::RangeInclusive,
    sync::atomic::AtomicBool,
};

use line_drawing::BresenhamCircle;
use parking_lot::Mutex;
use tracing::{debug, trace};

use crate::{
    SIM_APP, SimEvent,
    display::{SimDisplay, SimDisplayBuf},
};

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 272;
pub const HEADER_HEIGHT: u32 = 32;
pub const BUFSZ: usize = WIDTH as usize * HEIGHT as usize;

pub const DEFAULT_BG_COLOR: u32 = 0x00_00_00;
pub const DEFAULT_FG_COLOR: u32 = 0xFF_FF_FF;
pub const HEADER_COLOR: u32 = 0x00_99_CC;

/// The canvas instance used by user code.
pub static CANVAS: Mutex<Canvas> = Mutex::new(Canvas::new());

/// Indicates whether the render thread should automatically render the [user canvas](CANVAS).
pub static AUTORENDER: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Copy)]
pub struct CanvasState {
    pub fg_color: u32,
    pub bg_color: u32,
    clip_region: Rect,
}

impl CanvasState {
    pub fn swap_colors(&mut self) {
        mem::swap(&mut self.fg_color, &mut self.bg_color);
    }

    pub fn set_clip_region(&mut self, mut region: Rect) {
        region.clip_to(&Rect::FULL_CLIP);
        self.clip_region = region;
    }
}

pub struct Canvas {
    back_buffer: [u32; BUFSZ],
    pub state: CanvasState,
    pub saved_state: CanvasState,
}

impl Canvas {
    pub const fn new() -> Self {
        let state = CanvasState {
            fg_color: DEFAULT_FG_COLOR,
            bg_color: DEFAULT_BG_COLOR,
            clip_region: Rect::FULL_CLIP,
        };

        Self {
            back_buffer: [0; _],
            state,
            saved_state: state,
        }
    }

    pub fn save(&mut self) {
        self.saved_state = self.state;
    }

    pub fn restore(&mut self) {
        self.state = self.saved_state;
    }

    pub fn set_pixel(&mut self, point: Point) {
        if !point.is_inside(&self.state.clip_region) {
            return;
        }

        let idx = point.y * WIDTH + point.x;
        self.back_buffer[idx as usize] = self.state.fg_color;

        trace!(color = %Hex(self.state.fg_color), ?point, "update pixel");
    }

    pub fn draw_horizontal_line(&mut self, x_range: RangeInclusive<u32>, y: u32) {
        tracing::info!(?x_range, y, "horizontal line");

        let clip = self.state.clip_region;

        // Is the line above or below the clip region?
        if !(clip.0.y..=clip.1.y).contains(&y) {
            return;
        }

        // Try to clamp to the horizontal clip region or bail out if it's totally out of range.
        let allowed_x_range = clip.0.x..=(clip.1.x - 1);
        let Some(x_range) = clamp_range(x_range, allowed_x_range) else {
            return;
        };

        for x in x_range {
            let idx = y * WIDTH + x;
            self.back_buffer[idx as usize] = self.state.fg_color;
        }
    }

    pub fn draw_vertical_line(&mut self, x: u32, y_range: RangeInclusive<u32>) {
        tracing::info!(x, ?y_range, "vertical line");

        let clip = self.state.clip_region;

        // Is the line left or right of the clip region?
        if !(clip.0.x..=clip.1.x).contains(&x) {
            return;
        }

        // Try to clamp to the vertical clip region or bail out if it's totally out of range.
        let allowed_y_range = clip.0.y..=(clip.1.y - 1);
        let Some(y_range) = clamp_range(y_range, allowed_y_range) else {
            return;
        };

        for y in y_range {
            let idx = y * WIDTH + x;
            self.back_buffer[idx as usize] = self.state.fg_color;
        }
    }

    pub fn fill_rect(&mut self, mut bounds: Rect) {
        trace!(color = %Hex(self.state.fg_color), ?bounds, "fill rect");

        bounds.clip_to(&self.state.clip_region);

        for pixel in bounds.pixels() {
            let idx = pixel.y * WIDTH + pixel.x;
            self.back_buffer[idx as usize] = self.state.fg_color;
        }
    }

    pub fn trace_rect(&mut self, mut bounds: Rect) {
        let horizontal_lines = [bounds.0.y, bounds.1.y];
        let vertical_lines = [bounds.0.x, bounds.1.x];

        for y in horizontal_lines {
            self.draw_horizontal_line(bounds.0.x..=(bounds.1.x - 1), y);
        }

        for x in vertical_lines {
            self.draw_vertical_line(x, bounds.0.y..=(bounds.1.y - 1));
        }
    }

    pub fn fill_circle(&mut self, center: Point, radius: u32) {
        trace!(color = %Hex(self.state.fg_color), ?center, radius, "fill circle");

        // Special case to treat radius zero as a set_pixel call since using Bresenham would just
        // give us an empty iterator.
        if radius == 0 {
            self.set_pixel(center);
        }

        // Turn the circle into a bunch of horizontal lines by using Bresenham's circle
        // algorithm to find the left and right extents of each line.

        // The center point isn't included in the radius, so it gets its own extra line.
        let num_lines = 1 + radius * 2;
        let mut lines = vec![(center.x, center.x); num_lines as usize];

        for (dx, i) in BresenhamCircle::new(0, radius as i32, radius as i32) {
            tracing::info!(dx, i, radius, "circle pixel");
            let x = (center.x as i32 + dx) as u32;

            if dx < 0 {
                lines[i as usize].0 = x;
            } else {
                lines[i as usize].1 = x;
            }
        }

        // Iterate through each line and draw it.
        for (line, (left, right)) in lines.into_iter().enumerate() {
            let y = center.y - radius + line as u32;
            self.draw_horizontal_line(left..=right, y);
        }
    }

    pub fn draw_header(&mut self) {
        self.state.fg_color = HEADER_COLOR;
        self.fill_rect(Rect::new(0, 0, WIDTH, HEADER_HEIGHT));
    }

    /// Signal the render thread to show the updated canvas.
    pub fn dispatch_render(&mut self) {
        trace!("Dispatching render");

        _ = SIM_APP
            .get()
            .expect("Attempted to dispatch render without an active render thread")
            .send_event(SimEvent::Render);
    }

    pub fn buffer(&self) -> &[u32; BUFSZ] {
        &self.back_buffer
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    fn clamp_to(&mut self, region: &Rect) {
        self.x = self.x.clamp(region.0.x, region.1.x - 1);
        self.y = self.y.clamp(region.0.y, region.1.y - 1);
    }

    fn is_inside(&self, region: &Rect) -> bool {
        (region.0.x..region.1.x).contains(&self.x) && (region.0.y..region.1.y).contains(&self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect(pub Point, pub Point);

impl Rect {
    pub const FULL_CLIP: Self = Rect::new(0, 0, WIDTH, HEIGHT);
    pub const USER_CLIP: Self = Rect::new(0, HEADER_HEIGHT, WIDTH, HEIGHT);
    pub const HEADER_CLIP: Self = Rect::new(0, 0, WIDTH, HEADER_HEIGHT);

    pub const fn new(mut x0: u32, mut y0: u32, mut x1: u32, mut y1: u32) -> Self {
        if x0 > x1 {
            mem::swap(&mut x0, &mut x1);
        }
        if y0 > y1 {
            mem::swap(&mut y0, &mut y1);
        }

        Self(Point { x: x0, y: y0 }, Point { x: x1, y: y1 })
    }

    pub fn from_sdk(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let mut rect = Self::new(
            i32::max(x0, 0) as u32,
            i32::max(y0 + HEADER_HEIGHT as i32, 0) as u32,
            i32::max(x1, 0) as u32,
            i32::max(y1 + HEADER_HEIGHT as i32, 0) as u32,
        );
        rect.1.x += 1;
        rect.1.y += 1;
        rect
    }

    pub fn clip_to(&mut self, region: &Rect) {
        self.0.clamp_to(region);
        self.1.clamp_to(region);
    }

    pub fn pixels(&self) -> impl Iterator<Item = Point> {
        (self.0.x..self.1.x).flat_map(|x| (self.0.y..self.1.y).map(move |y| Point { x, y }))
    }
}

struct Hex(u32);
impl std::fmt::Display for Hex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "#{:x?}", self.0)
    }
}

/// Clamps `source` to the range `region`, or returns `None` if source is completely outside
/// `region`.
///
/// # Panics
///
/// Panics if `start` < `end` for either `source` or `region`.
fn clamp_range(
    source: RangeInclusive<u32>,
    region: RangeInclusive<u32>,
) -> Option<RangeInclusive<u32>> {
    assert!(source.start() <= source.end());
    assert!(region.start() <= region.end());

    let mut begin = *source.start();
    let mut end = *source.end();

    let region_begin = *region.start();
    let region_end = *region.end();

    if begin > region_end || end < region_begin {
        return None;
    }

    if end >= region_end {
        end = region_end;
    }
    if begin < region_begin {
        begin = region_begin;
    }

    Some(begin..=end)
}
