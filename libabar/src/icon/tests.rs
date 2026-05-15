use std::path::PathBuf;

use super::{IconCache, load_png, resolve_icon};

fn fixture_theme_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("icons");
    // hicolor/48x48/apps/test-icon.png
    let icon_dir = base.join("hicolor").join("48x48").join("apps");
    std::fs::create_dir_all(&icon_dir).unwrap();
    let icon_path = icon_dir.join("test-icon.png");
    write_test_png(&icon_path, 48);
    (dir, base)
}

fn write_test_png(path: &std::path::Path, size: i32) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        cr.paint().unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    surface.write_to_png(&mut file).unwrap();
}

#[test]
fn resolve_finds_icon_in_hicolor() {
    let (_dir, base) = fixture_theme_dir();
    let result = resolve_icon("test-icon", 48, &[base], "hicolor");
    assert!(result.is_some(), "expected to resolve test-icon");
}

#[test]
fn resolve_returns_none_for_missing_icon() {
    let (_dir, base) = fixture_theme_dir();
    let result = resolve_icon("nonexistent-icon", 48, &[base], "hicolor");
    assert!(result.is_none());
}

#[test]
fn resolve_falls_back_to_hicolor_from_other_theme() {
    let (_dir, base) = fixture_theme_dir();
    // Request with a different theme name; should still find via hicolor fallback
    let result = resolve_icon("test-icon", 48, &[base], "Papirus");
    assert!(result.is_some(), "should fall back to hicolor");
}

#[test]
fn load_png_returns_surface_with_correct_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("icon.png");
    write_test_png(&path, 48);

    let surface = load_png(&path, 24).unwrap().unwrap();
    assert_eq!(surface.width(), 24);
    assert_eq!(surface.height(), 24);
}

#[test]
fn load_png_returns_none_for_missing_file() {
    let result = load_png(std::path::Path::new("/nonexistent/icon.png"), 24).unwrap();
    assert!(result.is_none());
}

#[test]
fn icon_cache_returns_surface_on_hit() {
    let (_dir, base) = fixture_theme_dir();
    let mut cache = IconCache::with_dirs(vec![base], "hicolor");
    let surface = cache.get("test-icon", 24);
    assert!(surface.is_some());
}

#[test]
fn icon_cache_returns_none_for_missing() {
    let (_dir, base) = fixture_theme_dir();
    let mut cache = IconCache::with_dirs(vec![base], "hicolor");
    let surface = cache.get("does-not-exist", 24);
    assert!(surface.is_none());
}

#[cfg(feature = "svg")]
mod svg_tests {
    use std::path::PathBuf;

    use super::super::{IconCache, load_svg, resolve_icon};

    fn fixture_svg_theme_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("icons");
        let icon_dir = base.join("hicolor").join("scalable").join("apps");
        std::fs::create_dir_all(&icon_dir).unwrap();
        let svg_path = icon_dir.join("test-svg-icon.svg");
        std::fs::write(
            &svg_path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect width="48" height="48" fill="red"/></svg>"#,
        )
        .unwrap();
        (dir, base)
    }

    #[test]
    fn resolve_finds_svg_icon() {
        let (_dir, base) = fixture_svg_theme_dir();
        let result = resolve_icon("test-svg-icon", 24, &[base], "hicolor");
        assert!(result.is_some());
        assert_eq!(result.unwrap().extension().unwrap(), "svg");
    }

    #[test]
    fn load_svg_returns_surface_with_correct_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect width="48" height="48" fill="red"/></svg>"#,
        )
        .unwrap();

        let surface = load_svg(&path, 24).unwrap().unwrap();
        assert_eq!(surface.width(), 24);
        assert_eq!(surface.height(), 24);
    }

    #[test]
    fn load_svg_returns_none_for_missing_file() {
        let result = load_svg(std::path::Path::new("/nonexistent/icon.svg"), 24).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn icon_cache_loads_svg_via_get() {
        let (_dir, base) = fixture_svg_theme_dir();
        let mut cache = IconCache::with_dirs(vec![base], "hicolor");
        let surface = cache.get("test-svg-icon", 24);
        assert!(surface.is_some());
    }
}
