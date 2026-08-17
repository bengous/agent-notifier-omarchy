use super::*;

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
fn the_pid_chain_starts_at_the_given_process_and_climbs_to_its_parent() {
    let own_pid = i64::from(std::process::id());

    let chain = pid_chain(own_pid, 80);

    assert_eq!(chain.first(), Some(&own_pid));
    assert_eq!(chain.get(1), Some(&current_parent_pid()));
}

#[test]
fn the_pid_chain_stops_at_the_requested_depth() {
    assert_eq!(pid_chain(i64::from(std::process::id()), 1).len(), 1);
}

#[test]
fn an_unknown_process_ends_the_chain_and_yields_no_stat_field() {
    assert_eq!(pid_chain(999_999_999, 80), [999_999_999]);
    assert_eq!(proc_stat_field::<i64>(999_999_999, PARENT_PID_FIELD), None);
}

#[test]
fn a_watch_focused_window_cmdline_is_a_listener() {
    assert!(cmdline_is_listener(&[
        "agent-notifier",
        "watch-focused-window"
    ]));
    assert!(cmdline_is_listener(&[
        "/usr/local/bin/agent-notifier",
        "watch-focused-window"
    ]));
}

#[test]
fn any_other_cmdline_is_not_a_listener() {
    assert!(!cmdline_is_listener(&["agent-notifier", "doctor"]));
    assert!(!cmdline_is_listener(&[
        "other-binary",
        "watch-focused-window"
    ]));
    assert!(!cmdline_is_listener(&[
        "agent-notifier",
        "watch-focused-window",
        "extra"
    ]));
    assert!(!cmdline_is_listener(&["agent-notifier"]));
    assert!(!cmdline_is_listener(&[]));
}

#[test]
fn a_stat_field_is_counted_after_the_command_name() -> Result<(), Box<dyn std::error::Error>> {
    let own_pid = i64::from(std::process::id());

    let parent = proc_stat_field::<i64>(own_pid, PARENT_PID_FIELD).ok_or("no parent pid field")?;
    let start_time = proc_stat_field::<u64>(own_pid, START_TIME_FIELD).ok_or("no start time")?;

    assert_eq!(parent, current_parent_pid());
    assert_eq!(
        process_ref(own_pid).map(|own| own.start_time),
        Some(start_time)
    );
    Ok(())
}
