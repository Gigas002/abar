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
        // set_text does not reset attributes left by a previous set_markup call; clear them
        // explicitly so color spans from markup segments don't bleed into plain-text segments.
        self.layout.set_attributes(None);
        let (w, h) = self.layout.size();
        (
            f64::from(w) / f64::from(pango::SCALE),
            f64::from(h) / f64::from(pango::SCALE),
        )
    }

    /// Measure text that contains Pango markup.  Sets the layout to markup mode so that a
    /// subsequent `show_layout` call renders with the embedded color/style attributes.
    pub fn measure_markup(&self, markup: &str) -> (f64, f64) {
        self.layout.set_markup(markup);
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
