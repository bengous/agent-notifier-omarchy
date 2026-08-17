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
