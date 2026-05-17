use cairo::{Context, Format, ImageSurface, Operator};
use pangocairo::functions::show_layout;

use crate::error::AbarError;
use crate::icon::IconCache;
use crate::layout::ComputedBar;
use crate::model::{BarColors, BarSpec, DisplayMode};

use super::font::FontContext;
use super::shape::rounded_rect;

/// ARGB8888 SHM buffer in **BGRA** byte order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub data: Vec<u8>,
}

/// Frame buffer plus layout geometry for hit-testing pointer input.
#[derive(Debug, Clone)]
pub struct PaintOutput {
    pub frame: Frame,
    pub computed: ComputedBar,
}

/// Paint the full bar into a CPU buffer.
pub fn paint_bar(
    spec: &BarSpec,
    bar_width: u32,
    icons: &mut IconCache,
) -> Result<PaintOutput, AbarError> {
    let font = FontContext::new(&spec.style.font_name, spec.style.font_size)?;
    let computed = crate::layout::compute_bar(spec, bar_width, &|text, is_markup| {
        if is_markup {
            font.measure_markup(text)
        } else {
            font.measure(text)
        }
    });
    let frame = paint_computed(spec, &computed, &font, icons, None, None)?;
    Ok(PaintOutput { frame, computed })
}

pub fn paint_computed(
    spec: &BarSpec,
    computed: &ComputedBar,
    font: &FontContext,
    icons: &mut IconCache,
    hover_seg: Option<(usize, usize)>,
    active_seg: Option<(usize, usize)>,
) -> Result<Frame, AbarError> {
    let width = computed.width;
    let height = computed.height.max(1);
    let stride = width
        .checked_mul(4)
        .ok_or_else(|| AbarError::Render("stride overflow".into()))? as i32;

    let surface = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
        .map_err(|e| AbarError::Render(format!("cairo surface: {e}")))?;

    {
        let cr =
            Context::new(&surface).map_err(|e| AbarError::Render(format!("cairo context: {e}")))?;

        cr.set_operator(Operator::Clear);
        cr.paint()
            .map_err(|e| AbarError::Render(format!("clear: {e}")))?;
        cr.set_operator(Operator::Over);

        for (island_idx, island) in computed.islands.iter().enumerate() {
            let is_single = island.segments.len() == 1;

            // Single-module island: highlight the whole island background.
            // Grouped island: always use normal background; per-segment rects follow.
            let island_bg = if is_single {
                seg_bg(spec, Some((island_idx, 0)), hover_seg, active_seg)
            } else {
                spec.colors.background
            };
            set_source_bgra(&cr, island_bg);
            rounded_rect(
                &cr,
                island.x,
                island.y,
                island.width,
                island.height,
                spec.style.island_radius,
            );
            cr.fill()
                .map_err(|e| AbarError::Render(format!("island fill: {e}")))?;

            // Per-segment highlight rects for grouped islands, clipped to the island shape.
            if !is_single {
                cr.save()
                    .map_err(|e| AbarError::Render(format!("cr save clip: {e}")))?;
                rounded_rect(
                    &cr,
                    island.x,
                    island.y,
                    island.width,
                    island.height,
                    spec.style.island_radius,
                );
                cr.clip();

                let n = island.segments.len();
                for (seg_idx, seg) in island.segments.iter().enumerate() {
                    let bg = seg_bg(spec, Some((island_idx, seg_idx)), hover_seg, active_seg);
                    if bg == spec.colors.background {
                        continue;
                    }
                    // Distribute the segment gap evenly: each segment owns half on each side,
                    // except at the island edges where the full island boundary is used.
                    let half_gap = spec.style.segment_gap / 2.0;
                    let left = if seg_idx == 0 {
                        island.x
                    } else {
                        seg.x - half_gap
                    };
                    let right = if seg_idx == n - 1 {
                        island.x + island.width
                    } else {
                        seg.x + seg.width + half_gap
                    };
                    set_source_bgra(&cr, bg);
                    cr.rectangle(left, island.y, right - left, island.height);
                    cr.fill()
                        .map_err(|e| AbarError::Render(format!("seg highlight fill: {e}")))?;
                }

                cr.restore()
                    .map_err(|e| AbarError::Render(format!("cr restore clip: {e}")))?;
            }

            for seg in &island.segments {
                match seg.display_mode {
                    DisplayMode::TextOnly => draw_segment_text(&cr, font, &spec.colors, seg)?,
                    DisplayMode::IconOnly => {
                        if let Some(name) = &seg.icon_name {
                            draw_segment_icon(&cr, icons, name, spec.style.font_size, seg)?;
                        }
                    }
                }
            }
        }
    }

    let mut data = surface
        .take_data()
        .map_err(|e| AbarError::Render(format!("take_data: {e}")))?
        .to_vec();

    // Cairo may return a larger stride than width * 4; pack tightly for SHM.
    let tight_stride = stride;
    let row_bytes = (width * 4) as usize;
    if data.len() >= (stride * height as i32) as usize && stride as usize != row_bytes {
        let mut tight = vec![0u8; row_bytes * height as usize];
        for row in 0..height as usize {
            let src = row * stride as usize;
            let dst = row * row_bytes;
            tight[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
        }
        data = tight;
    }

    Ok(Frame {
        width,
        height,
        stride: tight_stride,
        data,
    })
}

/// Paint a submenu popup into a CPU buffer.
///
/// Uses the same font, colors and geometry parameters as the bar so no new theming
/// variables are needed.  The caller supplies the pre-computed `item_height` (pixels per
/// row) so that the submenu surface matches the dimensions that were already sent to the
/// compositor via `set_size`.
pub fn paint_submenu(
    items: &[crate::model::SubmenuItemConfig],
    style: &crate::model::BarStyle,
    colors: &BarColors,
    hovered: Option<usize>,
    item_height: f64,
    font: &FontContext,
) -> Result<Frame, AbarError> {
    // Measure the widest label to size the surface.
    let mut max_w = 0.0_f64;
    for item in items {
        let (w, _) = font.measure(&item.content);
        max_w = max_w.max(w);
    }

    let width = ((max_w + 2.0 * style.island_padding_x).ceil() as u32).max(1);
    let height = ((item_height * items.len() as f64).ceil() as u32).max(1);
    let stride = (width * 4) as i32;

    let surface = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
        .map_err(|e| AbarError::Render(format!("submenu surface: {e}")))?;
    {
        let cr = Context::new(&surface)
            .map_err(|e| AbarError::Render(format!("submenu context: {e}")))?;

        cr.set_operator(Operator::Clear);
        cr.paint()
            .map_err(|e| AbarError::Render(format!("submenu clear: {e}")))?;
        cr.set_operator(Operator::Over);

        // Rounded background
        set_source_bgra(&cr, colors.background);
        rounded_rect(
            &cr,
            0.0,
            0.0,
            width as f64,
            height as f64,
            style.island_radius,
        );
        cr.fill()
            .map_err(|e| AbarError::Render(format!("submenu bg fill: {e}")))?;

        // Per-item hover highlights, clipped to the rounded background shape.
        cr.save()
            .map_err(|e| AbarError::Render(format!("submenu clip save: {e}")))?;
        rounded_rect(
            &cr,
            0.0,
            0.0,
            width as f64,
            height as f64,
            style.island_radius,
        );
        cr.clip();
        for (i, _) in items.iter().enumerate() {
            if hovered == Some(i) {
                let bg = colors
                    .hover_background
                    .unwrap_or_else(|| lighten(colors.background, 0.12));
                set_source_bgra(&cr, bg);
                let item_y = i as f64 * item_height;
                cr.rectangle(0.0, item_y, width as f64, item_height);
                cr.fill()
                    .map_err(|e| AbarError::Render(format!("submenu hover fill: {e}")))?;
            }
        }
        cr.restore()
            .map_err(|e| AbarError::Render(format!("submenu clip restore: {e}")))?;

        // Item labels, left-aligned with island_padding_x.
        for (i, item) in items.iter().enumerate() {
            let (_, th) = font.measure(&item.content);
            let item_y = i as f64 * item_height;
            let tx = style.island_padding_x;
            let ty = item_y + (item_height - th) / 2.0;
            cr.move_to(tx, ty);
            set_source_bgra(&cr, colors.foreground);
            show_layout(&cr, font.layout());
        }
    }

    let mut data = surface
        .take_data()
        .map_err(|e| AbarError::Render(format!("submenu take_data: {e}")))?
        .to_vec();

    let row_bytes = (width * 4) as usize;
    if data.len() >= (stride * height as i32) as usize && stride as usize != row_bytes {
        let mut tight = vec![0u8; row_bytes * height as usize];
        for row in 0..height as usize {
            let src = row * stride as usize;
            let dst = row * row_bytes;
            tight[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
        }
        data = tight;
    }

    Ok(Frame {
        width,
        height,
        stride,
        data,
    })
}

fn draw_segment_text(
    cr: &Context,
    font: &FontContext,
    colors: &BarColors,
    seg: &crate::layout::PlacedSegment,
) -> Result<(), AbarError> {
    let (tw, th) = if seg.use_markup {
        font.measure_markup(&seg.label)
    } else {
        font.measure(&seg.label)
    };
    let tx = seg.x + (seg.width - tw) / 2.0;
    let ty = seg.y + (seg.height - th) / 2.0;

    cr.move_to(tx, ty);
    set_source_bgra(cr, colors.foreground);
    show_layout(cr, font.layout());
    Ok(())
}

/// Paint an icon centered within the segment rect.
///
/// Used for custom modules; the same helper will be reused by tray item pixmaps in Phase 7.
fn draw_segment_icon(
    cr: &Context,
    icons: &mut IconCache,
    name: &str,
    size: f64,
    seg: &crate::layout::PlacedSegment,
) -> Result<(), AbarError> {
    let size_px = size.round() as u32;
    let Some(surface) = icons.get(name, size_px) else {
        return Ok(());
    };

    let tx = seg.x + (seg.width - size) / 2.0;
    let ty = seg.y + (seg.height - size) / 2.0;

    cr.save()
        .map_err(|e| AbarError::Render(format!("cr save for icon: {e}")))?;
    cr.set_source_surface(surface, tx, ty)
        .map_err(|e| AbarError::Render(format!("set_source_surface icon: {e}")))?;
    cr.rectangle(tx, ty, size, size);
    cr.fill()
        .map_err(|e| AbarError::Render(format!("fill icon rect: {e}")))?;
    cr.restore()
        .map_err(|e| AbarError::Render(format!("cr restore after icon: {e}")))?;

    Ok(())
}

/// Resolve the effective background color for a segment key given hover/active state.
fn seg_bg(
    spec: &BarSpec,
    key: Option<(usize, usize)>,
    hover: Option<(usize, usize)>,
    active: Option<(usize, usize)>,
) -> [u8; 4] {
    if key == active {
        spec.colors
            .active_background
            .unwrap_or_else(|| lighten(spec.colors.background, 0.25))
    } else if key == hover {
        spec.colors
            .hover_background
            .unwrap_or_else(|| lighten(spec.colors.background, 0.12))
    } else {
        spec.colors.background
    }
}

fn set_source_bgra(cr: &Context, bgra: [u8; 4]) {
    let [b, g, r, a] = bgra;
    cr.set_source_rgba(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        f64::from(a) / 255.0,
    );
}

/// Blend each RGB channel toward 255 (white) by `amount` (0.0–1.0). Alpha is unchanged.
fn lighten(bgra: [u8; 4], amount: f32) -> [u8; 4] {
    let ch = |c: u8| -> u8 {
        let v = f32::from(c);
        (v + (255.0 - v) * amount).round() as u8
    };
    [ch(bgra[0]), ch(bgra[1]), ch(bgra[2]), bgra[3]]
}
