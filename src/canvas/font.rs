use std::{collections::HashMap, rc::Rc, sync::Arc};

use font_kit::loaders::freetype::Font;

static MONOSPACE_FONT: &[u8] = include_bytes!("../../assets/font/NotoSansMono-Regular.ttf");
static PROPORTIONAL_FONT: &[u8] = include_bytes!("../../assets/font/NotoSans-Regular.ttf");

static FONT_MAP: &[(&str, f32, &[u8])] = &[
    ("NotoSansMono", 49.0, MONOSPACE_FONT),
    ("NotoSansLatin", 54.0, PROPORTIONAL_FONT),
];

thread_local! {
    pub static FONTS: FontLoader = FontLoader::new();
}

pub struct FontLoader {
    fonts: HashMap<&'static str, (Rc<Font>, f32)>,
}

impl FontLoader {
    pub fn new() -> Self {
        Self {
            fonts: FONT_MAP
                .iter()
                .map(|&(name, size, data)| {
                    let data = Arc::new(Vec::from(data));
                    let font = Font::from_bytes(data.clone(), 0).expect("bundled fonts are valid");
                    (name, (Rc::new(font), size))
                })
                .collect(),
        }
    }

    pub fn get(&self, mut name: &str) -> Option<(&'static str, f32, Rc<Font>)> {
        if name == "proportional" {
            name = "NotoSansLatin";
        }

        if name == "monospace" {
            name = "NotoSansMono";
        }

        let (name, (font, size)) = self.fonts.get_key_value(name)?;

        Some((name, *size, font.clone()))
    }
}
