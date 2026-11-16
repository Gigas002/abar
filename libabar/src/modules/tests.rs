use std::io::Read;
use std::os::unix::net::UnixStream;

use super::{Mailbox, ModuleUpdate};

fn mailbox() -> (Mailbox, UnixStream) {
    let (rx, tx) = UnixStream::pair().unwrap();
    rx.set_nonblocking(true).unwrap();
    (Mailbox::new(tx).unwrap(), rx)
}

#[test]
fn a_burst_keeps_only_the_latest_update_per_module() {
    let (mb, _rx) = mailbox();
    for n in 0..3000 {
        mb.post(ModuleUpdate::text("window", format!("n{n}")));
    }
    mb.post(ModuleUpdate::text("window", "FINAL"));
    mb.post(ModuleUpdate::text("clock", "12:00"));

    let pending = mb.take();
    let texts: Vec<_> = pending
        .modules
        .iter()
        .map(|u| (u.module_id.as_str(), u.text.as_str()))
        .collect();
    assert_eq!(texts, [("window", "FINAL"), ("clock", "12:00")]);
    assert!(mb.take().is_empty());
}

#[test]
fn posting_never_blocks_on_a_full_wakeup_socket() {
    let (mb, mut rx) = mailbox();
    // Far more wakeups than a socket buffer holds, with nobody reading.
    for n in 0..1_000_000 {
        mb.post(ModuleUpdate::text("m", n.to_string()));
    }
    let mut buf = [0u8; 16];
    assert!(rx.read(&mut buf).unwrap() > 0, "the loop was woken");
    assert_eq!(mb.take().modules[0].text, "999999");
}
