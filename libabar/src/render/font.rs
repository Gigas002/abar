use std::borrow::Cow;
use std::cell::{RefCell, RefMut};

use parley::{
    FontFamily, FontFamilyName, FontStyle, FontWeight, FontWidth, GenericFamily, Layout,
    LayoutContext, StyleProperty,
};
use vello_cpu::Resources;

use crate::error::AbarError;

use super::markup::{self, Attr};

/// Pango treats font sizes as points at 96 DPI; keep the same visual size.
pub(crate) const PT_TO_PX: f32 = 96.0 / 72.0;

/// Per-glyph-run paint data (BGRA) carried through parley. `None` means "use the bar default".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextBrush {
    pub foreground: Option<[u8; 4]>,
    pub foreground_alpha: Option<u8>,
    pub background: Option<[u8; 4]>,
    pub background_alpha: Option<u8>,
}

impl TextBrush {
    pub fn foreground_or(&self, default: [u8; 4]) -> [u8; 4] {
        let [b, g, r, a] = self.foreground.unwrap_or(default);
        [b, g, r, self.foreground_alpha.unwrap_or(a)]
    }

    pub fn background(&self) -> Option<[u8; 4]> {
        let [b, g, r, a] = self.background?;
        Some([b, g, r, self.background_alpha.unwrap_or(a)])
    }
}

/// Font configuration plus the shaping and glyph caches reused across repaints.
///
/// Not `Send` — keep on the main thread alongside the Wayland event loop.
pub struct FontContext {
    families: Vec<FontFamilyName<'static>>,
    weight: FontWeight,
    style: FontStyle,
    width: FontWidth,
    size_px: f32,
    fonts: RefCell<parley::FontContext>,
    layouts: RefCell<LayoutContext<TextBrush>>,
    resources: RefCell<Resources>,
}

impl FontContext {
    /// `font_name` is a Pango-style description (`"Family[, Family…] [Style…]"`); a trailing size
    /// in the description is ignored in favour of `font_size` (points).
    pub fn new(font_name: &str, font_size: f64) -> Result<Self, AbarError> {
        let desc = parse_description(font_name);
        if desc.families.is_empty() {
            return Err(AbarError::Render(format!(
                "no font family in description {font_name:?}"
            )));
        }
        Ok(Self {
            families: desc.families,
            weight: desc.weight,
            style: desc.style,
            width: desc.width,
            size_px: font_size as f32 * PT_TO_PX,
            fonts: RefCell::new(parley::FontContext::new()),
            layouts: RefCell::new(LayoutContext::new()),
            resources: RefCell::new(Resources::new()),
        })
    }

    pub fn measure(&self, text: &str) -> (f64, f64) {
        size_of(&self.layout(text, false))
    }

    /// Measure text that contains Pango markup.
    pub fn measure_markup(&self, markup: &str) -> (f64, f64) {
        size_of(&self.layout(markup, true))
    }

    /// Shape `text` into a single-line layout. Markup that fails to parse is shown verbatim.
    pub(crate) fn layout(&self, text: &str, is_markup: bool) -> Layout<TextBrush> {
        let parsed = if is_markup {
            markup::parse(text, self.size_px).unwrap_or_else(|e| {
                tracing::warn!(error = %e, markup = text, "invalid markup; rendering as plain text");
                markup::Parsed {
                    text: text.to_string(),
                    spans: Vec::new(),
                }
            })
        } else {
            markup::Parsed {
                text: text.to_string(),
                spans: Vec::new(),
            }
        };

        let mut fonts = self.fonts.borrow_mut();
        let mut layouts = self.layouts.borrow_mut();
        let mut builder = layouts.ranged_builder(&mut fonts, &parsed.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(
            &self.families,
        ))));
        builder.push_default(StyleProperty::FontSize(self.size_px));
        builder.push_default(StyleProperty::FontWeight(self.weight));
        builder.push_default(StyleProperty::FontStyle(self.style));
        builder.push_default(StyleProperty::FontWidth(self.width));

        // Brush fields compose across nested spans, so resolve them per span up front.
        let brushes = resolve_brushes(&parsed.spans);
        for (span, brush) in parsed.spans.iter().zip(brushes) {
            let range = span.range.clone();
            if brush != TextBrush::default() {
                builder.push(StyleProperty::Brush(brush), range.clone());
            }
            for attr in &span.attrs {
                let prop = match attr {
                    Attr::Family(name) => {
                        StyleProperty::FontFamily(FontFamily::Source(Cow::Owned(family_css(name))))
                    }
                    Attr::Weight(w) => StyleProperty::FontWeight(FontWeight::new(*w)),
                    Attr::Italic(true) => StyleProperty::FontStyle(FontStyle::Italic),
                    Attr::Italic(false) => StyleProperty::FontStyle(FontStyle::Normal),
                    Attr::Size(px) => StyleProperty::FontSize(*px),
                    Attr::Underline(on) => StyleProperty::Underline(*on),
                    Attr::Strikethrough(on) => StyleProperty::Strikethrough(*on),
                    Attr::Foreground(_)
                    | Attr::Background(_)
                    | Attr::ForegroundAlpha(_)
                    | Attr::BackgroundAlpha(_) => continue,
                };
                builder.push(prop, range.clone());
            }
        }

        let mut layout = builder.build(&parsed.text);
        layout.break_all_lines(None);
        layout
    }

    pub(crate) fn resources(&self) -> RefMut<'_, Resources> {
        self.resources.borrow_mut()
    }
}

fn size_of(layout: &Layout<TextBrush>) -> (f64, f64) {
    (f64::from(layout.width()), f64::from(layout.height()))
}

/// Compute the effective brush for each span, inheriting from the nearest enclosing span so
/// that e.g. an inner `fgalpha` keeps the outer `foreground`.
fn resolve_brushes(spans: &[markup::Span]) -> Vec<TextBrush> {
    let mut out: Vec<TextBrush> = Vec::with_capacity(spans.len());
    for (i, span) in spans.iter().enumerate() {
        // Spans are in opening order, so the enclosing span is the last earlier one containing it.
        let mut brush = (0..i)
            .rev()
            .find(|&j| {
                spans[j].range.start <= span.range.start && span.range.end <= spans[j].range.end
            })
            .map(|j| out[j])
            .unwrap_or_default();
        for attr in &span.attrs {
            match attr {
                Attr::Foreground(c) => brush.foreground = Some(*c),
                Attr::Background(c) => brush.background = Some(*c),
                Attr::ForegroundAlpha(a) => brush.foreground_alpha = Some(*a),
                Attr::BackgroundAlpha(a) => brush.background_alpha = Some(*a),
                _ => {}
            }
        }
        out.push(brush);
    }
    out
}

struct Description {
    families: Vec<FontFamilyName<'static>>,
    weight: FontWeight,
    style: FontStyle,
    width: FontWidth,
}

/// Parse a Pango font description such as `"JetBrains Mono, Noto Sans Bold Italic 12"`.
fn parse_description(desc: &str) -> Description {
    let mut weight = FontWeight::NORMAL;
    let mut style = FontStyle::Normal;
    let mut width = FontWidth::NORMAL;

    let mut words: Vec<&str> = desc.split_whitespace().collect();
    if words.len() > 1
        && let Some(last) = words.last()
        && last.trim_end_matches("px").parse::<f32>().is_ok()
    {
        words.pop();
    }
    // Style words trail the family list; stop at the first word that is not one.
    while words.len() > 1 {
        let word = words[words.len() - 1].to_ascii_lowercase();
        let word = word.replace('-', "");
        match word.as_str() {
            "normal" | "regular" | "roman" => {}
            "italic" => style = FontStyle::Italic,
            "oblique" => style = FontStyle::Oblique(None),
            "ultracondensed" => width = FontWidth::ULTRA_CONDENSED,
            "extracondensed" => width = FontWidth::EXTRA_CONDENSED,
            "condensed" => width = FontWidth::CONDENSED,
            "semicondensed" => width = FontWidth::SEMI_CONDENSED,
            "semiexpanded" => width = FontWidth::SEMI_EXPANDED,
            "expanded" => width = FontWidth::EXPANDED,
            "extraexpanded" => width = FontWidth::EXTRA_EXPANDED,
            "ultraexpanded" => width = FontWidth::ULTRA_EXPANDED,
            w => match markup::parse_weight(w).filter(|_| w.parse::<f32>().is_err()) {
                Some(v) => weight = FontWeight::new(v),
                None => break,
            },
        }
        words.pop();
    }

    let families = words
        .join(" ")
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| family_name(f).into_owned())
        .collect();
    Description {
        families,
        weight,
        style,
        width,
    }
}

/// Map a Pango/fontconfig family name to a parley family, recognising generic aliases.
fn family_name(name: &str) -> FontFamilyName<'_> {
    let generic = match name.to_ascii_lowercase().as_str() {
        "sans" | "sans-serif" => Some(GenericFamily::SansSerif),
        "serif" => Some(GenericFamily::Serif),
        "mono" | "monospace" => Some(GenericFamily::Monospace),
        "cursive" => Some(GenericFamily::Cursive),
        "fantasy" => Some(GenericFamily::Fantasy),
        "system-ui" => Some(GenericFamily::SystemUi),
        "emoji" => Some(GenericFamily::Emoji),
        _ => None,
    };
    generic.map_or(FontFamilyName::named(name), FontFamilyName::Generic)
}

/// Turn a comma-separated family list from markup into a CSS family list.
fn family_css(list: &str) -> String {
    list.split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| match family_name(f) {
            FontFamilyName::Generic(g) => g.to_string(),
            FontFamilyName::Named(n) => format!("\"{}\"", n.replace('"', "")),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
