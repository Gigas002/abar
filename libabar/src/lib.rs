pub mod color;
pub mod error;
pub mod hit_test;
pub mod icon;
pub mod input;
pub mod layout;
pub mod model;
pub mod modules;
pub mod render;
pub mod spawn;
pub mod wayland;

pub use color::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};
pub use error::AbarError;
pub use icon::{IconCache, default_search_dirs, resolve_icon};
pub use input::PointerAction;
pub use model::{
    BarColors, BarLayout, BarSpec, BarStyle, DisplayMode, Island, Segment, SegmentEvents,
    SubmenuItemConfig,
};
pub use modules::ModuleConfigs;
#[cfg(feature = "workspaces")]
pub use modules::workspaces::{
    VisibilityMode as WorkspaceVisibilityMode, WorkspacesConfig, trim_alpha,
};
pub use render::{FontContext, Frame, PaintOutput, paint_bar};
pub use spawn::{ensure_runtime, spawn_shell_command};
pub use wayland::run_bar;
