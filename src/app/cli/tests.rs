use super::*;

fn parse(args: &[&str]) -> CliCommand {
    CliCommand::from_args(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn parses_commands_without_arguments() {
    assert_eq!(parse(&["hook"]), CliCommand::Hook);
    assert_eq!(parse(&["pi-hook"]), CliCommand::PiHook);
    assert_eq!(parse(&["claude-hook"]), CliCommand::ClaudeHook);
    assert_eq!(parse(&["status-json"]), CliCommand::StatusJson);
    assert_eq!(parse(&["version-json"]), CliCommand::VersionJson);
    assert_eq!(parse(&["clear-all"]), CliCommand::ClearAll);
    assert_eq!(parse(&["prune-stale"]), CliCommand::PruneStale);
}

#[test]
fn parses_commands_with_event_id() {
    assert_eq!(
        parse(&["focus-id", "event-1"]),
        CliCommand::FocusId("event-1".to_owned())
    );
    assert_eq!(
        parse(&["mark-read", "event-1"]),
        CliCommand::MarkRead("event-1".to_owned())
    );
}

#[test]
fn parses_doctor_and_its_json_flag() {
    assert_eq!(parse(&["doctor"]), CliCommand::Doctor);
    assert_eq!(parse(&["doctor", "--json"]), CliCommand::DoctorJson);
    assert_eq!(parse(&["doctor", "bogus"]), CliCommand::Unknown);
    assert_eq!(parse(&["doctor", "--json", "extra"]), CliCommand::Unknown);
}

#[test]
fn parses_setup_for_every_wireable_harness() {
    assert_eq!(
        parse(&["setup", "claude"]),
        CliCommand::Setup(WireTarget::Claude)
    );
    assert_eq!(
        parse(&["setup", "codex"]),
        CliCommand::Setup(WireTarget::Codex)
    );
    assert_eq!(
        parse(&["setup", "codex", "--remove"]),
        CliCommand::SetupRemove(WireTarget::Codex)
    );
}

#[test]
fn setup_refuses_the_pi_extension_and_every_unknown_harness() {
    assert_eq!(parse(&["setup", "pi"]), CliCommand::SetupUnsupported);
    assert_eq!(
        parse(&["setup", "pi", "--remove"]),
        CliCommand::SetupUnsupported
    );
    assert_eq!(parse(&["setup", "gemini"]), CliCommand::Unknown);
    assert_eq!(parse(&["setup"]), CliCommand::SetupMissing);
}

#[test]
fn setup_rejects_a_misplaced_flag_and_extra_arguments() {
    assert_eq!(parse(&["setup", "--remove", "claude"]), CliCommand::Unknown);
    assert_eq!(parse(&["setup", "claude", "--force"]), CliCommand::Unknown);
    assert_eq!(
        parse(&["setup", "claude", "--remove", "extra"]),
        CliCommand::Unknown
    );
}

#[test]
fn parses_help_and_version() {
    assert_eq!(parse(&["--help"]), CliCommand::Help);
    assert_eq!(parse(&["-h"]), CliCommand::Help);
    assert_eq!(parse(&["--version"]), CliCommand::Version);
}

#[test]
fn rejects_extra_arguments() {
    assert_eq!(parse(&["status-json", "extra"]), CliCommand::Unknown);
    assert_eq!(parse(&["version-json", "extra"]), CliCommand::Unknown);
    assert_eq!(
        parse(&["focus-id", "event-1", "extra"]),
        CliCommand::Unknown
    );
}

#[test]
fn preserves_missing_argument_as_error_command() {
    assert_eq!(parse(&["focus-id"]), CliCommand::FocusIdMissing);
    assert_eq!(parse(&["mark-read"]), CliCommand::MarkReadMissing);
}

#[test]
fn parses_unknown_or_empty_command() {
    assert_eq!(parse(&["wat"]), CliCommand::Unknown);
    assert_eq!(parse(&[]), CliCommand::Unknown);
}
