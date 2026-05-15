use crate::model::{BarSpec, BarStyle, Island, Segment};

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
    pub module_id: String,
    pub label: String,
    pub events: crate::model::SegmentEvents,
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

/// One bar region's islands and their measured sizes.
pub struct RegionLayout<'a> {
    pub islands: &'a [Island],
    pub metrics: &'a [IslandMetrics],
}

/// Lay out islands for a bar of `bar_width` pixels using pre-measured sizes.
pub fn layout_regions(
    bar_width: u32,
    style: &BarStyle,
    left: RegionLayout<'_>,
    center: RegionLayout<'_>,
    right: RegionLayout<'_>,
) -> ComputedBar {
    let bar_width = f64::from(bar_width);
    let inner_height = region_inner_height(left.metrics, center.metrics, right.metrics);
    let bar_height = inner_height + 2.0 * style.bar_padding_y;

    let mut islands = Vec::new();

    let center_total = row_width(center.metrics, style.island_gap);
    let right_total = row_width(right.metrics, style.island_gap);

    let x = style.bar_padding_x;
    place_row(
        &mut islands,
        x,
        style,
        bar_height,
        inner_height,
        left.islands,
        left.metrics,
    );

    let center_x = ((bar_width - center_total) / 2.0).max(style.bar_padding_x);
    place_row(
        &mut islands,
        center_x,
        style,
        bar_height,
        inner_height,
        center.islands,
        center.metrics,
    );

    let right_x = (bar_width - style.bar_padding_x - right_total).max(style.bar_padding_x);
    place_row(
        &mut islands,
        right_x,
        style,
        bar_height,
        inner_height,
        right.islands,
        right.metrics,
    );

    ComputedBar {
        width: bar_width.round() as u32,
        height: bar_height.round() as u32,
        islands,
    }
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
    islands: &[Island],
    metrics: &[IslandMetrics],
) {
    for (island, metric) in islands.iter().zip(metrics) {
        let y = style.bar_padding_y + (inner_height - metric.height) / 2.0;
        let placed = place_island(x, y, metric, style, &island.segments);
        x += metric.width + style.island_gap;
        out.push(placed);
    }
}

fn place_island(
    x: f64,
    y: f64,
    metric: &IslandMetrics,
    style: &BarStyle,
    segments: &[Segment],
) -> PlacedIsland {
    let mut seg_x = x + style.island_padding_x;
    let inner_h = metric.height - 2.0 * style.island_padding_y;
    let seg_y = y + style.island_padding_y;

    let widths = if metric.segment_widths.is_empty() {
        vec![0.0]
    } else {
        metric.segment_widths.clone()
    };

    let placed_segments = segments
        .iter()
        .zip(widths.iter())
        .map(|(segment, &seg_w)| {
            let placed = PlacedSegment {
                module_id: segment.module_id.clone(),
                label: segment.label.clone(),
                events: segment.events.clone(),
                x: seg_x,
                y: seg_y,
                width: seg_w,
                height: inner_h,
            };
            seg_x += seg_w + style.segment_gap;
            placed
        })
        .collect();

    PlacedIsland {
        x,
        y,
        width: metric.width,
        height: metric.height,
        segments: placed_segments,
    }
}

/// Measure all islands and run the layout pass for `spec` at `bar_width`.
pub fn compute_bar(
    spec: &BarSpec,
    bar_width: u32,
    measure: &impl Fn(&str) -> (f64, f64),
) -> ComputedBar {
    let left_metrics = spec
        .layout
        .left
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();
    let center_metrics = spec
        .layout
        .center
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();
    let right_metrics = spec
        .layout
        .right
        .iter()
        .map(|i| measure_island(i, &spec.style, measure))
        .collect::<Vec<_>>();

    layout_regions(
        bar_width,
        &spec.style,
        RegionLayout {
            islands: &spec.layout.left,
            metrics: &left_metrics,
        },
        RegionLayout {
            islands: &spec.layout.center,
            metrics: &center_metrics,
        },
        RegionLayout {
            islands: &spec.layout.right,
            metrics: &right_metrics,
        },
    )
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
