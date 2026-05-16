use libabar::SegmentEvents;
use serde::Deserialize;

use super::Config;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Events {
    pub on_left_click: Option<String>,
    pub on_right_click: Option<String>,
    pub on_middle_click: Option<String>,
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
}

pub fn events_for_module(config: &Config, module_id: &str) -> SegmentEvents {
    if let Some(custom) = config
        .modules
        .as_ref()
        .and_then(|m| m.custom_by_name(module_id))
    {
        return events_from_config(custom.events.as_ref());
    }

    match module_id {
        "clock" => events_from_config(config.clock.as_ref().and_then(|c| c.events.as_ref())),
        "keyboard" => events_from_config(config.keyboard.as_ref().and_then(|k| k.events.as_ref())),
        "workspaces" => {
            events_from_config(config.workspaces.as_ref().and_then(|w| w.events.as_ref()))
        }
        _ => SegmentEvents::default(),
    }
}

fn events_from_config(events: Option<&Events>) -> SegmentEvents {
    let Some(events) = events else {
        return SegmentEvents::default();
    };
    SegmentEvents {
        on_left_click: events.on_left_click.clone(),
        on_right_click: events.on_right_click.clone(),
        on_middle_click: events.on_middle_click.clone(),
        on_scroll_up: events.on_scroll_up.clone(),
        on_scroll_down: events.on_scroll_down.clone(),
    }
}

pub fn apply_module_events(layout: &mut libabar::BarLayout, config: &Config) {
    for island in layout
        .left
        .iter_mut()
        .chain(layout.center.iter_mut())
        .chain(layout.right.iter_mut())
    {
        for segment in &mut island.segments {
            segment.events = events_for_module(config, &segment.module_id);
        }
    }
}
