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
}
