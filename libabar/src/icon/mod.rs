use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cairo::ImageSurface;

use crate::error::AbarError;

/// Pixmap cache keyed by icon name; icons are scaled on first load.
///
/// Not `Send` — keep on the main thread alongside the Wayland event loop.
pub struct IconCache {
    entries: HashMap<String, Option<ImageSurface>>,
    search_dirs: Vec<PathBuf>,
    theme_name: String,
}

impl IconCache {
    pub fn new() -> Self {
        let theme_name = std::env::var("XDG_ICON_THEME").unwrap_or_else(|_| "hicolor".to_string());
        Self {
            entries: HashMap::new(),
            search_dirs: default_search_dirs(),
            theme_name,
        }
    }

    /// Construct with explicit search directories (useful for tests).
    pub fn with_dirs(search_dirs: Vec<PathBuf>, theme_name: impl Into<String>) -> Self {
        Self {
            entries: HashMap::new(),
            search_dirs,
            theme_name: theme_name.into(),
        }
    }

    /// Return a cached surface for `name`, loading and scaling to `size` × `size` pixels on first
    /// access. Returns `None` if the icon cannot be found or loaded.
    pub fn get(&mut self, name: &str, size: u32) -> Option<&ImageSurface> {
        if !self.entries.contains_key(name) {
            let dirs = self.search_dirs.clone();
            let theme = self.theme_name.clone();
            let surface = resolve_icon(name, size, &dirs, &theme)
                .and_then(|p| load_icon_file(&p, size).ok().flatten());
            self.entries.insert(name.to_string(), surface);
        }
        self.entries.get(name)?.as_ref()
    }
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a FreeDesktop icon name to a file path (PNG preferred, SVG with `svg` feature).
///
/// Searches `search_dirs` for `theme_name` first, then `hicolor` as fallback, then
/// `/usr/share/pixmaps`.
pub fn resolve_icon(
    name: &str,
    _size: u32,
    search_dirs: &[PathBuf],
    theme_name: &str,
) -> Option<PathBuf> {
    for base in search_dirs {
        if let Some(p) = find_in_theme(base, theme_name, name) {
            return Some(p);
        }
        if theme_name != "hicolor"
            && let Some(p) = find_in_theme(base, "hicolor", name)
        {
            return Some(p);
        }
    }
    // Last-resort pixmaps directory
    find_in_dir(Path::new("/usr/share/pixmaps"), name)
}

/// Returns XDG icon search directories in priority order.
pub fn default_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/icons"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d).join("icons"));
    }
    dirs
}

/// Load a PNG at `path`, scaling it to `size × size` pixels.
/// Returns `None` if `path` does not exist (not an error).
pub fn load_png(path: &Path, size: u32) -> Result<Option<ImageSurface>, AbarError> {
    if !path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(path).map_err(|source| AbarError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = std::io::BufReader::new(file);
    let src = ImageSurface::create_from_png(&mut reader)
        .map_err(|e| AbarError::Render(format!("png load {}: {e}", path.display())))?;

    let src_w = src.width();
    let src_h = src.height();
    if src_w <= 0 || src_h <= 0 {
        return Ok(None);
    }

    let size_i = size as i32;
    let scaled = ImageSurface::create(cairo::Format::ARgb32, size_i, size_i)
        .map_err(|e| AbarError::Render(format!("scaled surface: {e}")))?;
    {
        let cr = cairo::Context::new(&scaled)
            .map_err(|e| AbarError::Render(format!("cairo context for icon scale: {e}")))?;
        cr.scale(
            f64::from(size) / f64::from(src_w),
            f64::from(size) / f64::from(src_h),
        );
        cr.set_source_surface(&src, 0.0, 0.0)
            .map_err(|e| AbarError::Render(format!("set_source_surface for icon: {e}")))?;
        cr.paint()
            .map_err(|e| AbarError::Render(format!("paint icon pixels: {e}")))?;
    }
    Ok(Some(scaled))
}

/// Render an SVG at `path` to a `size × size` Cairo surface.
/// Returns `None` if `path` does not exist (not an error).
#[cfg(feature = "svg")]
pub fn load_svg(path: &Path, size: u32) -> Result<Option<ImageSurface>, AbarError> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(path).map_err(|source| AbarError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())
        .map_err(|e| AbarError::Render(format!("svg parse {}: {e}", path.display())))?;

    let svg_w = tree.size().width();
    let svg_h = tree.size().height();
    if svg_w <= 0.0 || svg_h <= 0.0 {
        return Ok(None);
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| AbarError::Render("failed to allocate svg pixmap".into()))?;
    let transform =
        resvg::tiny_skia::Transform::from_scale(size as f32 / svg_w, size as f32 / svg_h);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny_skia stores premultiplied RGBA; Cairo ARgb32 is premultiplied BGRA on little-endian.
    let stride = cairo::Format::ARgb32
        .stride_for_width(size)
        .map_err(|_| AbarError::Render("svg stride error".into()))?;
    let mut bgra = vec![0u8; stride as usize * size as usize];
    for (i, px) in pixmap.data().chunks_exact(4).enumerate() {
        let row = i / size as usize;
        let col = i % size as usize;
        let off = row * stride as usize + col * 4;
        bgra[off] = px[2]; // B
        bgra[off + 1] = px[1]; // G
        bgra[off + 2] = px[0]; // R
        bgra[off + 3] = px[3]; // A
    }

    let surface = ImageSurface::create_for_data(
        bgra,
        cairo::Format::ARgb32,
        size as i32,
        size as i32,
        stride,
    )
    .map_err(|e| AbarError::Render(format!("cairo surface from svg: {e}")))?;
    Ok(Some(surface))
}

/// Load any supported icon format (PNG, or SVG when the `svg` feature is enabled).
fn load_icon_file(path: &Path, size: u32) -> Result<Option<ImageSurface>, AbarError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => load_png(path, size),
        #[cfg(feature = "svg")]
        Some("svg") => load_svg(path, size),
        _ => Ok(None),
    }
}

fn find_in_theme(base: &Path, theme: &str, name: &str) -> Option<PathBuf> {
    let theme_dir = base.join(theme);
    if !theme_dir.is_dir() {
        return None;
    }
    // Walk size dirs (e.g. 48x48, scalable) then category subdirs (apps/, status/, …).
    let size_entries = std::fs::read_dir(&theme_dir).ok()?;
    for size_entry in size_entries.flatten() {
        let size_dir = size_entry.path();
        if !size_dir.is_dir() {
            continue;
        }
        if let Some(p) = find_in_dir(&size_dir, name) {
            return Some(p);
        }
        let Ok(cat_entries) = std::fs::read_dir(&size_dir) else {
            continue;
        };
        for cat_entry in cat_entries.flatten() {
            let cat_dir = cat_entry.path();
            if !cat_dir.is_dir() {
                continue;
            }
            if let Some(p) = find_in_dir(&cat_dir, name) {
                return Some(p);
            }
        }
    }
    None
}

/// Return the first supported icon file for `name` directly inside `dir`.
/// PNG is preferred over SVG when both exist.
fn find_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let png = dir.join(format!("{name}.png"));
    if png.exists() {
        return Some(png);
    }
    #[cfg(feature = "svg")]
    {
        let svg = dir.join(format!("{name}.svg"));
        if svg.exists() {
            return Some(svg);
        }
    }
    None
}

#[cfg(test)]
mod tests;
