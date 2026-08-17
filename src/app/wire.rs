use std::io;

use crate::app::Deps;
use crate::setup::{wire_line, WireAction, WireTarget};

pub(in crate::app) fn wire(
    target: WireTarget,
    action: WireAction,
    deps: &dyn Deps,
) -> io::Result<()> {
    let outcome = deps.wire_setup(target, action)?;
    deps.print_line(&wire_line(target, &outcome));
    Ok(())
}
