use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, BufRead, BufReader};
use std::ops::ControlFlow;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::event::{AgentEvent, FocusOutcome, SourceWindow};
use crate::exec::command_output;
use crate::window::proc::{current_parent_pid, pid_chain, process_ref};

const RECONNECT_DELAY: Duration = Duration::from_millis(250);
const RECONNECT_DELAY_MAX: Duration = Duration::from_secs(5);
const MAP_RETRY_DELAY: Duration = Duration::from_millis(100);

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
    best_effort(try_existing_window_addresses())
}

pub(crate) fn try_existing_window_addresses() -> io::Result<HashSet<String>> {
    Ok(try_read_clients()?
        .into_iter()
        .filter_map(|client| client.address)
        .collect())
}

pub(crate) fn focused_window_address() -> Option<String> {
    best_effort(try_read_focused_client().map(|client| client.address))
}

/// The reads that feed the widget degrade to an empty answer: a compositor
/// hiccup must not blank the bar or fail a hook. Reporting the failure is what
/// keeps the swallow honest; every caller that can act on the error takes the
/// `try_` variant instead.
fn best_effort<T: Default>(result: io::Result<T>) -> T {
    result.unwrap_or_else(|error| {
        eprintln!("agent-notifier: {error}");
        T::default()
    })
}

/// Report every focused-window change until the process ends.
pub(crate) fn watch_focused_window(on_change: impl FnMut(&str)) -> io::Result<()> {
    let socket_path = event_socket_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Hyprland event socket not found")
    })?;
    watch_event_stream(
        || UnixStream::connect(&socket_path).map(BufReader::new),
        |delay| {
            thread::sleep(delay);
            ControlFlow::Continue(())
        },
        on_change,
    );
    Ok(())
}

/// A Hyprland restart drops the socket, so the watch reconnects for as long as
/// the daemon lives: `wait` is what ends it, and only a test ever breaks out.
fn watch_event_stream<R: BufRead>(
    mut connect: impl FnMut() -> io::Result<R>,
    mut wait: impl FnMut(Duration) -> ControlFlow<()>,
    mut on_change: impl FnMut(&str),
) {
    let mut delay = RECONNECT_DELAY;
    loop {
        match connect() {
            Ok(stream) => {
                delay = RECONNECT_DELAY;
                report_focused_window_changes(stream, &mut on_change);
            }
            Err(error) => eprintln!("agent-notifier: hyprland socket unavailable: {error}"),
        }
        if wait(delay).is_break() {
            return;
        }
        delay = (delay * 2).min(RECONNECT_DELAY_MAX);
    }
}

fn report_focused_window_changes(stream: impl BufRead, on_change: &mut impl FnMut(&str)) {
    for line in stream.lines() {
        let Ok(line) = line else { break };
        let Some(address) = parse_focused_window_address(&line) else {
            continue;
        };
        on_change(&address);
    }
}

fn parse_focused_window_address(line: &str) -> Option<String> {
    let payload = line.strip_prefix("activewindowv2>>")?.trim();
    if payload.is_empty() || payload == "," {
        return None;
    }
    // hyprctl reports `0x…`; the socket payload may omit the prefix. Normalize to
    // the hyprctl form so stored addresses compare byte-for-byte.
    Some(if payload.starts_with("0x") {
        payload.to_owned()
    } else {
        format!("0x{payload}")
    })
}

fn event_socket_path() -> Option<PathBuf> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")?;
    let signature = env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    Some(
        PathBuf::from(runtime_dir)
            .join("hypr")
            .join(signature)
            .join(".socket2.sock"),
    )
}

/// Resolve the source window, retrying once when the address is missing —
/// `hyprctl clients` can race a window that has just been mapped.
pub(crate) fn current_source_window() -> Option<SourceWindow> {
    let first = read_source_window();
    if first
        .as_ref()
        .is_some_and(|source_window| source_window.client_address.is_some())
    {
        return first;
    }
    thread::sleep(MAP_RETRY_DELAY);
    read_source_window().or(first)
}

fn read_source_window() -> Option<SourceWindow> {
    let clients = attach_monitor_names(read_clients(), &read_monitors());
    resolve_source_window_from_pid_chain(current_parent_pid(), &clients)
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
    best_effort(try_read_clients())
}

fn try_read_clients() -> io::Result<Vec<HyprClient>> {
    hyprctl_json(["hyprctl", "clients", "-j"])
}

fn read_monitors() -> Vec<HyprMonitor> {
    best_effort(try_read_monitors())
}

fn try_read_monitors() -> io::Result<Vec<HyprMonitor>> {
    hyprctl_json(["hyprctl", "monitors", "-j"])
}

fn try_read_focused_client() -> io::Result<HyprClient> {
    hyprctl_json(["hyprctl", "activewindow", "-j"])
}

fn hyprctl_json<T: DeserializeOwned>(argv: [&str; 3]) -> io::Result<T> {
    parse_hyprctl_json(&argv.join(" "), command_output(argv))
}

fn parse_hyprctl_json<T: DeserializeOwned>(command: &str, output: Option<String>) -> io::Result<T> {
    let output = output
        .ok_or_else(|| io::Error::other(format!("failed to query Hyprland with `{command}`")))?;
    serde_json::from_str(&output).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid `{command}` JSON: {error}"),
        )
    })
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
/// the one whose completion `capture_decision` discards.
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
