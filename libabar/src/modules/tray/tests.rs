use crate::exec::tests::first_line;
use crate::modules::tray::ipc::{MinimalTrayItem, TrayItemStatus};

/// A tray script line is read as a `Vec<MinimalTrayItem>`.
#[test]
fn tray_exec_handler_receives_item() {
    let items: Vec<MinimalTrayItem> =
        first_line(r#"echo '[{"app_id":"x","status":"Active","icon_handle":"network-wireless"}]'"#);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].app_id, "x");
    assert_eq!(items[0].status, TrayItemStatus::Active);
    assert_eq!(items[0].icon_handle.as_deref(), Some("network-wireless"));
}

#[test]
fn unknown_status_does_not_drop_the_update() {
    let items: Vec<MinimalTrayItem> = serde_json::from_str(
        r#"[{"app_id":"a","status":"Active","icon_handle":null},
            {"app_id":"b","status":"Blinking","icon_handle":null}]"#,
    )
    .unwrap();
    assert_eq!(items[1].status, TrayItemStatus::Unknown);
}

#[test]
fn app_ids_are_passed_to_sh_verbatim() {
    use crate::modules::tray::ipc::shell_quote;
    for id in [
        "plain",
        "with space",
        "it's",
        "$(touch /tmp/x)",
        "`id`;rm",
        "",
    ] {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_quote(id)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), id);
    }
}

#[test]
fn new_trayd_fields_are_read() {
    let items: Vec<MinimalTrayItem> = serde_json::from_str(
        r#"[{"app_id":"a","status":"Active","icon_handle":"x","tooltip_title":"T",
             "tooltip_description":"D","icon_theme_path":"/opt/a/icons",
             "has_pixmap":true,"overlay_has_pixmap":false}]"#,
    )
    .unwrap();
    assert_eq!(items[0].tooltip_description.as_deref(), Some("D"));
    assert_eq!(items[0].icon_theme_path.as_deref(), Some("/opt/a/icons"));
}

mod choose_icon {
    use std::path::Path;

    use super::MinimalTrayItem;
    use crate::icon::IconCache;
    use crate::icon::tests::write_test_png;
    use crate::modules::tray::{AppIconTheme, choose_icon};

    /// A user theme with `user-icon`, and an app directory that ships
    /// `user-icon` and `app-only` as a hicolor theme.
    struct Fixture {
        _dir: tempfile::TempDir,
        cache: IconCache,
        app_dir: String,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("icons/mytheme/22x22/apps");
        let app = dir.path().join("app/hicolor/22x22/apps");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write_test_png(&user.join("user-icon.png"), 22);
        write_test_png(&app.join("user-icon.png"), 22);
        write_test_png(&app.join("app-only.png"), 22);
        Fixture {
            cache: IconCache::with_dirs(vec![dir.path().join("icons")], "mytheme"),
            app_dir: dir.path().join("app").to_str().unwrap().to_owned(),
            _dir: dir,
        }
    }

    fn item(icon: &str, app_dir: &str) -> MinimalTrayItem {
        MinimalTrayItem {
            app_id: "a".into(),
            title: None,
            status: super::TrayItemStatus::Active,
            icon_handle: Some(icon.into()),
            tooltip_title: None,
            tooltip_description: None,
            icon_theme_path: Some(app_dir.into()),
        }
    }

    fn in_app_dir(chosen: &Option<String>, f: &Fixture) -> bool {
        chosen
            .as_deref()
            .is_some_and(|c| Path::new(c).starts_with(&f.app_dir))
    }

    #[test]
    fn by_default_the_user_theme_wins_and_the_app_dir_fills_gaps() {
        let mut f = fixture();
        let (dir, mode) = (f.app_dir.clone(), AppIconTheme::Fallback);
        let both = choose_icon(&item("user-icon", &dir), 22, mode, &mut f.cache);
        assert_eq!(both.as_deref(), Some("user-icon"));
        let only_app = choose_icon(&item("app-only", &dir), 22, mode, &mut f.cache);
        assert!(in_app_dir(&only_app, &f), "{only_app:?}");
        assert!(f.cache.get(only_app.as_deref().unwrap(), 22).is_some());
    }

    #[test]
    fn prefer_searches_the_app_dir_first() {
        let mut f = fixture();
        let dir = f.app_dir.clone();
        let chosen = choose_icon(
            &item("user-icon", &dir),
            22,
            AppIconTheme::Prefer,
            &mut f.cache,
        );
        assert!(in_app_dir(&chosen, &f), "{chosen:?}");
    }

    #[test]
    fn never_ignores_the_app_dir() {
        let mut f = fixture();
        let dir = f.app_dir.clone();
        let chosen = choose_icon(
            &item("app-only", &dir),
            22,
            AppIconTheme::Never,
            &mut f.cache,
        );
        assert_eq!(chosen, None);
    }

    #[test]
    fn app_dirs_and_names_from_the_app_cannot_escape() {
        let mut f = fixture();
        let dir = f.app_dir.clone();
        for (icon, path) in [
            ("../hicolor/22x22/apps/app-only", dir.as_str()),
            ("app-only", "relative/dir"),
        ] {
            let chosen = choose_icon(&item(icon, path), 22, AppIconTheme::Prefer, &mut f.cache);
            assert_eq!(chosen, None, "{icon} in {path}");
        }
    }
}
