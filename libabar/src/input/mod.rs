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

/// Scroll distance of one wheel notch in `wl_pointer.axis` units (the
/// libinput/wlroots convention for continuous sources such as touchpads).
pub const SCROLL_STEP: f64 = 15.0;

/// Never run more than this many scroll actions for one pointer frame.
const MAX_STEPS_PER_FRAME: i32 = 10;

/// Turns `wl_pointer` vertical axis events into whole scroll steps, so a
/// high-resolution wheel or a touchpad runs one action per notch-sized
/// distance instead of one per event.
///
/// Feed every event of a pointer frame, then call [`frame`](Self::frame).
#[derive(Debug, Default)]
pub struct ScrollAccumulator {
    /// Carried-over `axis_value120` units (120 = one notch).
    v120: i32,
    /// Carried-over continuous distance.
    continuous: f64,
    frame_v120: i32,
    frame_discrete: i32,
    frame_value: f64,
    frame_stop: bool,
}

impl ScrollAccumulator {
    /// `wl_pointer.axis_value120` (seat v8+): 120 per notch, less for
    /// high-resolution wheels.
    pub fn value120(&mut self, value: i32) {
        self.frame_v120 += value;
    }

    /// `wl_pointer.axis_discrete` (seat v5–v7): whole notches.
    pub fn discrete(&mut self, steps: i32) {
        self.frame_discrete += steps;
    }

    /// `wl_pointer.axis`: distance in surface units.
    pub fn axis(&mut self, value: f64) {
        self.frame_value += value;
    }

    /// `wl_pointer.axis_stop`: the finger left the touchpad.
    pub fn stop(&mut self) {
        self.frame_stop = true;
    }

    /// End of a pointer frame: the number of steps to run, negative for up.
    pub fn frame(&mut self) -> i32 {
        let steps = if self.frame_v120 != 0 {
            if self.v120.signum() == -self.frame_v120.signum() {
                self.v120 = 0;
            }
            self.v120 += self.frame_v120;
            let steps = self.v120 / 120;
            self.v120 -= steps * 120;
            steps
        } else if self.frame_discrete != 0 {
            self.frame_discrete
        } else if self.frame_value != 0.0 {
            if self.continuous.signum() == -self.frame_value.signum() {
                self.continuous = 0.0;
            }
            self.continuous += self.frame_value;
            let steps = (self.continuous / SCROLL_STEP).trunc();
            self.continuous -= steps * SCROLL_STEP;
            steps as i32
        } else {
            0
        };
        if self.frame_stop {
            self.v120 = 0;
            self.continuous = 0.0;
        }
        self.frame_v120 = 0;
        self.frame_discrete = 0;
        self.frame_value = 0.0;
        self.frame_stop = false;
        steps.clamp(-MAX_STEPS_PER_FRAME, MAX_STEPS_PER_FRAME)
    }
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
