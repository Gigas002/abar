//! abar's Wayland event loop, run in a headless sway: what it draws, and
//! what pointer input does. See `common` for the harness.

mod common;

use common::{
    Abar, BAR_BG, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, HOVER_BG, ISLAND_BG, Image, Sway, wait_for,
};

/// A custom module whose icon does not exist, so it shows its name.
fn probe_config(extra: &str) -> String {
    format!(
        r#"
[base]
font_name = "sans-serif"
font_size = 16
[modules]
custom = [
    {{ name = "probe", icon = "abar-test-no-such-icon",
       on_left_click = "echo left >> {{dir}}/clicks",
       on_right_click = "echo right >> {{dir}}/clicks",
       on_middle_click = "echo middle >> {{dir}}/clicks",
       on_scroll_up = "echo up >> {{dir}}/clicks",
       on_scroll_down = "echo down >> {{dir}}/clicks" }},
    {{ name = "menu", icon = "abar-test-no-such-icon", submenu = [
        {{ content = "first", action = "echo first >> {{dir}}/menu" }},
        {{ content = "second", action = "echo second >> {{dir}}/menu" }},
    ] }},
]
{extra}
"#
    )
}

const LAYOUT: &str = r#"
[layout]
left = ["probe"]
right = ["menu"]
"#;

fn top(sway: &Sway) -> Image {
    sway.screenshot(0, 0, common::WIDTH, 80)
}

/// The rows that show some island: the bar's height, as islands fill it.
fn island_rows(img: &Image) -> Vec<u32> {
    (0..img.height)
        .filter(|&y| (0..img.width).any(|x| img.pixel(x, y) == ISLAND_BG))
        .collect()
}

/// `(start, end)` columns of each run of island-colored pixels in row `y`.
fn runs(img: &Image, y: u32, color: [u8; 3]) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start = None;
    for x in 0..=img.width {
        let inside = x < img.width && img.pixel(x, y) == color;
        match (inside, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                runs.push((s, x));
                start = None;
            }
            _ => {}
        }
    }
    runs
}

/// The bar's islands, left to right, and the y of their middle row.
///
/// Islands are found on their second row, above any text.
fn bar_islands(sway: &Sway) -> (Vec<(u32, u32)>, u32) {
    wait_for("islands drawn", || {
        let img = top(sway);
        let rows = island_rows(&img);
        let found = runs(&img, *rows.get(1)?, ISLAND_BG);
        (found.len() >= 2).then_some((found, rows[rows.len() / 2]))
    })
}

#[test]
fn the_bar_reserves_its_height_and_draws_its_islands() {
    let sway = sway_or_skip!();
    let _abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    let (islands, _) = bar_islands(&sway);
    let img = top(&sway);
    let rows = island_rows(&img);
    assert!(rows[0] < 10, "the bar is at the top: {rows:?}");
    assert!((20..60).contains(&rows.len()), "bar height {}", rows.len());
    assert!(islands[0].0 < 50, "probe is on the left: {islands:?}");
    assert!(
        islands[1].1 > common::WIDTH - 50,
        "menu on the right: {islands:?}"
    );
    // The bar background between islands is the theme's bar color, and
    // something (the label) is drawn inside the island.
    assert_eq!(img.pixel(common::WIDTH / 2, rows[rows.len() / 2]), BAR_BG);
    let (a, b) = islands[0];
    let mid = rows[rows.len() / 2];
    assert!(
        (a..b).any(|x| ![ISLAND_BG, BAR_BG].contains(&img.pixel(x, mid))),
        "the probe's label is drawn"
    );
}

#[test]
fn buttons_and_the_wheel_run_the_segment_commands() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    let (islands, y) = bar_islands(&sway);
    let mut pointer = abar.pointer(&sway);
    pointer.move_to((islands[0].0 + islands[0].1) / 2, y);
    pointer.click(BTN_LEFT);
    abar.wait_for_file("clicks", "left");
    pointer.click(BTN_RIGHT);
    abar.wait_for_file("clicks", "right");
    pointer.click(BTN_MIDDLE);
    abar.wait_for_file("clicks", "middle");
    pointer.wheel(-1);
    abar.wait_for_file("clicks", "up");
    pointer.wheel(2);
    let clicks = wait_for("two downs", || {
        let s = abar.wait_for_file("clicks", "down");
        (s.matches("down").count() == 2).then_some(s)
    });
    assert_eq!(clicks, "left\nright\nmiddle\nup\ndown\ndown\n");
}

#[test]
fn hovering_a_segment_highlights_it_until_the_pointer_leaves() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    let (islands, y) = bar_islands(&sway);
    let mut pointer = abar.pointer(&sway);
    pointer.move_to((islands[0].0 + islands[0].1) / 2, y);
    let edge = islands[0].0 + 1;
    let row = island_rows(&top(&sway))[1];
    wait_for("hover highlight", || {
        (top(&sway).pixel(edge, row) == HOVER_BG).then_some(())
    });
    let img = top(&sway);
    assert_eq!(
        img.pixel(islands[1].0 + 1, row),
        ISLAND_BG,
        "only one is lit"
    );
    pointer.move_to(common::WIDTH / 2, 400);
    wait_for("highlight cleared", || {
        let img = top(&sway);
        (img.count(HOVER_BG) == 0).then_some(())
    });
}

#[test]
fn a_submenu_opens_under_its_segment_and_runs_the_picked_item() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    let (islands, y) = bar_islands(&sway);
    let bar_height = abar.bar_height();
    let mut pointer = abar.pointer(&sway);
    pointer.move_to((islands[1].0 + islands[1].1) / 2, y);
    pointer.click(BTN_LEFT);

    // The popup is drawn below the bar, at the right edge where the
    // segment is; find its rows.
    let below = |sway: &Sway| sway.screenshot(0, bar_height, common::WIDTH, 200);
    let rows = wait_for("submenu drawn", || {
        let img = below(&sway);
        let rows = island_rows(&img);
        (!rows.is_empty()).then_some(rows)
    });
    let img = below(&sway);
    let cols = runs(&img, rows[1], ISLAND_BG);
    assert!(
        cols[0].1 > common::WIDTH - 60,
        "popup at the right: {cols:?}"
    );

    // Two items: click in the lower half.
    let second_y = bar_height + rows[0] + rows.len() as u32 * 3 / 4;
    pointer.move_to((cols[0].0 + cols[0].1) / 2, second_y);
    pointer.click(BTN_LEFT);
    assert_eq!(abar.wait_for_file("menu", "second"), "second\n");
    wait_for("submenu closed", || {
        (below(&sway).count(ISLAND_BG) == 0).then_some(())
    });
}

#[test]
fn clicking_outside_an_open_submenu_closes_it_without_running_anything() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    let (islands, y) = bar_islands(&sway);
    let bar_height = abar.bar_height();
    let below = |sway: &Sway| sway.screenshot(0, bar_height, common::WIDTH, 200);
    let mut pointer = abar.pointer(&sway);
    pointer.move_to((islands[1].0 + islands[1].1) / 2, y);
    pointer.click(BTN_LEFT);
    wait_for("submenu drawn", || {
        (below(&sway).count(ISLAND_BG) > 0).then_some(())
    });
    pointer.move_to(200, 500);
    pointer.click(BTN_LEFT);
    wait_for("submenu closed", || {
        (below(&sway).count(ISLAND_BG) == 0).then_some(())
    });
    assert!(!abar.path("menu").exists());
}

#[test]
fn a_new_output_gets_a_bar_and_an_unplugged_one_loses_it() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    bar_islands(&sway);
    sway.msg(&["create_output"]);
    sway.msg(&[
        "output",
        "HEADLESS-2",
        "resolution",
        "640x480",
        "position",
        "1280 0",
    ]);
    abar.wait_for_log_count("second bar painted", "bar painted", 2);
    wait_for("islands on the second output", || {
        let img = sway.screenshot(1280, 0, 640, 60);
        (img.count(ISLAND_BG) > 0).then_some(())
    });
    sway.msg(&["output", "HEADLESS-2", "unplug"]);
    // sway closes the layer surface, then removes the output.
    abar.wait_for_log("second bar closed", "layer surface closed");
    assert!(abar.try_wait().is_none(), "abar keeps running");
    // The first bar still answers.
    let (islands, y) = bar_islands(&sway);
    let mut pointer = abar.pointer(&sway);
    pointer.move_to((islands[0].0 + islands[0].1) / 2, y);
    pointer.click(BTN_LEFT);
    abar.wait_for_file("clicks", "left");
}

#[test]
fn sigterm_exits_cleanly() {
    let sway = sway_or_skip!();
    let mut abar = Abar::start(&sway, &(probe_config("") + LAYOUT));
    bar_islands(&sway);
    assert!(abar.terminate().success(), "{}", abar.log());
}

#[cfg(feature = "window")]
mod exec {
    use super::*;

    /// A `window` module fed from a FIFO, so the test decides when lines arrive.
    fn window_config() -> String {
        probe_config(
            r#"
[window]
exec = "[ -p {dir}/window.fifo ] || mkfifo {dir}/window.fifo; echo $$ > {dir}/script.pid; exec cat {dir}/window.fifo"
"#,
        ) + r#"
[layout]
left = ["probe"]
center = ["window"]
right = ["menu"]
"#
    }

    fn fifo(abar: &Abar) -> std::fs::File {
        let path = abar.path("window.fifo");
        std::fs::OpenOptions::new().write(true).open(path).unwrap()
    }

    #[test]
    fn script_lines_change_the_label_and_stop_with_abar() {
        use std::io::Write;
        let sway = sway_or_skip!();
        let mut abar = Abar::start(&sway, &window_config());
        abar.wait_for_file("script.pid", "\n");
        let width_of_center = |sway: &Sway| {
            let img = top(sway);
            let rows = island_rows(&img);
            runs(&img, rows[1], ISLAND_BG)
                .into_iter()
                .find(|(a, b)| a < &(common::WIDTH / 2) && b > &(common::WIDTH / 2))
                .map(|(a, b)| b - a)
        };
        let mut f = fifo(&abar);
        writeln!(f, r#"{{"text":"short"}}"#).unwrap();
        let short = wait_for("short label", || width_of_center(&sway));
        writeln!(f, r#"{{"text":"a much much longer window title"}}"#).unwrap();
        wait_for("label grows", || {
            width_of_center(&sway).filter(|w| *w > short + 50)
        });

        let pid: i32 = abar
            .wait_for_file("script.pid", "\n")
            .trim()
            .parse()
            .unwrap();
        assert!(abar.terminate().success());
        wait_for("script stopped", || {
            (!std::path::Path::new(&format!("/proc/{pid}")).exists()).then_some(())
        });
    }
}

#[cfg(feature = "tray")]
mod tray {
    use super::*;

    #[test]
    fn tray_items_appear_and_clicks_carry_their_id() {
        let sway = sway_or_skip!();
        let config = probe_config(
            r#"
[tray]
exec = "echo '[{\"app_id\":\"org.test.One\",\"title\":\"One\",\"status\":\"Active\",\"icon_handle\":null},{\"app_id\":\"org.test.Hidden\",\"title\":\"Hidden\",\"status\":\"Passive\",\"icon_handle\":null}]'; exec sleep 600"
feed_id = true
on_left_click = "echo clicked >> {dir}/tray"
"#,
        ) + r#"
[layout]
left = ["probe"]
right = [["tray"], ["menu"]]
"#;
        let mut abar = Abar::start(&sway, &config);
        // probe, tray, menu.
        let (islands, y) = wait_for("tray island", || {
            let (found, y) = bar_islands(&sway);
            (found.len() == 3).then_some((found, y))
        });
        let mut pointer = abar.pointer(&sway);
        pointer.move_to((islands[1].0 + islands[1].1) / 2, y);
        pointer.click(BTN_LEFT);
        assert_eq!(
            abar.wait_for_file("tray", "org.test"),
            "clicked org.test.One\n"
        );
    }
}
