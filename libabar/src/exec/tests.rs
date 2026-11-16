use std::sync::mpsc;
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::modules::{ModuleUpdate, ScriptLine};
use crate::spawn::ensure_runtime;

/// Run `command` through the exec handler and return the first parsed line.
pub(crate) fn first_line<T: DeserializeOwned + Send + 'static>(command: &str) -> T {
    let (tx, rx) = mpsc::channel();
    ensure_runtime().unwrap().spawn(super::run_exec_handler(
        "test".into(),
        command.into(),
        move |line: T| {
            let _ = tx.send(line);
        },
    ));
    rx.recv_timeout(Duration::from_secs(5))
        .expect("timed out waiting for exec output")
}

/// A script that emits one JSON line delivers it.
#[test]
fn exec_handler_receives_text() {
    let line: ScriptLine = first_line(r#"echo '{"text": "hello"}'"#);
    let update = ModuleUpdate::from_script("test_module", line);
    assert_eq!(update.module_id, "test_module");
    assert_eq!(update.text, "hello");
    assert!(!update.use_markup);
    assert!(update.icon.is_none());
}

/// Optional fields are read when present.
#[test]
fn exec_handler_markup_and_icon() {
    let line: ScriptLine =
        first_line(r#"echo '{"text": "<b>bold</b>", "markup": true, "icon": "network-wireless"}'"#);
    assert_eq!(line.text, "<b>bold</b>");
    assert!(line.markup);
    assert_eq!(line.icon.as_deref(), Some("network-wireless"));
}

/// Per-module data type with a custom field name deserializes correctly;
/// invalid lines before it are skipped.
#[test]
fn exec_handler_per_module_type() {
    #[derive(serde::Deserialize)]
    struct MyData {
        label: String,
    }
    let data: MyData = first_line(r#"echo 'not json'; echo; echo '{"label": "en-US"}'"#);
    assert_eq!(data.label, "en-US");
}
