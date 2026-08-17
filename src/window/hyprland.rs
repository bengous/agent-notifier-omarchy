use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::event::{AgentEvent, FocusOutcome, ProcessRef, SourceWindow};
use crate::exec::command_output;

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

pub(crate) fn process_ref(pid: i64) -> Option<ProcessRef> {
    Some(ProcessRef {
        pid,
        start_time: read_proc_start_time(pid)?,
    })
}

pub(crate) fn process_is_alive(process: &ProcessRef) -> bool {
    read_proc_start_time(process.pid) == Some(process.start_time)
}

fn read_proc_start_time(pid: i64) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close_paren = stat.rfind(')')?;
    stat[close_paren + 2..]
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
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
                focus_history_id: None,
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

    fn client_with_focus_history(address: &str, focus_history_id: Option<i64>) -> HyprClient {
        HyprClient {
            pid: Some(4682),
            address: Some(address.to_owned()),
            title: None,
            monitor: None,
            monitor_name: None,
            workspace: Some(HyprWorkspace {
                id: Some(3),
                name: Some("3".to_owned()),
            }),
            focus_history_id,
        }
    }

    #[test]
    fn a_process_ref_tracks_liveness_through_proc() -> Result<(), Box<dyn std::error::Error>> {
        let own_pid = i64::from(std::process::id());
        let own = process_ref(own_pid).ok_or("no process ref for the test process")?;

        assert!(process_is_alive(&own));
        assert!(!process_is_alive(&ProcessRef {
            start_time: own.start_time.wrapping_add(1),
            ..own
        }));
        assert_eq!(process_ref(999_999_999), None);
        Ok(())
    }

    #[test]
    fn captures_the_window_shell_as_the_source_process() -> Result<(), Box<dyn std::error::Error>> {
        let own_pid = i64::from(std::process::id());
        let parent_pid = read_proc_parent_pid(own_pid).ok_or("no parent pid")?;
        let clients = vec![HyprClient {
            pid: Some(parent_pid),
            ..client_with_focus_history("0xshell", Some(2))
        }];

        let source_window =
            resolve_source_window_from_pid_chain(own_pid, &clients).ok_or("no source window")?;
        let source = source_window.source_process.ok_or("no source process")?;

        assert_eq!(source.pid, own_pid);
        assert!(process_is_alive(&source));
        Ok(())
    }

    #[test]
    fn a_window_that_is_its_own_hook_parent_gets_no_source_process(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let clients = vec![client_with_focus_history("0xdirect", Some(2))];

        let source_window =
            resolve_source_window_from_pid_chain(4682, &clients).ok_or("no source window")?;

        assert_eq!(source_window.source_process, None);
        Ok(())
    }

    #[test]
    fn captures_ranked_candidate_addresses_for_a_shared_pid(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let clients = vec![
            client_with_focus_history("0xfocused", Some(0)),
            client_with_focus_history("0xold", Some(4)),
            client_with_focus_history("0xrecent", Some(2)),
        ];

        let source_window =
            resolve_source_window_from_pid_chain(4682, &clients).ok_or("no source window")?;

        assert_eq!(source_window.client_address.as_deref(), Some("0xrecent"));
        assert_eq!(
            source_window.client_addresses,
            ["0xrecent", "0xold", "0xfocused"]
        );
        Ok(())
    }

    #[test]
    fn a_candidate_without_an_address_is_left_out_of_the_candidate_list(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addressless = HyprClient {
            address: None,
            ..client_with_focus_history("unused", Some(5))
        };
        let clients = vec![client_with_focus_history("0xrecent", Some(2)), addressless];

        let source_window =
            resolve_source_window_from_pid_chain(4682, &clients).ok_or("no source window")?;

        assert_eq!(source_window.client_addresses, ["0xrecent"]);
        Ok(())
    }

    fn picked_address(clients: &[HyprClient]) -> Option<String> {
        let candidates = clients.iter().collect::<Vec<_>>();
        pick_source_client(&candidates).and_then(|client| client.address.clone())
    }

    #[test]
    fn picks_the_most_recently_focused_background_window() {
        let clients = vec![
            client_with_focus_history("0xold", Some(4)),
            client_with_focus_history("0xrecent", Some(2)),
        ];

        assert_eq!(picked_address(&clients).as_deref(), Some("0xrecent"));
    }

    #[test]
    fn prefers_a_background_window_over_the_focused_one() {
        let clients = vec![
            client_with_focus_history("0xfocused", Some(0)),
            client_with_focus_history("0xbackground", Some(7)),
        ];

        assert_eq!(picked_address(&clients).as_deref(), Some("0xbackground"));
    }

    #[test]
    fn falls_back_to_the_focused_window_when_it_is_the_only_candidate() {
        let clients = vec![client_with_focus_history("0xfocused", Some(0))];

        assert_eq!(picked_address(&clients).as_deref(), Some("0xfocused"));
    }

    #[test]
    fn a_client_without_focus_history_is_the_last_resort() {
        let clients = vec![
            client_with_focus_history("0xunknown", None),
            client_with_focus_history("0xknown", Some(9)),
        ];

        assert_eq!(picked_address(&clients).as_deref(), Some("0xknown"));
    }

    #[test]
    fn reads_the_hyprland_focus_history_field() -> Result<(), Box<dyn std::error::Error>> {
        let clients = parse_clients_output(Some(
            r#"[{"pid":4682,"address":"0xbeef","focusHistoryID":3}]"#.to_owned(),
        ))?;

        assert_eq!(
            clients.first().and_then(|client| client.focus_history_id),
            Some(3)
        );
        Ok(())
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
