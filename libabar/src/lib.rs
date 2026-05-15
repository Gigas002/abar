pub mod color;
pub mod error;
pub mod hit_test;
pub mod input;
pub mod layout;
pub mod model;
pub mod render;
pub mod spawn;
pub mod wayland;

pub use color::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};
pub use error::AbarError;
pub use input::PointerAction;
pub use model::{BarColors, BarLayout, BarSpec, BarStyle, Island, Segment, SegmentEvents};
pub use render::{FontContext, Frame, PaintOutput, paint_bar};
pub use spawn::{ensure_runtime, spawn_shell_command};
