use std::{
    fmt::{self, Formatter},
    mem,
    ops::RangeInclusive,
    sync::{Arc, LazyLock},
};

use color::{ColorSpaceTag, DynamicColor, HueDirection, OpaqueColor, PremulRgba8, Srgb};
use font_kit::{
    canvas::{Canvas as FontCanvas, Format, RasterizationOptions},
    hinting::HintingOptions,
    loaders::freetype::Font,
};
use line_drawing::{Bresenham, BresenhamCircle};
use parking_lot::Mutex;
use pathfinder_geometry::{
    transform2d::Transform2F,
    vector::{Vector2F, Vector2I},
};
use tracing::trace;

use crate::canvas::font::FONTS;

mod font;

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 272;
pub const HEADER_HEIGHT: i32 = 32;
pub const BUFSZ: usize = WIDTH as usize * HEIGHT as usize;

pub const DEFAULT_BG_COLOR: u32 = 0x00_00_00;
pub const DEFAULT_FG_COLOR: u32 = 0xFF_FF_FF;
pub const HEADER_COLOR: u32 = 0x00_99_CC;

/// The canvas instance used by user code.
pub static CANVAS: LazyLock<Mutex<Canvas>> = LazyLock::new(|| Mutex::new(Canvas::new()));

#[derive(Debug, Clone, Copy)]
pub struct CanvasState {
    pub fg_color: u32,
    pub bg_color: u32,
    // This doesn't seem to affect any operations, but we store it so we can return it from
    // `vexDisplayPenSizeGet`.
    pub pen_size: u32,
    clip_region: Rect,
    font_name: &'static str,
    /// Numerator and denominator of post-render scaling of the font.
    pub font_scale: (u32, u32),
}

impl CanvasState {
    pub fn swap_colors(&mut self) {
        mem::swap(&mut self.fg_color, &mut self.bg_color);
    }

    pub fn set_clip_region(&mut self, mut region: Rect) {
        region.clip_to(&Rect::FULL_CLIP);
        self.clip_region = region;
    }

    pub fn clip_region(&self) -> Rect {
        self.clip_region
    }

    pub fn set_named_font(&mut self, name: &str) {
        if let Some((name, _, _)) = FONTS.with(|f| f.get(name)) {
            self.font_name = name;
        }
    }
}

pub struct Canvas {
    buffer: Box<[u32; BUFSZ]>,
    font_buffer: FontCanvas,
    pub state: CanvasState,
    pub saved_state: CanvasState,
}

impl Canvas {
    pub fn new() -> Self {
        let state = CanvasState {
            fg_color: DEFAULT_FG_COLOR,
            bg_color: DEFAULT_BG_COLOR,
            clip_region: Rect::FULL_CLIP,
            pen_size: 1,
            font_name: "monospace",
            font_scale: (1, 3),
        };

        Self {
            // Allocate directly on the heap to prevent a stack overflow.
            buffer: vec![0u32; BUFSZ].into_boxed_slice().try_into().unwrap(),
            font_buffer: FontCanvas::new(Vector2I::new(WIDTH as i32, HEIGHT as i32), Format::A8),
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

        trace!(color = %Hex(self.state.fg_color), ?point, "update pixel");
        self.write_pixel(point, self.state.fg_color);
    }

    fn write_pixel(&mut self, point: Point, color: u32) {
        let idx = point.y * WIDTH as i32 + point.x;
        self.buffer[idx as usize] = color;
    }

    pub fn draw_horizontal_line(&mut self, x_range: RangeInclusive<i32>, y: i32) {
        trace!(?x_range, y, "horizontal line");

        let clip = self.state.clip_region;

        // Is the line above or below the clip region?
        if !(clip.0.y..clip.1.y).contains(&y) {
            return;
        }

        // Try to clamp to the horizontal clip region or bail out if it's totally out of range.
        let allowed_x_range = clip.0.x..=(clip.1.x - 1);
        let Some(x_range) = clamp_range(x_range, allowed_x_range) else {
            return;
        };

        for x in x_range {
            self.write_pixel(Point { x, y }, self.state.fg_color);
        }
    }

    pub fn draw_vertical_line(&mut self, x: i32, y_range: RangeInclusive<i32>) {
        trace!(x, ?y_range, "vertical line");

        let clip = self.state.clip_region;

        // Is the line left or right of the clip region?
        if !(clip.0.x..clip.1.x).contains(&x) {
            return;
        }

        // Try to clamp to the vertical clip region or bail out if it's totally out of range.
        let allowed_y_range = clip.0.y..=(clip.1.y - 1);
        let Some(y_range) = clamp_range(y_range, allowed_y_range) else {
            return;
        };

        for y in y_range {
            self.write_pixel(Point { x, y }, self.state.fg_color);
        }
    }

    pub fn draw_line(&mut self, start: Point, end: Point) {
        trace!(?start, ?end, "line");

        for (x, y) in Bresenham::new((start.x, start.y), (end.x, end.y)) {
            self.set_pixel(Point { x, y });
        }
    }

    pub fn fill_rect(&mut self, mut bounds: Rect) {
        trace!(color = %Hex(self.state.fg_color), ?bounds, "fill rect");

        bounds.clip_to(&self.state.clip_region);

        for pixel in bounds.pixels() {
            self.write_pixel(pixel, self.state.fg_color);
        }
    }

    pub fn trace_rect(&mut self, bounds: Rect) {
        trace!(color = %Hex(self.state.fg_color), ?bounds, "trace rect");

        let horizontal_lines = [bounds.0.y, bounds.1.y - 1];
        let vertical_lines = [bounds.0.x, bounds.1.x - 1];

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
            let x = center.x + dx;

            // The tops and bottoms of circles have several points on the same line, so only record
            // the leftmost or rightmost point for our horizontal line.
            if dx < 0 {
                if x < lines[i as usize].0 {
                    lines[i as usize].0 = x;
                }
            } else if x > lines[i as usize].1 {
                lines[i as usize].1 = x;
            }
        }

        // Iterate through each line and draw it.
        for (line, (left, right)) in lines.into_iter().enumerate() {
            let y = center.y - radius as i32 + line as i32;
            self.draw_horizontal_line(left..=right, y);
        }
    }

    pub fn trace_circle(&mut self, center: Point, radius: u32) {
        trace!(color = %Hex(self.state.fg_color), ?center, radius, "trace circle");

        // Special case to treat radius zero as a set_pixel call since using Bresenham would just
        // give us an empty iterator.
        if radius == 0 {
            self.set_pixel(center);
        }

        let clip = self.state.clip_region;

        for (x, y) in BresenhamCircle::new(center.x, center.y, radius as i32) {
            if (Point { x, y }).is_inside(&clip) {
                self.write_pixel(Point { x, y }, self.state.fg_color);
            }
        }
    }

    pub unsafe fn copy_rect(&mut self, mut bounds: Rect, source: *const u32, stride: usize) {
        trace!(?bounds, ?source, ?stride, "copy rect");
        bounds.clip_to(&self.state.clip_region);

        for (row_idx, row) in (bounds.0.y..bounds.1.y).enumerate() {
            for (col_idx, col) in (bounds.0.x..bounds.1.x).enumerate() {
                let dest_idx = row * WIDTH as i32 + col;
                let source_idx = row_idx * stride + col_idx;
                let pixel = unsafe { source.add(source_idx).read() };
                self.buffer[dest_idx as usize] = pixel;
            }
        }
    }

    pub fn draw_string(&mut self, mut origin: Point, string: &str) {
        let (font_name, font_size, font) = FONTS.with(|f| f.get(self.state.font_name)).unwrap();

        trace!(?string, ?origin, color = %Hex(self.state.fg_color), ?font_name, "Rendering string");

        let replacement_glyph_id = font
            .glyph_for_char('.')
            .expect("Font has '.' character as fallback");

        let metrics = font.metrics();
        let scale = font_size / metrics.units_per_em as f32;

        // Rasterize the pixels
        self.font_buffer.pixels.fill(0);
        let mut translation =
            Vector2F::new(origin.x as f32, origin.y as f32 + metrics.cap_height * scale);

        for character in string.chars() {
            let glyph_id = font
                .glyph_for_char(character)
                .unwrap_or(replacement_glyph_id);

            trace!(?character, ?glyph_id, ?translation, "Drawing character");

            font.rasterize_glyph(
                &mut self.font_buffer,
                glyph_id,
                font_size,
                Transform2F::from_translation(translation),
                HintingOptions::None,
                RasterizationOptions::GrayscaleAa,
            )
            .expect("glyph exists, platform succeeds");

            translation += font.advance(glyph_id).unwrap() * scale;
        }

        let [_, cr, cg, cb] = self.state.fg_color.to_be_bytes();

        // Copy rasterized pixels onto canvas
        for (i, &opacity) in self.font_buffer.pixels.iter().enumerate() {
            let destination = &mut self.buffer[i];

            let [_, r, g, b] = destination.to_be_bytes();
            let transparency = 255 - opacity as u32;

            // Alpha is 0..=255 instead of 0..=1 so we need to divide by 255 to keep the same scale.
            // This is done at the end to make the integer multiplication more accurate.
            let r = ((r as u32 * transparency) + (cr as u32 * opacity as u32)) / 255;
            let g = ((g as u32 * transparency) + (cg as u32 * opacity as u32)) / 255;
            let b = ((b as u32 * transparency) + (cb as u32 * opacity as u32)) / 255;

            *destination = u32::from_be_bytes([0, r as u8, g as u8, b as u8]);
        }
    }

    pub fn draw_header(&mut self) {
        self.state.fg_color = HEADER_COLOR;
        self.fill_rect(Rect::HEADER_CLIP);
    }

    pub fn buffer(&self) -> &[u32; BUFSZ] {
        &self.buffer
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    // These are signed so you can do things like drawing circles and lines with parts off the
    // left side of the screen (obviously they will be clipped, but the part that's on the screen
    // should work properly).
    pub x: i32,
    pub y: i32,
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
    pub const FULL_CLIP: Self = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
    pub const USER_CLIP: Self = Rect::new(0, HEADER_HEIGHT, WIDTH as i32, HEIGHT as i32);
    pub const HEADER_CLIP: Self = Rect::new(0, 0, WIDTH as i32, HEADER_HEIGHT);

    pub const fn new(mut x0: i32, mut y0: i32, mut x1: i32, mut y1: i32) -> Self {
        if x0 > x1 {
            mem::swap(&mut x0, &mut x1);
        }
        if y0 > y1 {
            mem::swap(&mut y0, &mut y1);
        }

        Self(Point { x: x0, y: y0 }, Point { x: x1, y: y1 })
    }

    pub fn from_sdk(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let mut rect = Self::new(x0, y0 + HEADER_HEIGHT, x1, y1 + HEADER_HEIGHT);
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
fn clamp_range<T: PartialOrd + Copy>(
    source: RangeInclusive<T>,
    region: RangeInclusive<T>,
) -> Option<RangeInclusive<T>> {
    assert!(source.start() <= source.end());
    assert!(region.start() <= region.end());

    let mut begin = *source.start();
    let mut end = *source.end();

    let region_begin = *region.start();
    let region_end = *region.end();

    if begin > region_end || end < region_begin {
        return None;
    }

    if end > region_end {
        end = region_end;
    }
    if begin < region_begin {
        begin = region_begin;
    }

    Some(begin..=end)
}
