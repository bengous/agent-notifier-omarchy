use super::*;
use crate::window::proc::process_is_alive;

/// Replay the watch over injected connections: `Some(payload)` is a socket that
/// serves those lines then closes, `None` is a refused connection. The watch
/// stops once every attempt is spent, and reports the addresses it saw and the
/// delay it waited before each reconnection.
fn watch_injected_stream(attempts: &[Option<&str>]) -> (Vec<String>, Vec<Duration>) {
    let mut remaining = attempts.iter();
    let mut addresses = Vec::new();
    let mut delays = Vec::new();
    watch_event_stream(
        || match remaining.next() {
            Some(Some(payload)) => Ok(io::Cursor::new(payload.as_bytes())),
            _ => Err(io::Error::other("connection refused")),
        },
        |delay| {
            delays.push(delay);
            if delays.len() < attempts.len() {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        },
        |address| addresses.push(address.to_owned()),
    );
    (addresses, delays)
}

#[test]
fn parses_focused_window_addresses_from_socket_lines() {
    assert_eq!(
        parse_focused_window_address("activewindowv2>>5934e19c0f30").as_deref(),
        Some("0x5934e19c0f30")
    );
    assert_eq!(
        parse_focused_window_address("activewindowv2>>0x5934e19c0f30").as_deref(),
        Some("0x5934e19c0f30")
    );
    assert_eq!(parse_focused_window_address("activewindowv2>>"), None);
    assert_eq!(parse_focused_window_address("workspace>>3"), None);
}

#[test]
fn reports_every_focused_window_change_of_a_connection() {
    let (addresses, _) = watch_injected_stream(&[Some(
        "activewindowv2>>0xaaa\nworkspace>>3\nactivewindowv2>>bbb\n",
    )]);

    assert_eq!(addresses, ["0xaaa", "0xbbb"]);
}

#[test]
fn a_closed_socket_reconnects_and_keeps_reporting_changes() {
    let (addresses, _) = watch_injected_stream(&[
        Some("activewindowv2>>0xaaa\n"),
        None,
        Some("activewindowv2>>0xccc\n"),
    ]);

    assert_eq!(addresses, ["0xaaa", "0xccc"]);
}

#[test]
fn the_reconnection_delay_doubles_up_to_its_ceiling() {
    let (_, delays) = watch_injected_stream(&[None; 8]);

    assert_eq!(
        delays,
        [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000, 5_000].map(Duration::from_millis)
    );
}

#[test]
fn a_connection_resets_the_reconnection_delay() {
    let (_, delays) =
        watch_injected_stream(&[None, None, Some("activewindowv2>>0xaaa\n"), None, None]);

    assert_eq!(
        delays,
        [250, 500, 250, 500, 1_000].map(Duration::from_millis)
    );
}

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
fn captures_the_window_shell_as_the_source_process() -> Result<(), Box<dyn std::error::Error>> {
    let own_pid = i64::from(std::process::id());
    let clients = vec![HyprClient {
        pid: Some(current_parent_pid()),
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
fn captures_ranked_candidate_addresses_for_a_shared_pid() -> Result<(), Box<dyn std::error::Error>>
{
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

fn parse_clients_output(output: Option<String>) -> io::Result<Vec<HyprClient>> {
    parse_hyprctl_json("hyprctl clients -j", output)
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
        Some("failed to query Hyprland with `hyprctl clients -j`")
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

/// Hyprland answers `{}` when no window holds the focus; that is an answer, not
/// a failed query.
#[test]
fn an_unfocused_compositor_reads_as_a_client_without_an_address(
) -> Result<(), Box<dyn std::error::Error>> {
    let client: HyprClient = parse_hyprctl_json("hyprctl activewindow -j", Some("{}".to_owned()))?;

    assert_eq!(client.address, None);
    Ok(())
}

#[test]
fn a_failed_query_degrades_to_the_empty_answer() {
    assert!(best_effort(parse_clients_output(None)).is_empty());
}
