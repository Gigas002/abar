use super::*;
use crate::model::{BarStyle, Island, Segment};

fn fixed_measure(label: &str, _is_markup: bool) -> (f64, f64) {
    let w = label.len() as f64 * 8.0;
    (w, 16.0)
}

#[test]
fn center_row_is_horizontally_centered() {
    let style = BarStyle {
        bar_padding_x: 0.0,
        island_gap: 0.0,
        island_padding_x: 0.0,
        island_padding_y: 0.0,
        ..BarStyle::default()
    };
    let left_island = Island {
        segments: vec![Segment::new("l", "L")],
    };
    let center_island = Island {
        segments: vec![Segment::new("c", "C")],
    };
    let right_island = Island {
        segments: vec![Segment::new("r", "R")],
    };
    let left = [IslandMetrics {
        width: 40.0,
        height: 20.0,
        segment_widths: vec![40.0],
    }];
    let center = [IslandMetrics {
        width: 60.0,
        height: 20.0,
        segment_widths: vec![60.0],
    }];
    let right = [IslandMetrics {
        width: 40.0,
        height: 20.0,
        segment_widths: vec![40.0],
    }];

    let bar = layout_regions(
        200,
        &style,
        RegionLayout {
            islands: &[left_island],
            metrics: &left,
        },
        RegionLayout {
            islands: &[center_island],
            metrics: &center,
        },
        RegionLayout {
            islands: &[right_island],
            metrics: &right,
        },
    );
    let center_island = bar
        .islands
        .iter()
        .find(|i| i.segments.first().is_some_and(|s| s.label == "C"))
        .unwrap();
    assert!(
        (center_island.x - 70.0).abs() < 0.01,
        "x={}",
        center_island.x
    );
}

#[test]
fn grouped_island_has_multiple_segments() {
    let style = BarStyle::default();
    let island = Island {
        segments: vec![Segment::new("a", "a"), Segment::new("b", "bb")],
    };
    let m = measure_island(&island, &style, &fixed_measure);
    assert_eq!(m.segment_widths.len(), 2);
    assert!(m.width > m.segment_widths[0] + m.segment_widths[1]);
}
