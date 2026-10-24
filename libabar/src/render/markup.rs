//! Parser for the Pango markup subset accepted in segment labels.
//!
//! Supported tags: `<span>`, `<b>`, `<i>`, `<u>`, `<s>`, `<big>`, `<small>`, `<tt>`.
//! Supported `<span>` attributes: `foreground`/`fgcolor`/`color`, `background`/`bgcolor`,
//! `fgalpha`/`alpha`, `bgalpha`, `font_family`/`face`, `font_weight`/`weight`,
//! `font_style`/`style`, `font_size`/`size`, `underline`, `strikethrough`.
//! Unknown attributes are ignored; unknown tags and malformed markup are errors.

use std::ops::Range;

/// A text property applied to a byte range of the parsed plain text.
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    /// BGRA foreground color.
    Foreground([u8; 4]),
    /// BGRA background color.
    Background([u8; 4]),
    /// Foreground alpha override (0–255).
    ForegroundAlpha(u8),
    /// Background alpha override (0–255).
    BackgroundAlpha(u8),
    Family(String),
    /// CSS-style weight (100–950).
    Weight(f32),
    Italic(bool),
    /// Absolute font size in pixels.
    Size(f32),
    Underline(bool),
    Strikethrough(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub attrs: Vec<Attr>,
}

/// Plain text with styled spans, ordered by opening tag so inner spans come after outer ones.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub text: String,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarkupError {
    #[error("unterminated tag")]
    UnterminatedTag,
    #[error("unknown tag <{0}>")]
    UnknownTag(String),
    #[error("mismatched closing tag </{0}>")]
    MismatchedClose(String),
    #[error("unclosed tag <{0}>")]
    Unclosed(String),
    #[error("malformed attribute in <{0}>")]
    BadAttribute(String),
    #[error("invalid value {1:?} for attribute {0}")]
    BadValue(String, String),
    #[error("unknown entity &{0};")]
    UnknownEntity(String),
}

/// Parse `markup`, resolving relative sizes (`<big>`, `larger`, …) against `base_size` pixels.
pub fn parse(markup: &str, base_size: f32) -> Result<Parsed, MarkupError> {
    struct Open {
        name: String,
        slot: usize,
        start: usize,
        size: f32,
    }

    let mut text = String::with_capacity(markup.len());
    let mut slots: Vec<Option<Span>> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut rest = markup;

    while let Some(pos) = rest.find(['<', '&']) {
        text.push_str(&rest[..pos]);
        rest = &rest[pos..];
        if rest.starts_with('&') {
            let end = rest
                .find(';')
                .ok_or_else(|| MarkupError::UnknownEntity(rest.into()))?;
            text.push(decode_entity(&rest[1..end])?);
            rest = &rest[end + 1..];
            continue;
        }

        let end = rest.find('>').ok_or(MarkupError::UnterminatedTag)?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];

        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            let open = stack
                .pop()
                .filter(|o| o.name == name)
                .ok_or_else(|| MarkupError::MismatchedClose(name.into()))?;
            if let Some(span) = slots[open.slot].as_mut() {
                span.range = open.start..text.len();
            }
            continue;
        }

        let tag = tag.trim();
        let (name, attr_src) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let current = stack.last().map_or(base_size, |o| o.size);
        let attrs = tag_attrs(name, attr_src, current, base_size)?;
        let size = attrs
            .iter()
            .rev()
            .find_map(|a| match a {
                Attr::Size(s) => Some(*s),
                _ => None,
            })
            .unwrap_or(current);
        stack.push(Open {
            name: name.to_string(),
            slot: slots.len(),
            start: text.len(),
            size,
        });
        slots.push((!attrs.is_empty()).then_some(Span { range: 0..0, attrs }));
    }
    text.push_str(rest);

    if let Some(open) = stack.pop() {
        return Err(MarkupError::Unclosed(open.name));
    }

    let spans = slots
        .into_iter()
        .flatten()
        .filter(|s| !s.range.is_empty())
        .collect();
    Ok(Parsed { text, spans })
}

fn tag_attrs(
    name: &str,
    attr_src: &str,
    current: f32,
    base: f32,
) -> Result<Vec<Attr>, MarkupError> {
    let attrs = match name {
        "span" => return span_attrs(attr_src, current, base),
        "b" => vec![Attr::Weight(700.0)],
        "i" => vec![Attr::Italic(true)],
        "u" => vec![Attr::Underline(true)],
        "s" => vec![Attr::Strikethrough(true)],
        "big" => vec![Attr::Size(current * 1.2)],
        "small" => vec![Attr::Size(current / 1.2)],
        "tt" => vec![Attr::Family("monospace".into())],
        _ => return Err(MarkupError::UnknownTag(name.into())),
    };
    if !attr_src.trim().is_empty() {
        return Err(MarkupError::BadAttribute(name.into()));
    }
    Ok(attrs)
}

fn span_attrs(mut src: &str, current: f32, base: f32) -> Result<Vec<Attr>, MarkupError> {
    let bad = || MarkupError::BadAttribute("span".into());
    let mut attrs = Vec::new();
    loop {
        src = src.trim_start();
        if src.is_empty() {
            return Ok(attrs);
        }
        let eq = src.find('=').ok_or_else(bad)?;
        let key = src[..eq].trim();
        src = src[eq + 1..].trim_start();
        let quote = src
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(bad)?;
        src = &src[1..];
        let close = src.find(quote).ok_or_else(bad)?;
        let raw = &src[..close];
        src = &src[close + 1..];

        let value = decode_entities(raw)?;
        let bad_value = || MarkupError::BadValue(key.into(), value.clone());
        let attr = match key {
            "foreground" | "fgcolor" | "color" => {
                Attr::Foreground(parse_color(&value).ok_or_else(bad_value)?)
            }
            "background" | "bgcolor" => {
                Attr::Background(parse_color(&value).ok_or_else(bad_value)?)
            }
            "fgalpha" | "alpha" => {
                Attr::ForegroundAlpha(parse_alpha(&value).ok_or_else(bad_value)?)
            }
            "bgalpha" => Attr::BackgroundAlpha(parse_alpha(&value).ok_or_else(bad_value)?),
            "font_family" | "face" => Attr::Family(value.clone()),
            "font_weight" | "weight" => Attr::Weight(parse_weight(&value).ok_or_else(bad_value)?),
            "font_style" | "style" => Attr::Italic(match value.as_str() {
                "normal" => false,
                "italic" | "oblique" => true,
                _ => return Err(bad_value()),
            }),
            "font_size" | "size" => {
                Attr::Size(parse_size(&value, current, base).ok_or_else(bad_value)?)
            }
            "underline" => Attr::Underline(value != "none"),
            "strikethrough" => Attr::Strikethrough(parse_bool(&value).ok_or_else(bad_value)?),
            _ => {
                tracing::debug!(attribute = key, "ignoring unsupported span attribute");
                continue;
            }
        };
        attrs.push(attr);
    }
}

fn decode_entities(s: &str) -> Result<String, MarkupError> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let end = rest
            .find(';')
            .ok_or_else(|| MarkupError::UnknownEntity(rest.into()))?;
        out.push(decode_entity(&rest[pos + 1..end])?);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn decode_entity(name: &str) -> Result<char, MarkupError> {
    let numeric = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        u32::from_str_radix(hex, 16).ok()
    } else if let Some(dec) = name.strip_prefix('#') {
        dec.parse().ok()
    } else {
        match name {
            "amp" => return Ok('&'),
            "lt" => return Ok('<'),
            "gt" => return Ok('>'),
            "quot" => return Ok('"'),
            "apos" => return Ok('\''),
            _ => None,
        }
    };
    numeric
        .and_then(char::from_u32)
        .ok_or_else(|| MarkupError::UnknownEntity(name.into()))
}

/// Parse `#rgb`, `#rrggbb`, `#rrggbbaa`, `#rrrrggggbbbb` or a basic color name into BGRA.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let Some(hex) = s.strip_prefix('#') else {
        return named_color(&s.to_ascii_lowercase());
    };
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let [r, g, b, a] = match hex.len() {
        3 => {
            let nib = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|v| v * 17);
            [nib(0)?, nib(1)?, nib(2)?, 255]
        }
        6 => [byte(0)?, byte(2)?, byte(4)?, 255],
        8 => [byte(0)?, byte(2)?, byte(4)?, byte(6)?],
        12 => [byte(0)?, byte(4)?, byte(8)?, 255],
        _ => return None,
    };
    Some([b, g, r, a])
}

fn named_color(name: &str) -> Option<[u8; 4]> {
    let [r, g, b] = match name {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 255, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "cyan" => [0, 255, 255],
        "magenta" => [255, 0, 255],
        "orange" => [255, 165, 0],
        "purple" => [160, 32, 240],
        "pink" => [255, 192, 203],
        "brown" => [165, 42, 42],
        "gray" | "grey" => [190, 190, 190],
        _ => return None,
    };
    Some([b, g, r, 255])
}

/// Pango alpha: `N%` or `1..=65535`.
fn parse_alpha(s: &str) -> Option<u8> {
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.parse().ok()?;
        return Some((v.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    let v: u32 = s.parse().ok()?;
    (1..=65535).contains(&v).then_some((v / 257) as u8)
}

pub fn parse_weight(s: &str) -> Option<f32> {
    let w = match s.to_ascii_lowercase().replace('-', "").as_str() {
        "thin" => 100.0,
        "ultralight" | "extralight" => 200.0,
        "light" => 300.0,
        "semilight" | "demilight" => 350.0,
        "book" => 380.0,
        "normal" | "regular" => 400.0,
        "medium" => 500.0,
        "semibold" | "demibold" => 600.0,
        "bold" => 700.0,
        "ultrabold" | "extrabold" => 800.0,
        "heavy" | "black" => 900.0,
        "ultraheavy" | "extrablack" => 950.0,
        other => other.parse::<f32>().ok()?.clamp(1.0, 1000.0),
    };
    Some(w)
}

/// Pango sizes: units of 1/1024 point, `Npt`, `Npx`, `N%`, or CSS-like keywords.
fn parse_size(s: &str, current: f32, base: f32) -> Option<f32> {
    let px = if let Some(pt) = s.strip_suffix("pt") {
        pt.parse::<f32>().ok()? * super::font::PT_TO_PX
    } else if let Some(px) = s.strip_suffix("px") {
        px.parse::<f32>().ok()?
    } else if let Some(pct) = s.strip_suffix('%') {
        current * pct.parse::<f32>().ok()? / 100.0
    } else if let Ok(units) = s.parse::<f32>() {
        units / 1024.0 * super::font::PT_TO_PX
    } else {
        let scale = match s {
            "xx-small" => 1.0 / (1.2 * 1.2 * 1.2),
            "x-small" => 1.0 / (1.2 * 1.2),
            "small" => 1.0 / 1.2,
            "medium" => 1.0,
            "large" => 1.2,
            "x-large" => 1.2 * 1.2,
            "xx-large" => 1.2 * 1.2 * 1.2,
            "smaller" => return Some(current / 1.2),
            "larger" => return Some(current * 1.2),
            _ => return None,
        };
        base * scale
    };
    (px > 0.0).then_some(px)
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}
