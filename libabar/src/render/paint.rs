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
    hover_island: Option<usize>,
    active_island: Option<usize>,
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

        for (idx, island) in computed.islands.iter().enumerate() {
            let bg = if Some(idx) == active_island {
                spec.colors
                    .active_background
                    .unwrap_or_else(|| lighten(spec.colors.background, 0.25))
            } else if Some(idx) == hover_island {
                spec.colors
                    .hover_background
                    .unwrap_or_else(|| lighten(spec.colors.background, 0.12))
            } else {
                spec.colors.background
            };
            set_source_bgra(&cr, bg);
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
