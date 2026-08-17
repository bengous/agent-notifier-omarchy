use super::*;

#[test]
fn returns_output_for_a_fast_command() {
    assert_eq!(command_output(["echo", "hello"]).as_deref(), Some("hello"));
}

#[test]
fn gives_up_on_a_command_that_outlives_the_deadline() {
    let start = Instant::now();
    assert!(command_output_with_timeout(["sleep", "5"], Duration::from_millis(200)).is_none());
    assert!(start.elapsed() < Duration::from_secs(2));
}
