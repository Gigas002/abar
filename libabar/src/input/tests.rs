use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::layout::{ComputedBar, PlacedIsland, PlacedSegment};
use crate::model::SegmentEvents;

use crate::spawn::ensure_runtime;

use super::{PointerAction, dispatch_pointer_action};

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
                events: SegmentEvents {
                    on_left_click: Some(command),
                    ..SegmentEvents::default()
                },
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
                events: SegmentEvents::default(),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
        }],
    };
    dispatch_pointer_action(&bar, 1.0, 1.0, PointerAction::LeftClick);
}
