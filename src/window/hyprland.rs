use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::io;
use std::path::PathBuf;

use crate::event::{AgentEvent, FocusOutcome, SourceWindow};
use crate::exec::command_output;
use crate::window::proc::{current_parent_pid, pid_chain, process_ref};

#[derive(Debug, Clone, Deserialize)]
struct HyprWorkspace {
    id: Option<i64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HyprClient {
    pid: Option<i64>,
    address: Option<String>,
    title: Option<String>,
    monitor: Option<Value>,
    #[serde(rename = "monitorName")]
    monitor_name: Option<String>,
    workspace: Option<HyprWorkspace>,
    #[serde(default, rename = "focusHistoryID")]
    focus_history_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct HyprMonitor {
    id: Option<i64>,
    name: Option<String>,
}

pub(crate) fn existing_window_addresses() -> HashSet<String> {
    try_existing_window_addresses().unwrap_or_default()
}

pub(crate) fn try_existing_window_addresses() -> io::Result<HashSet<String>> {
    Ok(try_read_clients()?
        .into_iter()
        .filter_map(|client| client.address)
        .collect())
}

pub(crate) fn focused_window_address() -> Option<String> {
    read_focused_client().and_then(|client| client.address)
}

pub(crate) fn event_socket_path() -> Option<PathBuf> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")?;
    let signature = env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    Some(
        PathBuf::from(runtime_dir)
            .join("hypr")
            .join(signature)
            .join(".socket2.sock"),
    )
}

pub(crate) fn current_source_window() -> Option<SourceWindow> {
    let clients = attach_monitor_names(read_clients(), &read_monitors());
    resolve_source_window_from_pid_chain(current_parent_pid(), &clients)
}

/// Drop a completion only when the source is certain: the candidate set is
/// exactly the focused window. An uncertain set keeps the event, even when the
/// best guess holds the focus.
pub(crate) fn is_focused_source_event(event: &AgentEvent) -> bool {
    let Some(source_window) = event.workspace.as_ref() else {
        return false;
    };
    focused_window_address().is_some_and(|focused| source_window.is_sole_candidate(&focused))
}

/// Focus the first candidate window that still exists.
///
/// There is deliberately no PID or workspace fallback beyond the stored
/// candidates: a fallback that reports success while focusing a different
/// window would consume the event in silence.
pub(crate) fn focus_event_source(event: Option<&AgentEvent>) -> FocusOutcome {
    let Some(source_window) = event.and_then(|event| event.workspace.as_ref()) else {
        return FocusOutcome::NotFocused;
    };
    let focused = source_window
        .candidate_addresses()
        .into_iter()
        .find(|address| {
            let target = format!("hl.dsp.focus({{ window = \"address:{address}\" }})");
            dispatch_succeeded(command_output(["hyprctl", "dispatch", target.as_str()]).as_deref())
        });
    source_window.focus_outcome(focused)
}

fn dispatch_succeeded(response: Option<&str>) -> bool {
    response == Some("ok")
}

fn read_clients() -> Vec<HyprClient> {
    try_read_clients().unwrap_or_default()
}

fn try_read_clients() -> io::Result<Vec<HyprClient>> {
    parse_clients_output(command_output(["hyprctl", "clients", "-j"]))
}

fn parse_clients_output(output: Option<String>) -> io::Result<Vec<HyprClient>> {
    let output = output.ok_or_else(|| {
        io::Error::other("failed to query Hyprland clients with `hyprctl clients -j`")
    })?;
    serde_json::from_str(&output).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid `hyprctl clients -j` JSON: {error}"),
        )
    })
}

fn read_monitors() -> Vec<HyprMonitor> {
    command_output(["hyprctl", "monitors", "-j"])
        .and_then(|output| serde_json::from_str::<Vec<HyprMonitor>>(&output).ok())
        .unwrap_or_default()
}

fn read_focused_client() -> Option<HyprClient> {
    command_output(["hyprctl", "activewindow", "-j"])
        .and_then(|output| serde_json::from_str::<HyprClient>(&output).ok())
}

fn attach_monitor_names(mut clients: Vec<HyprClient>, monitors: &[HyprMonitor]) -> Vec<HyprClient> {
    let names = monitors
        .iter()
        .filter_map(|monitor| Some((monitor.id?, monitor.name.clone()?)))
        .collect::<HashMap<_, _>>();
    for client in &mut clients {
        if client.monitor_name.is_some() {
            continue;
        }
        if let Some(id) = client.monitor.as_ref().and_then(Value::as_i64) {
            client.monitor_name = names.get(&id).cloned();
        }
    }
    clients
}

fn source_window_from_client(client: &HyprClient) -> Option<SourceWindow> {
    let pid = client.pid?;
    let workspace = client.workspace.as_ref()?;
    Some(SourceWindow {
        id: workspace.id?,
        name: workspace.name.clone()?,
        monitor: client
            .monitor_name
            .clone()
            .unwrap_or_else(|| match &client.monitor {
                Some(Value::Number(number)) => number.to_string(),
                Some(Value::String(text)) => text.clone(),
                _ => String::new(),
            }),
        client_pid: pid,
        client_address: client.address.clone(),
        client_addresses: Vec::new(),
        source_process: None,
        title: client.title.clone().unwrap_or_default(),
        extra: serde_json::Map::new(),
    })
}

fn clients_by_pid(clients: &[HyprClient]) -> HashMap<i64, Vec<&HyprClient>> {
    let mut by_pid: HashMap<i64, Vec<&HyprClient>> = HashMap::new();
    for client in clients {
        if let Some(pid) = client.pid {
            by_pid.entry(pid).or_default().push(client);
        }
    }
    by_pid
}

/// A single-process terminal gives every window the same pid, so the source
/// window is not knowable. Prefer an unfocused sibling: the focused window is
/// the one whose completion `is_focused_source_event` drops.
fn source_rank(candidate: &HyprClient) -> (bool, i64) {
    let rank = candidate.focus_history_id.unwrap_or(i64::MAX);
    (rank == 0, rank)
}

fn pick_source_client<'a>(candidates: &[&'a HyprClient]) -> Option<&'a HyprClient> {
    candidates
        .iter()
        .copied()
        .min_by_key(|candidate| source_rank(candidate))
}

fn ranked_addresses(candidates: &[&HyprClient]) -> Vec<String> {
    let mut ranked = candidates.to_vec();
    ranked.sort_by_key(|candidate| source_rank(candidate));
    ranked
        .into_iter()
        .filter_map(|candidate| candidate.address.clone())
        .collect()
}

fn resolve_source_window_from_pid_chain(
    start_pid: i64,
    clients: &[HyprClient],
) -> Option<SourceWindow> {
    let by_pid = clients_by_pid(clients);
    let mut seen = HashSet::new();
    let mut previous = None;
    for pid in pid_chain(start_pid, 80) {
        if !seen.insert(pid) {
            break;
        }
        if let Some(candidates) = by_pid.get(&pid) {
            return pick_source_client(candidates).and_then(|picked| {
                let mut source_window = source_window_from_client(picked)?;
                if source_window.client_address.is_some() {
                    source_window.client_addresses = ranked_addresses(candidates);
                }
                source_window.source_process = previous.and_then(process_ref);
                Some(source_window)
            });
        }
        previous = Some(pid);
    }
    None
}

#[cfg(test)]
mod tests;
