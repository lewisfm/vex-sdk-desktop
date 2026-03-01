use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    ops::RangeInclusive,
    sync::{Arc, LazyLock},
};

use fast_image_resize::{
    FilterType, ImageViewMut, ResizeAlg, ResizeOptions, Resizer, images::TypedImageRef, pixels::U8,
};
use font_kit::{
    canvas::{Canvas as FontCanvas, Format, RasterizationOptions},
    hinting::HintingOptions,
    loaders::freetype::Font,
    metrics::Metrics,
};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    transform2d::Transform2F,
    vector::{Vector2F, Vector2I},
};

static MONOSPACE_FONT: &[u8] = include_bytes!("../../assets/font/NotoMono-Regular.ttf");
static PROPORTIONAL_FONT: &[u8] = include_bytes!("../../assets/font/NotoSans-Regular.ttf");

static FONT_MAP: &[(&str, f32, &[u8])] = &[
    ("NotoSansMono", 49.0, MONOSPACE_FONT),
    ("NotoSansLatin", 54.0, PROPORTIONAL_FONT),
];

const PRE_RENDERED_CHARS: RangeInclusive<char> = (32u8 as char)..=(126u8 as char);
const NUM_CHARS: usize = *PRE_RENDERED_CHARS.end() as usize - *PRE_RENDERED_CHARS.start() as usize;

pub static FONTS: LazyLock<FontLoader> = LazyLock::new(FontLoader::new);

pub struct FontLoader {
    fonts: HashMap<&'static str, Arc<PreRenderedFont>>,
}

impl FontLoader {
    pub fn new() -> Self {
        let mut fonts = HashMap::new();

        for &(font_name, point_size, data) in FONT_MAP {
            let data = Arc::new(Vec::from(data));
            let font = Font::from_bytes(data.clone(), 0).expect("bundled fonts are valid");

            fonts.insert(font_name, Arc::new(PreRenderedFont::new(&font, point_size)));
        }

        // Font name aliases for public API
        fonts.insert("monospace", fonts["NotoSansMono"].clone());
        fonts.insert("proportional", fonts["NotoSansLatin"].clone());

        Self { fonts }
    }

    pub fn get(&self, name: &str) -> Option<Arc<PreRenderedFont>> {
        self.fonts.get(name).cloned()
    }
}

pub struct PreRenderedFont {
    name: String,
    point_size: f32,
    metrics: Metrics,
    characters: Vec<RasterizedGlyph>,
}

impl PreRenderedFont {
    pub fn new(font: &Font, point_size: f32) -> Self {
        let metrics = font.metrics();

        Self {
            name: font.full_name(),
            point_size,
            characters: PRE_RENDERED_CHARS
                .map(|character| RasterizedGlyph::new(font, &metrics, point_size, character))
                .collect(),
            metrics,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cap_height(&self, numerator: u32, denominator: u32) -> u32 {
        let scale = self.point_size / self.metrics.units_per_em as f32;
        (self.metrics.ascent * scale * numerator as f32 / denominator as f32) as u32
    }

    pub fn glyph_for_char(&self, character: char) -> &RasterizedGlyph {
        let replacement_char = b'.' - *PRE_RENDERED_CHARS.start() as u8;

        let idx = character as u8 - *PRE_RENDERED_CHARS.start() as u8;
        self.characters
            .get(idx as usize)
            .unwrap_or(&self.characters[replacement_char as usize])
    }
}

impl Debug for PreRenderedFont {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        struct RenderChars(usize);

        impl Debug for RenderChars {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                let chars =
                    (0..self.0).map(|i| (i + *PRE_RENDERED_CHARS.start() as usize) as u8 as char);
                f.debug_set().entries(chars).finish()
            }
        }

        f.debug_struct("PreRenderedFont")
            .field("point_size", &self.point_size)
            .field("characters", &RenderChars(self.characters.len()))
            .finish()
    }
}

pub struct RasterizedGlyph {
    size: Vector2I,
    offset: Vector2I,
    advance: f32,
    bitmap: Vec<u8>,
}

impl RasterizedGlyph {
    /// Rasterize the given character without any antialiasing.
    pub fn new(font: &Font, metrics: &Metrics, point_size: f32, character: char) -> Self {
        let scale = point_size / metrics.units_per_em as f32;

        let transform = Transform2F::default();
        let hinting = HintingOptions::Full(point_size);
        let rasterization_options = RasterizationOptions::SubpixelAa;

        let glyph_id = font.glyph_for_char(character).expect("missing character");

        let dims = font
            .raster_bounds(
                glyph_id,
                point_size,
                transform,
                hinting,
                rasterization_options,
            )
            .expect("glyph should render");
        let mut canvas = FontCanvas::new(dims.size(), Format::A8);

        // Move the character from its default offset to the upper left of the canvas so that none
        // of it is cut off.
        let render_transform = Transform2F::from_translation(-dims.origin().to_f32());
        font.rasterize_glyph(
            &mut canvas,
            glyph_id,
            point_size,
            render_transform,
            hinting,
            rasterization_options,
        )
        .expect("glyph should render");

        for pixel in &mut canvas.pixels {
            // Make pixels brighter.
            *pixel = ((*pixel as f32 / 255.0).sqrt() * 255.0) as u8;
            // Reduce number of possible opacity values by quantizing to a u2 and scaling back up.
            *pixel = *pixel / (255 / 3) * (255 / 3);
        }

        Self {
            size: dims.size(),
            offset: dims.origin(),
            advance: font.advance(glyph_id).unwrap().x() * scale,
            bitmap: canvas.pixels,
        }
    }

    /// Returns the dimensions of the glyph, scaled by the given fraction.
    pub fn scaled_bounds(&self, numerator: u32, denominator: u32) -> RectI {
        let scaled_size = self.size.to_f32() * numerator as f32 / denominator as f32;
        let scaled_offset = self.offset.to_f32() * numerator as f32 / denominator as f32;
        RectF::new(scaled_offset, scaled_size).to_i32()
    }

    pub fn advance(&self, numerator: u32, denominator: u32) -> i32 {
        (self.advance * numerator as f32 / denominator as f32) as i32
    }

    /// Scales the glyph and writes it to the given destination.
    pub fn render(&self, destination: &mut impl ImageViewMut<Pixel = U8>) {
        let source: TypedImageRef<'_, U8> =
            TypedImageRef::from_buffer(self.size.x() as u32, self.size.y() as u32, &self.bitmap)
                .expect("buffer aligned and big enough");

        let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));

        Resizer::new()
            .resize_typed(&source, destination, Some(&opts))
            .expect("resizing succeeds");
    }
}

impl Debug for RasterizedGlyph {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RasterizedGlyph")
            .field("size", &self.size)
            .field("offset", &self.offset)
            .field("advance", &self.advance)
            .finish()
    }
}
