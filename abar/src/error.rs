use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to parse TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error(transparent)]
    InvalidHexRgba(#[from] libabar::color::ParseHexRgbaError),
}
