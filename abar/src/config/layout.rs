use libabar::{BarLayout, Island, Segment};
use serde::Deserialize;

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

    pub fn to_bar_layout(&self) -> BarLayout {
        BarLayout {
            left: entries_to_islands(self.left.as_deref()),
            center: entries_to_islands(self.center.as_deref()),
            right: entries_to_islands(self.right.as_deref()),
        }
    }
}

fn entries_to_islands(entries: Option<&[LayoutEntry]>) -> Vec<Island> {
    let Some(entries) = entries else {
        return Vec::new();
    };
    entries.iter().map(entry_to_island).collect()
}

fn entry_to_island(entry: &LayoutEntry) -> Island {
    Island {
        segments: entry
            .module_names()
            .iter()
            .map(|name| Segment {
                label: segment_label(name),
            })
            .collect(),
    }
}

fn segment_label(module: &str) -> String {
    match module {
        "clock" => "clock".into(),
        "keyboard" => "kb".into(),
        "workspaces" => "ws".into(),
        "window" => "window".into(),
        "tray" => "tray".into(),
        other => other.into(),
    }
}
