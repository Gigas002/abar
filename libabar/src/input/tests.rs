use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::layout::{ComputedBar, PlacedIsland, PlacedSegment};
use crate::model::{DisplayMode, SegmentEvents};

use crate::spawn::ensure_runtime;

use super::{PointerAction, SCROLL_STEP, ScrollAccumulator, dispatch_pointer_action};

#[test]
fn dispatch_spawns_configured_command() {
    ensure_runtime().expect("tokio runtime");
    let dir = tempfile::tempdir().expect("tempdir");
    let marker: PathBuf = dir.path().join("clicked");
    let path = marker.display().to_string();
    let command = format!("touch '{path}'");

    let bar = ComputedBar {
        width: 100,
        height: 20,
        islands: vec![PlacedIsland {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
            segments: vec![PlacedSegment {
                module_id: "test".into(),
                label: "t".into(),
                icon_name: None,
                display_mode: DisplayMode::TextOnly,
                events: SegmentEvents {
                    on_left_click: Some(command),
                    ..SegmentEvents::default()
                },
                use_markup: false,
                submenu: Vec::new(),
                tooltip: None,
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            }],
        }],
    };

    dispatch_pointer_action(&bar, 10.0, 10.0, PointerAction::LeftClick);

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if marker.is_file() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("expected marker file at {}", marker.display());
}

#[test]
fn dispatch_without_configured_command_is_noop() {
    let bar = ComputedBar {
        width: 10,
        height: 10,
        islands: vec![PlacedIsland {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            segments: vec![PlacedSegment {
                module_id: "x".into(),
                label: "x".into(),
                icon_name: None,
                display_mode: DisplayMode::TextOnly,
                events: SegmentEvents::default(),
                use_markup: false,
                submenu: Vec::new(),
                tooltip: None,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
        }],
    };
    dispatch_pointer_action(&bar, 1.0, 1.0, PointerAction::LeftClick);
}

fn frame_steps(acc: &mut ScrollAccumulator, events: impl FnOnce(&mut ScrollAccumulator)) -> i32 {
    events(acc);
    acc.frame()
}

#[test]
fn a_classic_wheel_notch_is_one_step() {
    let mut acc = ScrollAccumulator::default();
    // seat v8: axis + value120 for one notch; the axis value must not count twice.
    assert_eq!(
        frame_steps(&mut acc, |a| {
            a.axis(15.0);
            a.value120(120)
        }),
        1
    );
    // seat v5-7: axis + discrete.
    assert_eq!(
        frame_steps(&mut acc, |a| {
            a.axis(-15.0);
            a.discrete(-1)
        }),
        -1
    );
}

#[test]
fn a_high_resolution_notch_is_one_step_not_four() {
    let mut acc = ScrollAccumulator::default();
    let steps: Vec<i32> = (0..4)
        .map(|_| {
            frame_steps(&mut acc, |a| {
                a.axis(3.75);
                a.value120(30)
            })
        })
        .collect();
    assert_eq!(steps, [0, 0, 0, 1]);
    // Without value120 (axis only), the distance still adds up to one notch.
    let mut acc = ScrollAccumulator::default();
    let total: i32 = (0..4)
        .map(|_| frame_steps(&mut acc, |a| a.axis(3.75)))
        .sum();
    assert_eq!(total, 1);
}

#[test]
fn touchpad_distance_maps_to_notches() {
    let mut acc = ScrollAccumulator::default();
    // 30 events of 3 units = 90 units = 6 notches.
    let total: i32 = (0..30)
        .map(|_| frame_steps(&mut acc, |a| a.axis(3.0)))
        .sum();
    assert_eq!(total, 6);
    assert_eq!(90.0 / SCROLL_STEP, 6.0);
}

#[test]
fn reversing_or_lifting_the_finger_drops_the_remainder() {
    let mut acc = ScrollAccumulator::default();
    assert_eq!(frame_steps(&mut acc, |a| a.axis(14.0)), 0);
    // Reversal: the 14 units down don't cancel the way up.
    assert_eq!(frame_steps(&mut acc, |a| a.axis(-15.0)), -1);
    assert_eq!(
        frame_steps(&mut acc, |a| {
            a.axis(10.0);
            a.stop()
        }),
        0
    );
    // After axis_stop a new gesture starts from zero.
    assert_eq!(frame_steps(&mut acc, |a| a.axis(10.0)), 0);
}

#[test]
fn a_huge_frame_is_capped() {
    let mut acc = ScrollAccumulator::default();
    assert_eq!(frame_steps(&mut acc, |a| a.value120(120 * 1000)), 10);
}
