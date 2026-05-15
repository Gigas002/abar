use crate::model::{BarLayout, BarSpec, BarStyle, Island};

/// Measured island before placement on the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct IslandMetrics {
    pub width: f64,
    pub height: f64,
    pub segment_widths: Vec<f64>,
}

/// Island ready to paint with absolute coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedIsland {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub segments: Vec<PlacedSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedSegment {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedBar {
    pub width: u32,
    pub height: u32,
    pub islands: Vec<PlacedIsland>,
}

/// Lay out islands for a bar of `bar_width` pixels using pre-measured sizes.
pub fn layout_regions(
    bar_width: u32,
    style: &BarStyle,
    left: &[IslandMetrics],
    center: &[IslandMetrics],
    right: &[IslandMetrics],
    labels: &RegionLabels,
) -> ComputedBar {
    let bar_width = f64::from(bar_width);
    let inner_height = region_inner_height(left, center, right);
    let bar_height = inner_height + 2.0 * style.bar_padding_y;

    let mut islands = Vec::new();

    let center_total = row_width(center, style.island_gap);
    let right_total = row_width(right, style.island_gap);

    let x = style.bar_padding_x;
    place_row(
        &mut islands,
        x,
        style,
        bar_height,
        inner_height,
        left,
        &labels.left,
    );

    let center_x = ((bar_width - center_total) / 2.0).max(style.bar_padding_x);
    place_row(
        &mut islands,
        center_x,
        style,
        bar_height,
        inner_height,
        center,
        &labels.center,
    );

    let right_x = (bar_width - style.bar_padding_x - right_total).max(style.bar_padding_x);
    place_row(
        &mut islands,
        right_x,
        style,
        bar_height,
        inner_height,
        right,
        &labels.right,
    );

    ComputedBar {
        width: bar_width.round() as u32,
        height: bar_height.round() as u32,
        islands,
    }
}

/// Labels per region, aligned with measured island lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RegionLabels {
    pub left: Vec<Vec<String>>,
    pub center: Vec<Vec<String>>,
    pub right: Vec<Vec<String>>,
}

pub fn region_labels(layout: &BarLayout) -> RegionLabels {
    RegionLabels {
        left: layout.left.iter().map(island_labels).collect(),
        center: layout.center.iter().map(island_labels).collect(),
        right: layout.right.iter().map(island_labels).collect(),
    }
}

fn island_labels(island: &Island) -> Vec<String> {
    island.segments.iter().map(|s| s.label.clone()).collect()
}

fn region_inner_height(
    left: &[IslandMetrics],
    center: &[IslandMetrics],
    right: &[IslandMetrics],
) -> f64 {
    left.iter()
        .chain(center)
        .chain(right)
        .map(|m| m.height)
        .fold(0.0_f64, f64::max)
}

fn row_width(islands: &[IslandMetrics], gap: f64) -> f64 {
    if islands.is_empty() {
        return 0.0;
    }
    let sum: f64 = islands.iter().map(|i| i.width).sum();
    sum + gap * (islands.len().saturating_sub(1) as f64)
}

fn place_row(
    out: &mut Vec<PlacedIsland>,
    mut x: f64,
    style: &BarStyle,
    _bar_height: f64,
    inner_height: f64,
    metrics: &[IslandMetrics],
    label_groups: &[Vec<String>],
) {
    for (metric, labels) in metrics.iter().zip(label_groups) {
        let y = style.bar_padding_y + (inner_height - metric.height) / 2.0;
        let placed = place_island(x, y, metric, style, labels);
        x += metric.width + style.island_gap;
        out.push(placed);
    }
}

fn place_island(
    x: f64,
    y: f64,
    metric: &IslandMetrics,
    style: &BarStyle,
    labels: &[String],
) -> PlacedIsland {
    let mut seg_x = x + style.island_padding_x;
    let inner_h = metric.height - 2.0 * style.island_padding_y;
    let seg_y = y + style.island_padding_y;

    let widths = if metric.segment_widths.is_empty() {
        vec![0.0]
    } else {
        metric.segment_widths.clone()
    };

    let segments = labels
        .iter()
        .zip(widths.iter())
        .map(|(label, &seg_w)| {
            let seg = PlacedSegment {
                label: label.clone(),
                x: seg_x,
                y: seg_y,
                width: seg_w,
                height: inner_h,
            };
            seg_x += seg_w + style.segment_gap;
            seg
        })
        .collect();

    PlacedIsland {
        x,
        y,
        width: metric.width,
        height: metric.height,
        segments,
    }
}

/// Measure all islands and run the layout pass for `spec` at `bar_width`.
pub fn compute_bar(
    spec: &BarSpec,
    bar_width: u32,
    measure: &impl Fn(&str) -> (f64, f64),
) -> ComputedBar {
    let left = spec
        .layout
        .left
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();
    let center = spec
        .layout
        .center
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();
    let right = spec
        .layout
        .right
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();

    let labels = region_labels(&spec.layout);
    layout_regions(bar_width, &spec.style, &left, &center, &right, &labels)
}

fn measure_island(
    island: &Island,
    style: &BarStyle,
    measure: &impl Fn(&str) -> (f64, f64),
) -> IslandMetrics {
    let mut max_h = 0.0_f64;
    let mut segment_widths = Vec::with_capacity(island.segments.len());

    for seg in &island.segments {
        let (w, h) = measure(&seg.label);
        max_h = max_h.max(h);
        segment_widths.push(w);
    }

    if island.segments.is_empty() {
        let (_, h) = measure(" ");
        max_h = h;
    }

    let gaps = style.segment_gap * segment_widths.len().saturating_sub(1) as f64;
    let inner_w: f64 = segment_widths.iter().sum::<f64>() + gaps;

    IslandMetrics {
        width: inner_w + 2.0 * style.island_padding_x,
        height: max_h + 2.0 * style.island_padding_y,
        segment_widths,
    }
}

#[cfg(test)]
mod tests;
