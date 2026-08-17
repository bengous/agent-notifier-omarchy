use std::io;

use crate::app::{focus, Deps};

pub(in crate::app) fn focused_window(deps: &dyn Deps) -> io::Result<()> {
    deps.watch_focused_window(&mut |address| {
        if let Err(error) = focus::mark_address_read(address, deps) {
            eprintln!("agent-notifier: state update failed: {error}");
        }
    })
}
