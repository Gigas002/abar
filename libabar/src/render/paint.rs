use std::sync::Arc;

use parley::PositionedLayoutItem;
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, Rect};
use vello_cpu::peniko::ImageSampler;
use vello_cpu::{Glyph, Image, ImageSource, Pixmap, RenderContext};

use crate::error::AbarError;
use crate::icon::IconCache;
use crate::layout::ComputedBar;
use crate::model::{BarColors, BarSpec, DisplayMode};

use super::font::{FontContext, TextBrush};
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
    let mut ctx = new_context(width, height)?;

    for (island_idx, island) in computed.islands.iter().enumerate() {
        let is_single = island.segments.len() == 1;
        let island_path = rounded_rect(
            island.x,
            island.y,
            island.width,
            island.height,
            spec.style.island_radius,
        );

        // Single-module island: highlight the whole island background.
        // Grouped island: always use normal background; per-segment rects follow.
        let island_bg = if is_single {
            seg_bg(spec, Some((island_idx, 0)), hover_seg, active_seg)
        } else {
            spec.colors.background
        };
        ctx.set_paint(color(island_bg));
        ctx.fill_path(&island_path);

        // Per-segment highlight rects for grouped islands, clipped to the island shape.
        if !is_single {
            ctx.push_clip_layer(&island_path);

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
                ctx.set_paint(color(bg));
                ctx.fill_rect(&Rect::new(left, island.y, right, island.y + island.height));
            }

            ctx.pop_layer();
        }

        for seg in &island.segments {
            match seg.display_mode {
                DisplayMode::TextOnly => draw_segment_text(&mut ctx, font, &spec.colors, seg),
                DisplayMode::IconOnly => {
                    if let Some(name) = &seg.icon_name {
                        draw_segment_icon(&mut ctx, icons, name, spec.style.font_size, seg);
                    }
                }
            }
        }
    }

    Ok(finish(&mut ctx, font, width, height))
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
    let mut ctx = new_context(width, height)?;

    // Rounded background
    let bg_path = rounded_rect(0.0, 0.0, width as f64, height as f64, style.island_radius);
    ctx.set_paint(color(colors.background));
    ctx.fill_path(&bg_path);

    // Per-item hover highlights, clipped to the rounded background shape.
    if let Some(i) = hovered.filter(|&i| i < items.len()) {
        let bg = colors
            .hover_background
            .unwrap_or_else(|| lighten(colors.background, 0.12));
        ctx.push_clip_layer(&bg_path);
        ctx.set_paint(color(bg));
        let item_y = i as f64 * item_height;
        ctx.fill_rect(&Rect::new(0.0, item_y, width as f64, item_y + item_height));
        ctx.pop_layer();
    }

    // Item labels, left-aligned with island_padding_x.
    for (i, item) in items.iter().enumerate() {
        let layout = font.layout(&item.content, false);
        let th = f64::from(layout.height());
        let item_y = i as f64 * item_height;
        let tx = style.island_padding_x;
        let ty = item_y + (item_height - th) / 2.0;
        draw_layout(&mut ctx, font, &layout, tx, ty, colors.foreground);
    }

    Ok(finish(&mut ctx, font, width, height))
}

/// Create a render context, checking that the surface fits vello_cpu's `u16` dimensions.
fn new_context(width: u32, height: u32) -> Result<RenderContext, AbarError> {
    let w = u16::try_from(width)
        .map_err(|_| AbarError::Render(format!("surface width {width} too large")))?;
    let h = u16::try_from(height)
        .map_err(|_| AbarError::Render(format!("surface height {height} too large")))?;
    Ok(RenderContext::new(w, h))
}

/// Rasterize `ctx` and convert vello's premultiplied RGBA to the premultiplied BGRA expected by
/// `wl_shm` ARGB8888.
fn finish(ctx: &mut RenderContext, font: &FontContext, width: u32, height: u32) -> Frame {
    ctx.flush();
    let mut pixmap = Pixmap::new(ctx.width(), ctx.height());
    ctx.render(&mut pixmap, &mut font.resources());

    let mut data = pixmap.take_rgba8(vello_cpu::peniko::ImageAlphaType::AlphaPremultiplied);
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Frame {
        width,
        height,
        stride: (width * 4) as i32,
        data,
    }
}

fn draw_segment_text(
    ctx: &mut RenderContext,
    font: &FontContext,
    colors: &BarColors,
    seg: &crate::layout::PlacedSegment,
) {
    let layout = font.layout(&seg.label, seg.use_markup);
    let tw = f64::from(layout.width());
    let th = f64::from(layout.height());
    let tx = seg.x + (seg.width - tw) / 2.0;
    let ty = seg.y + (seg.height - th) / 2.0;
    draw_layout(ctx, font, &layout, tx, ty, colors.foreground);
}

/// Draw a shaped layout with its top-left corner at (`x`, `y`).
fn draw_layout(
    ctx: &mut RenderContext,
    font: &FontContext,
    layout: &parley::Layout<TextBrush>,
    x: f64,
    y: f64,
    foreground: [u8; 4],
) {
    // Snap the origin to whole pixels so hinted glyphs stay crisp.
    let (x, y) = (x.round(), y.round());
    let mut resources = font.resources();

    for line in layout.lines() {
        let metrics = line.metrics();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let style = glyph_run.style();
            let run = glyph_run.run();
            let run_x = x + f64::from(glyph_run.offset());
            let run_end = run_x + f64::from(glyph_run.advance());

            if let Some(bg) = style.brush.background() {
                ctx.set_paint(color(bg));
                ctx.fill_rect(&Rect::new(
                    run_x,
                    y + f64::from(metrics.block_min_coord),
                    run_end,
                    y + f64::from(metrics.block_max_coord),
                ));
            }

            let fg = style.brush.foreground_or(foreground);
            ctx.set_paint(color(fg));

            let coords: Vec<i16> = run
                .normalized_coords()
                .iter()
                .map(|c| c.to_bits())
                .collect();
            let skew = run.synthesis().skew().map_or(Affine::IDENTITY, |deg| {
                Affine::skew(f64::from(deg).to_radians().tan(), 0.0)
            });
            let glyphs = glyph_run.positioned_glyphs().map(|g| Glyph {
                id: g.id,
                x: x as f32 + g.x,
                y: y as f32 + g.y,
            });
            if let Err(e) = ctx
                .glyph_run(&mut resources, run.font())
                .font_size(run.font_size())
                .normalized_coords(&coords)
                .glyph_transform(skew)
                .hint(true)
                .fill_glyphs(glyphs)
            {
                tracing::debug!(error = ?e, "glyph run rendered with errors");
            }

            // Decorations: offsets are relative to the baseline, positive above it.
            let baseline = y + f64::from(glyph_run.baseline());
            let fm = run.font_metrics();
            let decorations = [
                (&style.underline, fm.underline_offset, fm.underline_size),
                (
                    &style.strikethrough,
                    fm.strikethrough_offset,
                    fm.strikethrough_size,
                ),
            ];
            for (deco, default_offset, default_size) in decorations {
                let Some(deco) = deco else { continue };
                let offset = f64::from(deco.offset.unwrap_or(default_offset));
                let size = f64::from(deco.size.unwrap_or(default_size)).max(1.0);
                ctx.set_paint(color(deco.brush.foreground_or(fg)));
                let top = baseline - offset;
                ctx.fill_rect(&Rect::new(run_x, top, run_end, top + size));
            }
        }
    }
}

/// Paint an icon centered within the segment rect.
fn draw_segment_icon(
    ctx: &mut RenderContext,
    icons: &mut IconCache,
    name: &str,
    size: f64,
    seg: &crate::layout::PlacedSegment,
) {
    let size_px = size.round() as u32;
    let Some(pixmap) = icons.get(name, size_px) else {
        return;
    };

    let side = f64::from(size_px);
    let tx = (seg.x + (seg.width - side) / 2.0).round();
    let ty = (seg.y + (seg.height - side) / 2.0).round();

    ctx.set_paint(Image {
        image: ImageSource::Pixmap(Arc::clone(pixmap)),
        sampler: ImageSampler::default(),
    });
    ctx.set_paint_transform(Affine::translate((tx, ty)));
    ctx.fill_rect(&Rect::new(tx, ty, tx + side, ty + side));
    ctx.reset_paint_transform();
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

/// Convert a straight-alpha BGRA color to a vello paint color.
fn color(bgra: [u8; 4]) -> AlphaColor<Srgb> {
    let [b, g, r, a] = bgra;
    AlphaColor::from_rgba8(r, g, b, a)
}

/// Blend each RGB channel toward 255 (white) by `amount` (0.0–1.0). Alpha is unchanged.
fn lighten(bgra: [u8; 4], amount: f32) -> [u8; 4] {
    let ch = |c: u8| -> u8 {
        let v = f32::from(c);
        (v + (255.0 - v) * amount).round() as u8
    };
    [ch(bgra[0]), ch(bgra[1]), ch(bgra[2]), bgra[3]]
}
