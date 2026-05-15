use libabar::{BarLayout, DisplayMode, Island, Segment};
use serde::Deserialize;

use super::modules::Modules;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum LayoutEntry {
    Module(String),
    Group(Vec<String>),
}

impl LayoutEntry {
    pub fn module_names(&self) -> &[String] {
        match self {
            Self::Module(name) => std::slice::from_ref(name),
            Self::Group(names) => names.as_slice(),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Layout {
    pub left: Option<Vec<LayoutEntry>>,
    pub center: Option<Vec<LayoutEntry>>,
    pub right: Option<Vec<LayoutEntry>>,
}

impl Layout {
    pub fn module_names_in_order(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for entries in [
            self.left.as_deref(),
            self.center.as_deref(),
            self.right.as_deref(),
        ] {
            let Some(entries) = entries else { continue };
            for entry in entries {
                for name in entry.module_names() {
                    names.push(name.as_str());
                }
            }
        }
        names
    }

    pub fn to_bar_layout(&self, modules: Option<&Modules>) -> BarLayout {
        BarLayout {
            left: entries_to_islands(self.left.as_deref(), modules),
            center: entries_to_islands(self.center.as_deref(), modules),
            right: entries_to_islands(self.right.as_deref(), modules),
        }
    }
}

fn entries_to_islands(entries: Option<&[LayoutEntry]>, modules: Option<&Modules>) -> Vec<Island> {
    let Some(entries) = entries else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|e| entry_to_island(e, modules))
        .collect()
}

fn entry_to_island(entry: &LayoutEntry, modules: Option<&Modules>) -> Island {
    Island {
        segments: entry
            .module_names()
            .iter()
            .map(|name| make_segment(name, modules))
            .collect(),
    }
}

fn make_segment(name: &str, modules: Option<&Modules>) -> Segment {
    // Custom modules: icon-only display; events are wired by apply_module_events.
    if let Some(custom) = modules.and_then(|m| m.custom_by_name(name)) {
        return Segment::icon_only(name, &custom.icon);
    }
    // Built-in modules: text placeholder until their own phase adds live data.
    let mut seg = Segment::new(name, builtin_label(name));
    seg.display_mode = DisplayMode::TextOnly;
    seg
}

fn builtin_label(module: &str) -> String {
    match module {
        "clock" => "clock".into(),
        "keyboard" => "kb".into(),
        "workspaces" => "ws".into(),
        "window" => "window".into(),
        "tray" => "tray".into(),
        other => other.into(),
    }
}
