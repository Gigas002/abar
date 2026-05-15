use cairo::{Context, Format, ImageSurface, Operator};
use pangocairo::functions::show_layout;

use crate::error::AbarError;
use crate::layout::ComputedBar;
use crate::model::{BarColors, BarSpec};

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
pub fn paint_bar(spec: &BarSpec, bar_width: u32) -> Result<PaintOutput, AbarError> {
    let font = FontContext::new(&spec.style.font_name, spec.style.font_size)?;
    let computed = crate::layout::compute_bar(spec, bar_width, &|text| font.measure(text));
    let frame = paint_computed(spec, &computed, &font)?;
    Ok(PaintOutput { frame, computed })
}

pub fn paint_computed(
    spec: &BarSpec,
    computed: &ComputedBar,
    font: &FontContext,
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

        for island in &computed.islands {
            set_source_bgra(&cr, spec.colors.background);
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
                draw_segment_text(&cr, font, &spec.colors, seg)?;
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
    let layout = font.layout();
    layout.set_text(&seg.label);
    let (tw, th) = font.measure(&seg.label);
    let tx = seg.x + (seg.width - tw) / 2.0;
    let ty = seg.y + (seg.height - th) / 2.0;

    cr.move_to(tx, ty);
    set_source_bgra(cr, colors.foreground);
    show_layout(cr, layout);
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
