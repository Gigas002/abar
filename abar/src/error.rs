use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to parse TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("{}: {source}", path.display())]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("{}: {source}", path.display())]
    Invalid {
        path: std::path::PathBuf,
        source: toml::de::Error,
    },

    #[error("font_size must be a positive number of pixels, not {0}")]
    FontSize(f64),

    #[error(transparent)]
    InvalidHexRgba(#[from] libabar::color::ParseHexRgbaError),
}
