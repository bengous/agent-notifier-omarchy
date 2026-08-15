use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::process::{command_output, run_command, DEFAULT_TIMEOUT};
use crate::state::{AgentEvent, WorkspaceInfo};

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
}

#[derive(Debug, Clone, Deserialize)]
struct HyprMonitor {
    id: Option<i64>,
    name: Option<String>,
}

pub(crate) fn active_window_addresses() -> HashSet<String> {
    try_active_window_addresses().unwrap_or_default()
}

pub(crate) fn try_active_window_addresses() -> io::Result<HashSet<String>> {
    Ok(try_read_clients()?
        .into_iter()
        .filter_map(|client| client.address)
        .collect())
}

pub(crate) fn active_window_address() -> Option<String> {
    read_active_client().and_then(|client| client.address)
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

pub(crate) fn resolve_current_workspace() -> Option<WorkspaceInfo> {
    let clients = attach_monitor_names(read_clients(), &read_monitors());
    resolve_workspace_from_pid_chain(current_parent_pid(), &clients)
}

pub(crate) fn is_active_source_event(event: &AgentEvent) -> bool {
    let Some(source) = event
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.client_address.as_deref())
    else {
        return false;
    };
    active_window_address().is_some_and(|active| active == source)
}

/// Focus the exact source window. Returns false when it cannot be focused.
///
/// There is deliberately no PID or workspace fallback: the old workspace fallback
/// could report success while focusing a different window, and callers now use
/// this result to decide whether to acknowledge the event.
pub(crate) fn focus_event_source(event: Option<&AgentEvent>) -> bool {
    let Some(address) = event
        .and_then(|event| event.workspace.as_ref())
        .and_then(|workspace| workspace.client_address.as_deref())
    else {
        return false;
    };
    let target = format!("address:{address}");
    dispatch_succeeded(
        command_output(["hyprctl", "dispatch", "focuswindow", target.as_str()]).as_deref(),
    )
}

fn dispatch_succeeded(response: Option<&str>) -> bool {
    response == Some("ok")
}

pub(crate) fn focus_center_window(class_name: &str) -> bool {
    run_command(
        &[
            "hyprctl",
            "dispatch",
            "focuswindow",
            &format!("class:{class_name}"),
        ],
        DEFAULT_TIMEOUT,
    )
    .unwrap_or(1)
        == 0
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

fn read_active_client() -> Option<HyprClient> {
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

fn hypr_client_to_workspace(client: &HyprClient) -> Option<WorkspaceInfo> {
    let pid = client.pid?;
    let workspace = client.workspace.as_ref()?;
    Some(WorkspaceInfo {
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
        title: client.title.clone().unwrap_or_default(),
    })
}

fn resolve_workspace_from_pid_chain(
    start_pid: i64,
    clients: &[HyprClient],
) -> Option<WorkspaceInfo> {
    let by_pid = clients
        .iter()
        .filter_map(|client| Some((client.pid?, client)))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    for pid in pid_chain(start_pid, 80) {
        if !seen.insert(pid) {
            break;
        }
        if let Some(client) = by_pid.get(&pid) {
            return hypr_client_to_workspace(client);
        }
    }
    None
}

fn read_proc_parent_pid(pid: i64) -> Option<i64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close_paren = stat.rfind(')')?;
    stat[close_paren + 2..]
        .split_whitespace()
        .nth(1)?
        .parse::<i64>()
        .ok()
        .filter(|parent| *parent > 0)
}

fn current_parent_pid() -> i64 {
    read_proc_parent_pid(i64::from(std::process::id()))
        .unwrap_or_else(|| i64::from(std::process::id()))
}

fn pid_chain(start_pid: i64, max_depth: usize) -> Vec<i64> {
    let mut chain = Vec::new();
    let mut current = Some(start_pid);
    for _ in 0..max_depth {
        let Some(pid) = current else { break };
        if pid <= 0 {
            break;
        }
        chain.push(pid);
        current = read_proc_parent_pid(pid);
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_succeeded_accepts_ok_response() {
        assert!(dispatch_succeeded(Some("ok")));
    }

    #[test]
    fn dispatch_succeeded_rejects_any_other_response() {
        assert!(!dispatch_succeeded(Some("No such window found")));
        assert!(!dispatch_succeeded(Some("")));
        assert!(!dispatch_succeeded(None));
    }

    #[test]
    fn maps_hyprland_monitor_names() {
        let clients = attach_monitor_names(
            vec![HyprClient {
                pid: Some(30),
                address: None,
                title: None,
                monitor: Some(Value::from(0)),
                monitor_name: None,
                workspace: None,
            }],
            &[HyprMonitor {
                id: Some(0),
                name: Some("DP-3".to_owned()),
            }],
        );
        assert_eq!(
            clients
                .first()
                .and_then(|client| client.monitor_name.as_deref()),
            Some("DP-3")
        );
    }

    #[test]
    fn client_query_accepts_successful_empty_output() -> Result<(), Box<dyn std::error::Error>> {
        assert!(parse_clients_output(Some("[]".to_owned()))?.is_empty());
        Ok(())
    }

    #[test]
    fn client_query_rejects_command_failure() {
        let error = parse_clients_output(None)
            .err()
            .map(|error| error.to_string());

        assert_eq!(
            error.as_deref(),
            Some("failed to query Hyprland clients with `hyprctl clients -j`")
        );
    }

    #[test]
    fn client_query_rejects_invalid_json() {
        let error = parse_clients_output(Some("not json".to_owned()))
            .err()
            .map(|error| error.to_string());

        assert!(error
            .as_deref()
            .is_some_and(|error| error.starts_with("invalid `hyprctl clients -j` JSON:")));
    }
}
