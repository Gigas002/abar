use crate::icon::IconCache;
use crate::layout::compute_bar;
use crate::model::{BarColors, BarLayout, BarSpec, BarStyle, Island, Segment};
use crate::render::{FontContext, paint_bar, paint_computed};

fn test_spec(layout: BarLayout) -> BarSpec {
    BarSpec::new(
        BarColors {
            background: [0x25, 0x19, 0x16, 255],
            foreground: [0xed, 0x4d, 0xc7, 255],
        },
        BarStyle {
            font_name: "sans-serif".to_string(),
            font_size: 14.0,
            bar_padding_x: 8.0,
            bar_padding_y: 4.0,
            island_padding_x: 10.0,
            island_padding_y: 4.0,
            island_radius: 8.0,
            island_gap: 8.0,
            segment_gap: 6.0,
        },
        layout,
    )
}

fn pixel_bgra(data: &[u8], stride: i32, x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize) * stride as usize + (x as usize) * 4;
    let mut px = [0u8; 4];
    px.copy_from_slice(&data[offset..offset + 4]);
    px
}

#[test]
fn painted_island_has_background_pixel() {
    let spec = test_spec(BarLayout {
        left: vec![Island {
            segments: vec![Segment::new("clock", "clock")],
        }],
        ..BarLayout::default()
    });
    let font = FontContext::new(&spec.style.font_name, spec.style.font_size).expect("sans-serif");
    let computed = compute_bar(&spec, 400, &|t, _| font.measure(t));
    let mut icons = IconCache::with_dirs(vec![], "hicolor");
    let frame = paint_computed(&spec, &computed, &font, &mut icons).expect("paint");

    let island = &computed.islands[0];
    let sx = (island.x + 3.0) as u32;
    let sy = (island.y + 3.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, sx, sy);
    assert_eq!(px, spec.colors.background);
}

#[test]
fn transparent_gap_between_regions() {
    let spec = test_spec(BarLayout {
        left: vec![Island {
            segments: vec![Segment::new("l", "L")],
        }],
        center: vec![Island {
            segments: vec![Segment::new("c", "C")],
        }],
        right: vec![Island {
            segments: vec![Segment::new("r", "R")],
        }],
    });
    let mut icons = IconCache::with_dirs(vec![], "hicolor");
    let painted = paint_bar(&spec, 600, &mut icons).expect("paint");
    let computed = &painted.computed;
    let frame = &painted.frame;

    let gap_x = (computed.islands[0].x + computed.islands[0].width + 20.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, gap_x.min(frame.width - 1), 4);
    assert_eq!(px, [0, 0, 0, 0]);
}

#[test]
fn icon_segment_paints_non_transparent_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let icon_base = dir.path().join("icons");
    let icon_dir = icon_base.join("hicolor").join("48x48").join("apps");
    std::fs::create_dir_all(&icon_dir).unwrap();
    let icon_path = icon_dir.join("test-module.png");

    // Solid red icon
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 48, 48).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        cr.paint().unwrap();
    }
    let mut file = std::fs::File::create(&icon_path).unwrap();
    surface.write_to_png(&mut file).unwrap();
    drop(file);

    let spec = test_spec(BarLayout {
        left: vec![Island {
            segments: vec![Segment::icon_only("test-module", "test-module")],
        }],
        ..BarLayout::default()
    });
    let mut icons = IconCache::with_dirs(vec![icon_base], "hicolor");
    let painted = paint_bar(&spec, 400, &mut icons).expect("paint");
    let frame = &painted.frame;
    let island = &painted.computed.islands[0];
    let seg = &island.segments[0];

    // Center of the icon segment should have non-zero alpha (the icon was painted).
    let cx = (seg.x + seg.width / 2.0) as u32;
    let cy = (seg.y + seg.height / 2.0) as u32;
    let px = pixel_bgra(
        &frame.data,
        frame.stride,
        cx.min(frame.width - 1),
        cy.min(frame.height - 1),
    );
    assert_ne!(px[3], 0, "icon pixel should be non-transparent");
}
