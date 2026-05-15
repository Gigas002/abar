use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum Error {
    #[error("failed to parse TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("`[base].font` is required and must be non-empty")]
    MissingFont,

    #[error("`[base].theme` is required and must be non-empty")]
    MissingTheme,

    #[error("`[layout].{0}` must be an empty array (no slots yet)")]
    LayoutMustBeEmpty(&'static str),

    #[error(transparent)]
    InvalidHexRgba(#[from] libabar::color::ParseHexRgbaError),
}
