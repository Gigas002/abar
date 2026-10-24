use crate::icon::IconCache;
use crate::layout::compute_bar;
use crate::model::{BarColors, BarLayout, BarSpec, BarStyle, Island, Segment};
use crate::render::{FontContext, paint_bar, paint_computed};

fn test_spec(layout: BarLayout) -> BarSpec {
    BarSpec::new(
        BarColors {
            background: [0x25, 0x19, 0x16, 255],
            foreground: [0xed, 0x4d, 0xc7, 255],
            ..BarColors::default()
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
            icon_prefer_theme: false,
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
    let frame = paint_computed(&spec, &computed, &font, &mut icons, None, None).expect("paint");

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
    crate::icon::tests::write_test_png(&icon_path, 48);

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

mod markup {
    use crate::render::markup::{Attr, MarkupError, parse, parse_color};

    #[test]
    fn plain_text_passes_through() {
        let p = parse("hello world", 16.0).unwrap();
        assert_eq!(p.text, "hello world");
        assert!(p.spans.is_empty());
    }

    #[test]
    fn span_foreground_is_bgra() {
        let p = parse(r##"a <span foreground="#00c1e4">1</span> b"##, 16.0).unwrap();
        assert_eq!(p.text, "a 1 b");
        assert_eq!(p.spans.len(), 1);
        assert_eq!(p.spans[0].range, 2..3);
        assert_eq!(
            p.spans[0].attrs,
            vec![Attr::Foreground([0xe4, 0xc1, 0x00, 255])]
        );
    }

    #[test]
    fn entities_are_decoded() {
        let p = parse("&lt;&amp;&gt; &#65;&#x42;", 16.0).unwrap();
        assert_eq!(p.text, "<&> AB");
    }

    #[test]
    fn nested_spans_keep_opening_order() {
        let p = parse("<b>x<i>y</i></b>", 16.0).unwrap();
        assert_eq!(p.text, "xy");
        assert_eq!(p.spans[0].range, 0..2);
        assert_eq!(p.spans[0].attrs, vec![Attr::Weight(700.0)]);
        assert_eq!(p.spans[1].range, 1..2);
        assert_eq!(p.spans[1].attrs, vec![Attr::Italic(true)]);
    }

    #[test]
    fn big_scales_relative_to_enclosing_size() {
        let p = parse("<big><big>x</big></big>", 10.0).unwrap();
        let Attr::Size(inner) = p.spans[1].attrs[0] else {
            panic!("expected size");
        };
        assert!((inner - 14.4).abs() < 1e-4);
    }

    #[test]
    fn malformed_markup_is_rejected() {
        assert_eq!(
            parse("<b>x</i>", 16.0),
            Err(MarkupError::MismatchedClose("i".into()))
        );
        assert_eq!(parse("<b>x", 16.0), Err(MarkupError::Unclosed("b".into())));
        assert_eq!(
            parse("<blink>x</blink>", 16.0),
            Err(MarkupError::UnknownTag("blink".into()))
        );
        assert!(parse("&bogus;", 16.0).is_err());
    }

    #[test]
    fn unknown_span_attributes_are_ignored() {
        let p = parse(r#"<span gravity="south" weight="bold">x</span>"#, 16.0).unwrap();
        assert_eq!(p.spans[0].attrs, vec![Attr::Weight(700.0)]);
    }

    #[test]
    fn color_formats() {
        assert_eq!(parse_color("#f00"), Some([0, 0, 255, 255]));
        assert_eq!(parse_color("#11223380"), Some([0x33, 0x22, 0x11, 0x80]));
        assert_eq!(parse_color("#ffff00000000"), Some([0, 0, 255, 255]));
        assert_eq!(parse_color("White"), Some([255, 255, 255, 255]));
        assert_eq!(parse_color("#12"), None);
    }
}

#[test]
fn markup_foreground_reaches_pixels() {
    let spec = test_spec(BarLayout {
        left: vec![Island {
            segments: vec![
                Segment::new("ws", r##"<span foreground="#00ff00">████</span>"##).with_markup(),
            ],
        }],
        ..BarLayout::default()
    });
    let mut icons = IconCache::with_dirs(vec![], "hicolor");
    let painted = paint_bar(&spec, 400, &mut icons).expect("paint");
    let frame = &painted.frame;
    let seg = &painted.computed.islands[0].segments[0];

    // Some pixel inside the segment must be (mostly) pure green in BGRA.
    let mut found = false;
    for y in seg.y as u32..(seg.y + seg.height) as u32 {
        for x in seg.x as u32..(seg.x + seg.width) as u32 {
            let [b, g, r, a] = pixel_bgra(&frame.data, frame.stride, x, y);
            if a == 255 && g > 200 && r < 40 && b < 40 {
                found = true;
            }
        }
    }
    assert!(found, "expected green glyph pixels from markup span");
}

#[test]
fn text_measures_non_zero() {
    let font = FontContext::new("sans-serif", 14.0).expect("font");
    let (w, h) = font.measure("Hello");
    assert!(w > 0.0 && h > 0.0, "got {w}x{h}");
    let (mw, _) = font.measure_markup("<b>Hello</b>");
    assert!(mw >= w * 0.9, "bold markup width {mw} vs plain {w}");
}

#[test]
fn font_description_strips_style_and_size() {
    // Unknown families still construct; parley falls back to system fonts.
    assert!(FontContext::new("Nonexistent Family Bold Italic 12", 12.0).is_ok());
    assert!(FontContext::new("", 12.0).is_err());
}
