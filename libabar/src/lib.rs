pub mod color;
pub mod error;
pub mod layout;
pub mod model;
pub mod render;
pub mod wayland;

pub use color::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};
pub use error::AbarError;
pub use model::{BarColors, BarLayout, BarSpec, BarStyle, Island, Segment};
pub use render::{FontContext, Frame, paint_bar};
