mod font;
mod paint;
mod shape;

#[cfg(test)]
mod tests;

pub use font::FontContext;
pub use paint::{Frame, PaintOutput, paint_bar, paint_computed};
pub use shape::rounded_rect;

// IconCache is re-exported at the crate root via libabar::icon; callers pass it into paint_bar.
