/// Optional shell commands for pointer actions on a module segment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentEvents {
    pub on_left_click: Option<String>,
    pub on_right_click: Option<String>,
    pub on_middle_click: Option<String>,
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
}

impl SegmentEvents {
    pub fn on_left_click(&self) -> Option<&str> {
        self.on_left_click.as_deref()
    }

    pub fn on_right_click(&self) -> Option<&str> {
        self.on_right_click.as_deref()
    }

    pub fn on_middle_click(&self) -> Option<&str> {
        self.on_middle_click.as_deref()
    }

    pub fn on_scroll_up(&self) -> Option<&str> {
        self.on_scroll_up.as_deref()
    }

    pub fn on_scroll_down(&self) -> Option<&str> {
        self.on_scroll_down.as_deref()
    }
}

/// Whether a segment paints its icon, its text label, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    TextOnly,
    IconOnly,
}

/// One module segment inside a grouped island.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub module_id: String,
    pub label: String,
    /// FreeDesktop icon name; required when `display_mode` is `IconOnly`.
    pub icon_name: Option<String>,
    pub display_mode: DisplayMode,
    pub events: SegmentEvents,
}

impl Segment {
    /// Construct a text-only segment with no icon.
    pub fn new(module_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            label: label.into(),
            icon_name: None,
            display_mode: DisplayMode::TextOnly,
            events: SegmentEvents::default(),
        }
    }

    /// Construct an icon-only segment for a custom module.
    pub fn icon_only(module_id: impl Into<String>, icon_name: impl Into<String>) -> Self {
        let id = module_id.into();
        Self {
            label: id.clone(),
            module_id: id,
            icon_name: Some(icon_name.into()),
            display_mode: DisplayMode::IconOnly,
            events: SegmentEvents::default(),
        }
    }
}

/// Rounded background region with one or more segments (left to right).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Island {
    pub segments: Vec<Segment>,
}

/// Three logical bar regions; each entry is one island.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BarLayout {
    pub left: Vec<Island>,
    pub center: Vec<Island>,
    pub right: Vec<Island>,
}

/// Colors in **BGRA** byte order for `WL_SHM_FORMAT_ARGB8888` buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarColors {
    pub background: [u8; 4],
    pub foreground: [u8; 4],
}

/// Spacing and typography for layout and paint.
#[derive(Debug, Clone, PartialEq)]
pub struct BarStyle {
    pub font_name: String,
    pub font_size: f64,
    pub bar_padding_x: f64,
    pub bar_padding_y: f64,
    pub island_padding_x: f64,
    pub island_padding_y: f64,
    pub island_radius: f64,
    pub island_gap: f64,
    pub segment_gap: f64,
}

impl Default for BarStyle {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".to_string(),
            font_size: 14.0,
            bar_padding_x: 8.0,
            bar_padding_y: 4.0,
            island_padding_x: 12.0,
            island_padding_y: 4.0,
            island_radius: 12.0,
            island_gap: 8.0,
            segment_gap: 8.0,
        }
    }
}

/// Fully resolved bar description for one paint pass.
#[derive(Debug, Clone, PartialEq)]
pub struct BarSpec {
    pub colors: BarColors,
    pub style: BarStyle,
    pub layout: BarLayout,
}

impl BarSpec {
    pub fn new(colors: BarColors, style: BarStyle, layout: BarLayout) -> Self {
        Self {
            colors,
            style,
            layout,
        }
    }
}
