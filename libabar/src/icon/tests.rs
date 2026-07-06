use std::path::PathBuf;

use super::{IconCache, IconLookupMode, load_png, resolve_icon};

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
    let result = resolve_icon("test-icon", 48, &[base], "hicolor", IconLookupMode::Exact);
    assert!(result.is_some(), "expected to resolve test-icon");
}

#[test]
fn resolve_returns_none_for_missing_icon() {
    let (_dir, base) = fixture_theme_dir();
    let result = resolve_icon(
        "nonexistent-icon",
        48,
        &[base],
        "hicolor",
        IconLookupMode::Exact,
    );
    assert!(result.is_none());
}

#[test]
fn resolve_falls_back_to_hicolor_from_other_theme() {
    let (_dir, base) = fixture_theme_dir();
    // Request with a different theme name; should still find via hicolor fallback
    let result = resolve_icon("test-icon", 48, &[base], "Papirus", IconLookupMode::Exact);
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

/// Test that a theme using category-first layout (like candy-icons: `apps/scalable/icon.png`)
/// is resolved correctly when `index.theme` declares the directories.
#[test]
fn resolve_finds_icon_in_category_first_theme() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("icons");

    // Create a candy-icons–style layout: mytheme/apps/scalable/discord.png
    let icon_dir = base.join("mytheme").join("apps").join("scalable");
    std::fs::create_dir_all(&icon_dir).unwrap();
    let icon_path = icon_dir.join("discord.png");
    write_test_png(&icon_path, 48);

    // Write an index.theme that declares the directory.
    let index_content = "[Icon Theme]\nName=mytheme\nDirectories=apps/scalable\n\n[apps/scalable]\nSize=96\nType=Scalable\nMinSize=8\nMaxSize=512\n";
    std::fs::write(base.join("mytheme").join("index.theme"), index_content).unwrap();

    let result = resolve_icon("discord", 24, &[base], "mytheme", IconLookupMode::Exact);
    assert!(
        result.is_some(),
        "should find discord icon via index.theme Directories"
    );
    assert!(result.unwrap().ends_with("discord.png"));
}

/// Test that theme inheritance works (parent theme has the icon, child does not).
#[test]
fn resolve_follows_inherits_from_index_theme() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("icons");

    // Parent theme has the icon.
    let parent_dir = base.join("parent-theme").join("48x48").join("apps");
    std::fs::create_dir_all(&parent_dir).unwrap();
    write_test_png(&parent_dir.join("myapp.png"), 48);

    // Child theme inherits from parent-theme, but has no icons.
    let child_dir = base.join("child-theme");
    std::fs::create_dir_all(&child_dir).unwrap();
    let index_content =
        "[Icon Theme]\nName=child-theme\nInherits=parent-theme,hicolor\nDirectories=\n";
    std::fs::write(child_dir.join("index.theme"), index_content).unwrap();

    let result = resolve_icon("myapp", 48, &[base], "child-theme", IconLookupMode::Exact);
    assert!(
        result.is_some(),
        "should find icon via inherited parent-theme"
    );
}

/// Test that trailing `-` segments are stripped to find a shorter icon name.
/// Uses PreferTheme mode which strips per-theme.
#[test]
fn resolve_strips_trailing_hyphen_segments() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("icons");

    // Theme only has "org.telegram.desktop.png", not the full -mute-symbolic variant.
    let icon_dir = base.join("mytheme").join("apps").join("scalable");
    std::fs::create_dir_all(&icon_dir).unwrap();
    write_test_png(&icon_dir.join("org.telegram.desktop.png"), 48);

    let index_content = "[Icon Theme]\nName=mytheme\nDirectories=apps/scalable\n\n[apps/scalable]\nSize=96\nType=Scalable\n";
    std::fs::write(base.join("mytheme").join("index.theme"), index_content).unwrap();

    // PreferTheme mode: strips per-theme.
    let result = resolve_icon(
        "org.telegram.desktop-mute-symbolic",
        24,
        &[base],
        "mytheme",
        IconLookupMode::PreferTheme,
    );
    assert!(
        result.is_some(),
        "should find icon after stripping -mute-symbolic"
    );
    assert!(
        result.unwrap().ends_with("org.telegram.desktop.png"),
        "should resolve to org.telegram.desktop.png"
    );
}

/// Test that the primary theme with a stripped name takes priority over hicolor's exact match
/// when using PreferTheme mode.
#[test]
fn resolve_prefers_primary_theme_stripped_over_hicolor_exact() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("icons");

    // Primary theme has the stripped name only.
    let primary_dir = base.join("mytheme").join("apps").join("scalable");
    std::fs::create_dir_all(&primary_dir).unwrap();
    write_test_png(&primary_dir.join("org.telegram.desktop.png"), 48);
    let index = "[Icon Theme]\nName=mytheme\nDirectories=apps/scalable\n\n[apps/scalable]\nSize=96\nType=Scalable\n";
    std::fs::write(base.join("mytheme").join("index.theme"), index).unwrap();

    // hicolor has the exact long name.
    let hicolor_dir = base.join("hicolor").join("scalable").join("apps");
    std::fs::create_dir_all(&hicolor_dir).unwrap();
    write_test_png(
        &hicolor_dir.join("org.telegram.desktop-mute-symbolic.png"),
        16,
    );

    // PreferTheme: primary theme's stripped match wins over hicolor exact.
    let result = resolve_icon(
        "org.telegram.desktop-mute-symbolic",
        24,
        &[base.clone()],
        "mytheme",
        IconLookupMode::PreferTheme,
    );
    assert!(result.is_some());
    let path = result.unwrap();
    assert!(
        path.to_string_lossy().contains("mytheme"),
        "PreferTheme: expected primary theme icon, got: {}",
        path.display()
    );

    // Exact mode: hicolor's exact match wins.
    let result = resolve_icon(
        "org.telegram.desktop-mute-symbolic",
        24,
        &[base],
        "mytheme",
        IconLookupMode::Exact,
    );
    assert!(result.is_some());
    let path = result.unwrap();
    assert!(
        path.to_string_lossy().contains("hicolor"),
        "Exact: expected hicolor icon, got: {}",
        path.display()
    );
}

#[cfg(feature = "svg")]
mod svg_tests {
    use std::path::PathBuf;

    use super::super::{IconCache, IconLookupMode, load_svg, resolve_icon};

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
        let result = resolve_icon(
            "test-svg-icon",
            24,
            &[base],
            "hicolor",
            IconLookupMode::Exact,
        );
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
