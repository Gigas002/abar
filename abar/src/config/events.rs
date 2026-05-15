#![allow(clippy::derivable_impls)]

use serde::Deserialize;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Events {
    pub on_left_click: Option<String>,
    pub on_right_click: Option<String>,
    pub on_middle_click: Option<String>,
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
}
