use crate::layout::{ComputedBar, PlacedIsland, PlacedSegment};
use crate::model::SegmentEvents;

use super::hit_test;

fn sample_bar() -> ComputedBar {
    ComputedBar {
        width: 400,
        height: 32,
        islands: vec![
            PlacedIsland {
                x: 8.0,
                y: 4.0,
                width: 80.0,
                height: 24.0,
                segments: vec![PlacedSegment {
                    module_id: "clock".into(),
                    label: "clock".into(),
                    events: SegmentEvents::default(),
                    x: 20.0,
                    y: 8.0,
                    width: 56.0,
                    height: 16.0,
                }],
            },
            PlacedIsland {
                x: 300.0,
                y: 4.0,
                width: 90.0,
                height: 24.0,
                segments: vec![
                    PlacedSegment {
                        module_id: "kb".into(),
                        label: "kb".into(),
                        events: SegmentEvents::default(),
                        x: 312.0,
                        y: 8.0,
                        width: 30.0,
                        height: 16.0,
                    },
                    PlacedSegment {
                        module_id: "tray".into(),
                        label: "tray".into(),
                        events: SegmentEvents::default(),
                        x: 350.0,
                        y: 8.0,
                        width: 32.0,
                        height: 16.0,
                    },
                ],
            },
        ],
    }
}

#[test]
fn hit_test_finds_segment_center() {
    let bar = sample_bar();
    let hit = hit_test(&bar, 48.0, 16.0).expect("clock segment");
    assert_eq!(hit.module_id, "clock");
}

#[test]
fn hit_test_picks_segment_by_x_inside_grouped_island() {
    let bar = sample_bar();
    let hit = hit_test(&bar, 360.0, 16.0).expect("tray segment");
    assert_eq!(hit.module_id, "tray");
}

#[test]
fn hit_test_misses_outside_bar() {
    let bar = sample_bar();
    assert!(hit_test(&bar, 200.0, 200.0).is_none());
}
