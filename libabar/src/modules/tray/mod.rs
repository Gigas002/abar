use crate::icon::IconCache;
use crate::model::SegmentEvents;

pub mod ipc;

/// Whether the icons an app ships (its SNI `IconThemePath`) are used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppIconTheme {
    /// Only when nothing in the user's icon themes matches (default): the
    /// user's theme always wins.
    #[default]
    Fallback,
    /// Before the user's icon themes.
    Prefer,
    /// Never.
    Never,
}

/// The icon to draw for a tray item, as a name [`IconCache::get`] loads (a
/// theme icon name, or an absolute path into the app's icon directory), or
/// `None` to show text.
///
/// In the user's themes: the item's icon name, then its tooltip title and
/// title lowercased (`"Discord"` → `discord`). The app's own icon directory
/// is searched before or after those, or not at all, as `app_theme` says.
pub fn choose_icon(
    item: &ipc::MinimalTrayItem,
    size: u32,
    app_theme: AppIconTheme,
    cache: &mut IconCache,
) -> Option<String> {
    let from_app = |cache: &mut IconCache| {
        let dir = item.icon_theme_path.as_deref().filter(|d| !d.is_empty())?;
        let name = item.icon_handle.as_deref()?;
        let path = crate::icon::resolve_in_app_dir(dir, name, size, cache.theme_name())?;
        let path = path.to_str()?.to_owned();
        cache.get(&path, size).map(|_| path)
    };
    if app_theme == AppIconTheme::Prefer
        && let Some(path) = from_app(cache)
    {
        return Some(path);
    }
    let guesses = [
        item.icon_handle.clone(),
        item.tooltip_title.as_deref().map(str::to_lowercase),
        item.title.as_deref().map(str::to_lowercase),
    ];
    if let Some(name) = guesses
        .into_iter()
        .flatten()
        .find(|name| cache.get(name, size).is_some())
    {
        return Some(name);
    }
    if app_theme == AppIconTheme::Fallback {
        return from_app(cache);
    }
    None
}

/// Runtime configuration for the tray module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct TrayConfig {
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    /// Each stdout line must be a JSON array of `MinimalTrayItem`.
    pub exec: Option<String>,
    /// When `true`, each item's `app_id` is appended to every configured `on_*`
    /// handler when tray segments are rebuilt, e.g. `tray-menu.sh <app_id>`.
    pub feed_id: bool,
    /// Pointer-action handlers applied to every tray item segment.
    pub events: SegmentEvents,
    /// Whether the icons apps ship are used.
    pub app_icon_theme: AppIconTheme,
}

#[cfg(test)]
mod tests;
