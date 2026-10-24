use vello_cpu::kurbo::{BezPath, Rect, RoundedRect, Shape};

/// Build a rounded rectangle path; the radius is clamped to half the shorter side.
pub fn rounded_rect(x: f64, y: f64, w: f64, h: f64, radius: f64) -> BezPath {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let rect = Rect::new(x, y, x + w, y + h);
    if r <= 0.0 {
        return rect.to_path(0.1);
    }
    RoundedRect::from_rect(rect, r).to_path(0.1)
}
