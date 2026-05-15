use pango::prelude::*;
use pango::{FontDescription, Layout};
use pangocairo::FontMap as CairoFontMap;

use crate::error::AbarError;

pub struct FontContext {
    layout: Layout,
}

impl FontContext {
    pub fn new(font_name: &str, font_size: f64) -> Result<Self, AbarError> {
        let mut desc = FontDescription::from_string(font_name);
        let size = (font_size * f64::from(pango::SCALE)).round() as i32;
        desc.set_size(size);
        let map = CairoFontMap::new();
        let context = map.create_context();
        context.set_font_description(&desc);
        let layout = Layout::new(&context);
        Ok(Self { layout })
    }

    pub fn measure(&self, text: &str) -> (f64, f64) {
        self.layout.set_text(text);
        let (w, h) = self.layout.size();
        (
            f64::from(w) / f64::from(pango::SCALE),
            f64::from(h) / f64::from(pango::SCALE),
        )
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }
}
