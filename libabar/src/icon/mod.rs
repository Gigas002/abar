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
/// Searches `search_dirs` for `theme_name` first, then inherited themes from `index.theme`,
/// then `hicolor` as fallback, then `/usr/share/pixmaps`.
///
/// Per the FreeDesktop icon naming spec, if the exact name is not found, trailing `-`
/// segments are stripped progressively (e.g. `app-status-symbolic` → `app-status` → `app`).
/// Stripping is done per-theme so that the user's preferred theme with a shorter name
/// takes priority over a fallback theme with an exact match.
///
/// If `name` starts with `/` and the file exists (with `.png`/`.svg` extension or as-is),
/// it is returned directly without any theme lookup.
pub fn resolve_icon(
    name: &str,
    size: u32,
    search_dirs: &[PathBuf],
    theme_name: &str,
) -> Option<PathBuf> {
    // Absolute path: return directly if it exists.
    if name.starts_with('/') {
        let p = Path::new(name);
        if p.exists() {
            return Some(p.to_path_buf());
        }
        let png = PathBuf::from(format!("{name}.png"));
        if png.exists() {
            return Some(png);
        }
        #[cfg(feature = "svg")]
        {
            let svg = PathBuf::from(format!("{name}.svg"));
            if svg.exists() {
                return Some(svg);
            }
        }
        return None;
    }

    // Collect inherited themes once.
    let mut inherited = Vec::new();
    for base in search_dirs {
        let theme_dir = base.join(theme_name);
        if theme_dir.is_dir() {
            inherited = parse_inherits(&theme_dir);
            break;
        }
    }

    // Strategy: for each theme layer (primary → inherited → hicolor),
    // try the full name then progressively strip trailing `-` segments.
    // This ensures the user's preferred theme wins even with a shorter name
    // over a fallback theme's exact match.

    // 1. Primary theme with all name variants.
    if let Some(p) = resolve_with_stripping(name, size, search_dirs, theme_name) {
        return Some(p);
    }

    // 2. Inherited themes with all name variants.
    for parent_theme in &inherited {
        if parent_theme == "hicolor" {
            continue;
        }
        if let Some(p) = resolve_with_stripping(name, size, search_dirs, parent_theme) {
            return Some(p);
        }
    }

    // 3. hicolor fallback with all name variants.
    if theme_name != "hicolor" {
        if let Some(p) = resolve_with_stripping(name, size, search_dirs, "hicolor") {
            return Some(p);
        }
    }

    // Last-resort pixmaps directory (exact name only).
    find_in_dir(Path::new("/usr/share/pixmaps"), name)
}

/// Try the full icon name, then progressively strip trailing `-` segments,
/// searching a single theme across all search directories.
fn resolve_with_stripping(
    name: &str,
    size: u32,
    search_dirs: &[PathBuf],
    theme: &str,
) -> Option<PathBuf> {
    let mut candidate = name.to_string();
    loop {
        for base in search_dirs {
            if let Some(p) = find_in_theme(base, theme, &candidate, size) {
                return Some(p);
            }
        }
        match candidate.rfind('-') {
            Some(pos) => candidate.truncate(pos),
            None => break,
        }
    }
    None
}

/// Parse the `Inherits=` line from `index.theme` in the given theme directory.
/// Returns the comma-separated theme names, or an empty vec if not found.
pub fn parse_inherits(theme_dir: &Path) -> Vec<String> {
    let index_path = theme_dir.join("index.theme");
    let content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Inherits=") {
            return value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
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

/// Metadata for a directory entry declared in `index.theme`.
struct ThemeDir {
    path: PathBuf,
    size: u32,
    is_scalable: bool,
}

fn find_in_theme(base: &Path, theme: &str, name: &str, size: u32) -> Option<PathBuf> {
    let theme_dir = base.join(theme);
    if !theme_dir.is_dir() {
        return None;
    }

    // Try index.theme–driven lookup first (handles themes like candy-icons that use
    // non-standard layouts such as `apps/scalable/` instead of `scalable/apps/`).
    if let Some(p) = find_in_theme_indexed(&theme_dir, name, size) {
        return Some(p);
    }

    // Fallback: heuristic walk for themes without a usable index.theme.
    find_in_theme_heuristic(&theme_dir, name, size)
}

/// Parse `index.theme` and search the declared `Directories=` paths, sorted by size fit.
fn find_in_theme_indexed(theme_dir: &Path, name: &str, size: u32) -> Option<PathBuf> {
    let index_path = theme_dir.join("index.theme");
    let content = std::fs::read_to_string(&index_path).ok()?;

    // Parse Directories= line.
    let directories_line = content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed.strip_prefix("Directories=")
    })?;

    let dir_names: Vec<&str> = directories_line
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if dir_names.is_empty() {
        return None;
    }

    // Parse per-directory metadata from their [section] blocks.
    let mut theme_dirs: Vec<ThemeDir> = Vec::new();
    for dir_name in &dir_names {
        let section_header = format!("[{dir_name}]");
        let dir_meta = parse_dir_section(&content, &section_header);
        let path = theme_dir.join(dir_name);
        if !path.is_dir() {
            continue;
        }
        theme_dirs.push(ThemeDir {
            path,
            size: dir_meta.0,
            is_scalable: dir_meta.1,
        });
    }

    // Sort by fitness for the requested size.
    sort_theme_dirs(&mut theme_dirs, size);

    // Search each directory.
    for td in &theme_dirs {
        // Skip scalable dirs when svg feature is disabled.
        #[cfg(not(feature = "svg"))]
        if td.is_scalable && td.size == 0 {
            continue;
        }
        if let Some(p) = find_in_dir(&td.path, name) {
            return Some(p);
        }
    }
    None
}

/// Parse a directory's `[section]` block for `Size=` and `Type=` values.
/// Returns (size, is_scalable). Defaults to (0, false) if not found.
fn parse_dir_section(content: &str, section_header: &str) -> (u32, bool) {
    let mut in_section = false;
    let mut size = 0u32;
    let mut is_scalable = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == section_header {
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with('[') {
                break; // next section
            }
            if let Some(val) = trimmed.strip_prefix("Size=") {
                size = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = trimmed.strip_prefix("Type=") {
                is_scalable = val.trim().eq_ignore_ascii_case("Scalable");
            }
        }
    }
    (size, is_scalable)
}

/// Sort directories by fitness: exact size → scalable → larger (less overshoot) → smaller.
fn sort_theme_dirs(dirs: &mut Vec<ThemeDir>, requested: u32) {
    dirs.sort_by(|a, b| {
        fn key(td: &ThemeDir, req: u32) -> (u8, u32) {
            if td.size == req {
                (0, 0)
            } else if td.is_scalable {
                (1, 0)
            } else if td.size == 0 {
                (4, u32::MAX)
            } else if td.size > req {
                (2, td.size - req)
            } else {
                (3, req - td.size)
            }
        }
        key(a, requested).cmp(&key(b, requested))
    });
}

/// Heuristic fallback for themes without `index.theme` or without `Directories=`.
/// Walks `<theme_dir>/<top>/<sub>/` two levels deep, guessing sizes from directory names.
fn find_in_theme_heuristic(theme_dir: &Path, name: &str, size: u32) -> Option<PathBuf> {
    let entries = std::fs::read_dir(theme_dir).ok()?;
    let mut dirs: Vec<(PathBuf, u32)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        let dir_name_str = dir_name.to_string_lossy();
        let parsed = parse_dir_size(&dir_name_str).unwrap_or(0);
        dirs.push((path, parsed));
    }

    // Sort by closeness to requested size.
    dirs.sort_by(|a, b| {
        fn key(dir_size: u32, req: u32) -> (u8, u32) {
            if dir_size == req {
                (0, 0)
            } else if dir_size == u32::MAX {
                (1, 0)
            } else if dir_size == 0 {
                (4, u32::MAX)
            } else if dir_size > req {
                (2, dir_size - req)
            } else {
                (3, req - dir_size)
            }
        }
        key(a.1, size).cmp(&key(b.1, size))
    });

    for (dir, _) in &dirs {
        if let Some(p) = find_in_dir(dir, name) {
            return Some(p);
        }
        let Ok(sub_entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for sub_entry in sub_entries.flatten() {
            let sub_dir = sub_entry.path();
            if !sub_dir.is_dir() {
                continue;
            }
            if let Some(p) = find_in_dir(&sub_dir, name) {
                return Some(p);
            }
        }
    }
    None
}

/// Parse the numeric size from a directory name like "48x48", "32x32", or "scalable".
fn parse_dir_size(dir_name: &str) -> Option<u32> {
    if dir_name == "scalable" {
        #[cfg(feature = "svg")]
        {
            return Some(u32::MAX);
        }
        #[cfg(not(feature = "svg"))]
        {
            return None;
        }
    }
    let parts: Vec<&str> = dir_name.split('x').collect();
    if parts.len() == 2 {
        if let Ok(w) = parts[0].parse::<u32>() {
            return Some(w);
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
