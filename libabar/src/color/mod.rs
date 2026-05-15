//! Hex RGBA strings: `#RRGGBBAA` (eight hex digits, alpha last).

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseHexRgbaError {
    #[error("expected `#RRGGBBAA` (leading `#` and exactly 8 hex digits)")]
    InvalidFormat,
    #[error("non-hex digit in color string")]
    InvalidDigit,
}

/// Parse `#RRGGBBAA` into `[r, g, b, a]` with each channel `0..=255`.
pub fn parse_hex_rgba(s: &str) -> Result<[u8; 4], ParseHexRgbaError> {
    let s = s.trim();
    let hex = s.strip_prefix('#').ok_or(ParseHexRgbaError::InvalidFormat)?;
    if hex.len() != 8 {
        return Err(ParseHexRgbaError::InvalidFormat);
    }
    if !hex.as_bytes().iter().all(|b| b.is_ascii_hexdigit()) {
        return Err(ParseHexRgbaError::InvalidDigit);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| ParseHexRgbaError::InvalidDigit)?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| ParseHexRgbaError::InvalidDigit)?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| ParseHexRgbaError::InvalidDigit)?;
    let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| ParseHexRgbaError::InvalidDigit)?;
    Ok([r, g, b, a])
}

/// Reorder `[r, g, b, a]` → `[b, g, r, a]` for common Wayland SHM `ARGB8888` / `XBGR8888`-style buffers.
#[inline]
pub fn rgba_to_bgra(rgba: [u8; 4]) -> [u8; 4] {
    let [r, g, b, a] = rgba;
    [b, g, r, a]
}

/// Parse `#RRGGBBAA` and return **BGRA** byte order (`[b, g, r, a]`).
pub fn parse_hex_rgba_to_bgra(s: &str) -> Result<[u8; 4], ParseHexRgbaError> {
    Ok(rgba_to_bgra(parse_hex_rgba(s)?))
}

#[cfg(test)]
mod tests;
