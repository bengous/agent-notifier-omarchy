use std::io;

use crate::app::Deps;
use crate::display::event_label;
use crate::event::store::with_state_update;
use crate::event::{append_and_trim, capture_decision, Agent, AgentEvent, CaptureDecision};
use crate::intake;

pub(in crate::app) fn hook(agent: Agent, deps: &dyn Deps) -> io::Result<()> {
    let raw = deps.read_stdin()?;
    let event = intake::capture(agent, &raw, deps.current_source_window(), deps.now());
    match capture_decision(&event, deps.focused_window_address().as_deref()) {
        CaptureDecision::Discard => return Ok(()),
        CaptureDecision::PersistAndAlert => store(&event, deps)?,
        CaptureDecision::AlertOnly => eprintln!(
            "agent-notifier: no Hyprland client address for this completion; alerting without storing"
        ),
    }
    alert(agent, &event, deps);
    Ok(())
}

fn store(event: &AgentEvent, deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
        append_and_trim(state, event.clone())
    })?;
    Ok(())
}

fn alert(agent: Agent, event: &AgentEvent, deps: &dyn Deps) {
    let agent_name = agent.display_name();
    deps.alert(
        agent_name,
        &format!("{agent_name} completed"),
        &event_label(event),
    );
}
