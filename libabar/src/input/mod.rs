use crate::hit_test;
use crate::layout::ComputedBar;
use crate::model::SegmentEvents;
use crate::spawn;

/// Pointer interaction mapped from Wayland button / scroll events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerAction {
    LeftClick,
    RightClick,
    MiddleClick,
    ScrollUp,
    ScrollDown,
}

impl SegmentEvents {
    pub fn command_for(&self, action: PointerAction) -> Option<&str> {
        match action {
            PointerAction::LeftClick => self.on_left_click(),
            PointerAction::RightClick => self.on_right_click(),
            PointerAction::MiddleClick => self.on_middle_click(),
            PointerAction::ScrollUp => self.on_scroll_up(),
            PointerAction::ScrollDown => self.on_scroll_down(),
        }
    }
}

/// Hit-test `(x, y)` and spawn the configured shell command without blocking the caller.
pub fn dispatch_pointer_action(computed: &ComputedBar, x: f64, y: f64, action: PointerAction) {
    let Some(segment) = hit_test::hit_test(computed, x, y) else {
        return;
    };
    let Some(command) = segment.events.command_for(action) else {
        return;
    };
    spawn::spawn_shell_command(command);
}

#[cfg(test)]
mod tests;
