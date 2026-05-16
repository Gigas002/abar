use crate::layout::{ComputedBar, PlacedSegment};

fn point_in_rect(x: f64, y: f64, rx: f64, ry: f64, rw: f64, rh: f64) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Return the index of the island whose bounding rect contains `(x, y)`, if any.
pub fn island_index_at(computed: &ComputedBar, x: f64, y: f64) -> Option<usize> {
    computed.islands.iter().position(|island| {
        point_in_rect(x, y, island.x, island.y, island.width, island.height)
    })
}

/// Return the segment under `(x, y)` in bar surface coordinates, if any.
pub fn hit_test(computed: &ComputedBar, x: f64, y: f64) -> Option<&PlacedSegment> {
    for island in &computed.islands {
        if !point_in_rect(x, y, island.x, island.y, island.width, island.height) {
            continue;
        }

        for segment in &island.segments {
            if point_in_rect(x, y, segment.x, segment.y, segment.width, segment.height) {
                return Some(segment);
            }
        }

        if let Some(segment) = island
            .segments
            .iter()
            .find(|s| x >= s.x && x < s.x + s.width)
        {
            return Some(segment);
        }

        return island.segments.first();
    }
    None
}

#[cfg(test)]
mod tests;
