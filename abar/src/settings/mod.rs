use libabar::color::parse_hex_rgba_to_bgra;
use libabar::{
    BarColors, BarLayout, BarSpec, BarStyle, DisplayMode, ModuleConfigs, default_search_dirs,
    resolve_icon,
};

use crate::cli::Cli;
use crate::config::{self, Base as ConfigBase, Config};
use crate::error::Error;
#[cfg(feature = "workspaces")]
use crate::theme::Workspaces as ThemeWorkspaces;
use crate::theme::{Base as ThemeBase, Theme};

#[derive(Debug)]
pub struct Settings {
    pub bar: BarSpec,
    pub modules: ModuleConfigs,
}

impl Settings {
    pub fn resolve(_cli: &Cli, config: Config, theme: Theme) -> Result<Self, Error> {
        let modules_cfg = config.modules.as_ref();

        let mut layout = config
            .layout
            .as_ref()
            .map(|l| l.to_bar_layout(modules_cfg))
            .unwrap_or_default();
        config::apply_module_events(&mut layout, &config);

        let theme_base = theme.base.clone().unwrap_or_default();
        let config_base = config.base.clone().unwrap_or_default();

        let background = theme_base
            .background_color
            .unwrap_or_else(|| ThemeBase::default().background_color.unwrap());
        let foreground = theme_base
            .foreground_color
            .unwrap_or_else(|| ThemeBase::default().foreground_color.unwrap());
        let font_name = config_base
            .font_name
            .unwrap_or_else(|| ConfigBase::default().font_name.unwrap());
        let font_size = config_base
            .font_size
            .unwrap_or_else(|| ConfigBase::default().font_size.unwrap());

        apply_icon_fallbacks(&mut layout, font_size);

        // Enable Pango markup on workspaces segments when theme colors are configured.
        #[cfg(feature = "workspaces")]
        apply_markup_for_workspaces(&mut layout, theme.workspaces.as_ref());

        // Build module configs and set live initial labels on the matching segments.
        let module_configs = build_module_configs(&config, &theme, &mut layout);

        let hover_background = theme_base
            .hover_background_color
            .as_deref()
            .map(parse_hex_rgba_to_bgra)
            .transpose()?;
        let active_background = theme_base
            .active_background_color
            .as_deref()
            .map(parse_hex_rgba_to_bgra)
            .transpose()?;

        let defaults = BarStyle::default();
        let island_padding_x = theme_base
            .island_padding_x
            .unwrap_or(defaults.island_padding_x);
        let island_padding_y = theme_base
            .island_padding_y
            .unwrap_or(defaults.island_padding_y);
        let island_radius = theme_base.island_radius.unwrap_or(defaults.island_radius);

        Ok(Self {
            bar: BarSpec::new(
                BarColors {
                    background: parse_hex_rgba_to_bgra(&background)?,
                    foreground: parse_hex_rgba_to_bgra(&foreground)?,
                    hover_background,
                    active_background,
                },
                BarStyle {
                    font_name,
                    font_size,
                    island_padding_x,
                    island_padding_y,
                    island_radius,
                    ..defaults
                },
                layout,
            ),
            modules: module_configs,
        })
    }
}

impl Settings {
    pub fn font_name(&self) -> &str {
        &self.bar.style.font_name
    }

    pub fn font_size(&self) -> f64 {
        self.bar.style.font_size
    }
}

/// Build `ModuleConfigs` from the parsed config and set initial segment labels
/// so the bar shows real data immediately on the first paint.
fn build_module_configs(
    _config: &Config,
    _theme: &Theme,
    _layout: &mut BarLayout,
) -> ModuleConfigs {
    #[cfg(feature = "clock")]
    let clock = {
        use libabar::modules::clock::{ClockConfig, parse_tz};
        _config.clock.as_ref().map(|c| {
            let formats = c
                .formats
                .clone()
                .unwrap_or_else(|| vec!["%H:%M".to_string()]);
            let timezones = c
                .timezones
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(|name| parse_tz(name))
                .collect::<Vec<_>>();
            let cfg = ClockConfig { formats, timezones };

            // Prime the clock segment with the current time so it's never blank.
            let fmt = cfg.formats.first().map_or("%H:%M", |s| s.as_str());
            let tz = cfg.timezones.first().copied();
            let initial = libabar::modules::clock::current_label(fmt, tz);
            set_segment_label(_layout, "clock", &initial);

            cfg
        })
    };

    #[cfg(feature = "keyboard")]
    let keyboard = {
        use libabar::modules::keyboard::KeyboardConfig;
        _config.keyboard.as_ref().map(|k| KeyboardConfig {
            exec: k.exec.clone(),
        })
    };

    #[cfg(feature = "workspaces")]
    let workspaces = {
        use libabar::modules::workspaces::{VisibilityMode, WorkspacesConfig, trim_alpha};
        let theme_ws = _theme.workspaces.as_ref().cloned().unwrap_or_default();
        let visibility_mode = theme_ws
            .visibility_mode
            .as_deref()
            .map(VisibilityMode::parse)
            .unwrap_or_default();
        let active_color = theme_ws.active_color.as_deref().map(trim_alpha);
        let inactive_color = theme_ws.inactive_color.as_deref().map(trim_alpha);
        let exec = _config.workspaces.as_ref().and_then(|w| w.exec.clone());
        let cfg = WorkspacesConfig {
            visibility_mode,
            active_color,
            inactive_color,
            exec,
        };
        // Workspaces shows a placeholder until the background task sends the first update.
        set_segment_label(_layout, "workspaces", "...");
        Some(cfg)
    };

    #[cfg(feature = "window")]
    let window = {
        use libabar::modules::window::WindowConfig;
        let max_length = _config
            .window
            .as_ref()
            .and_then(|w| w.max_length)
            .unwrap_or(50);
        let exec = _config.window.as_ref().and_then(|w| w.exec.clone());
        set_segment_label(_layout, "window", "");
        Some(WindowConfig { max_length, exec })
    };

    #[cfg(feature = "mpris")]
    let mpris = {
        use libabar::modules::mpris::MprisConfig;
        let max_length = _config
            .mpris
            .as_ref()
            .and_then(|m| m.max_length)
            .unwrap_or(0);
        let exec = _config.mpris.as_ref().and_then(|m| m.exec.clone());
        set_segment_label(_layout, "mpris", "");
        Some(MprisConfig { max_length, exec })
    };

    ModuleConfigs {
        #[cfg(feature = "clock")]
        clock,
        #[cfg(feature = "keyboard")]
        keyboard,
        #[cfg(feature = "workspaces")]
        workspaces,
        #[cfg(feature = "window")]
        window,
        #[cfg(feature = "mpris")]
        mpris,
    }
}

#[cfg(any(
    feature = "clock",
    feature = "keyboard",
    feature = "workspaces",
    feature = "window",
    feature = "mpris",
))]
fn set_segment_label(layout: &mut BarLayout, module_id: &str, label: &str) {
    for island in layout
        .left
        .iter_mut()
        .chain(layout.center.iter_mut())
        .chain(layout.right.iter_mut())
    {
        for seg in &mut island.segments {
            if seg.module_id == module_id {
                seg.label = label.to_string();
            }
        }
    }
}

/// Enable Pango markup on workspaces segments when the theme provides `active_color` or
/// `inactive_color`.  Must run before `build_module_configs` sets the initial label.
#[cfg(feature = "workspaces")]
fn apply_markup_for_workspaces(layout: &mut BarLayout, theme_ws: Option<&ThemeWorkspaces>) {
    let has_colors =
        theme_ws.is_some_and(|w| w.active_color.is_some() || w.inactive_color.is_some());
    if !has_colors {
        return;
    }
    for island in layout
        .left
        .iter_mut()
        .chain(layout.center.iter_mut())
        .chain(layout.right.iter_mut())
    {
        for seg in &mut island.segments {
            if seg.module_id == "workspaces" {
                seg.use_markup = true;
            }
        }
    }
}

/// For every icon-only segment whose icon cannot be resolved, switch it to text mode so the
/// module name is displayed instead. Runs once at startup before the bar spec is finalized.
fn apply_icon_fallbacks(layout: &mut BarLayout, font_size: f64) {
    let search_dirs = default_search_dirs();
    let theme_name = std::env::var("XDG_ICON_THEME").unwrap_or_else(|_| "hicolor".to_string());
    apply_icon_fallbacks_with_dirs(layout, font_size, &search_dirs, &theme_name);
}

pub(crate) fn apply_icon_fallbacks_with_dirs(
    layout: &mut BarLayout,
    font_size: f64,
    search_dirs: &[std::path::PathBuf],
    theme_name: &str,
) {
    let size = font_size.round() as u32;

    for island in layout
        .left
        .iter_mut()
        .chain(layout.center.iter_mut())
        .chain(layout.right.iter_mut())
    {
        for seg in &mut island.segments {
            if seg.display_mode != DisplayMode::IconOnly {
                continue;
            }
            let Some(icon_name) = &seg.icon_name else {
                continue;
            };
            if resolve_icon(icon_name, size, search_dirs, theme_name).is_none() {
                tracing::warn!(
                    module = %seg.module_id,
                    icon = %icon_name,
                    "icon not found in theme, falling back to text"
                );
                seg.display_mode = DisplayMode::TextOnly;
            }
        }
    }
}

#[cfg(test)]
mod tests;
