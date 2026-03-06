use std::io::Cursor;

use image::{ImageFormat, ImageReader};

use crate::canvas::{Canvas, Point, Rect};

pub struct SimImage {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
}

impl SimImage {
    pub fn from_png(png_bytes: &[u8]) -> Self {
        let img = ImageReader::with_format(Cursor::new(png_bytes), ImageFormat::Png)
            .decode()
            .unwrap()
            .into_rgb8();

        let width = img.width();
        let height = img.height();

        // We use u32 colors, not u8x4/u8x3, so we need to align the pixels to 4 bytes.
        let pixels: Vec<u32> = img
            .pixels()
            .map(|p| u32::from_be_bytes([0, p[0], p[1], p[2]]))
            .collect();

        Self {
            pixels,
            width,
            height,
        }
    }

    pub fn height(&self) -> i32 {
        self.height as i32
    }

    pub fn width(&self) -> i32 {
        self.width as i32
    }

    pub fn buffer(&self) -> &[u32] {
        &self.pixels
    }

    pub fn draw(&self, canvas: &mut Canvas, origin: Point) {
        let bounds = Rect::sized(origin.x, origin.y, self.width as i32, self.height as i32);
        unsafe {
            canvas.copy_rect(bounds, self.pixels.as_ptr(), self.width as usize);
        };
    }
}
