mod app;
mod display;
mod event;
mod exec;
mod intake;
#[cfg(test)]
mod test_fixtures;
mod window;

use crate::app::cli::CliCommand;

const UNAVAILABLE_STATUS_JSON: &str =
    r#"{"text":"agents !","tooltip":"Agent notifier unavailable","class":"error"}"#;
const UNAVAILABLE_STATUS_TOOLTIP: &str = "Agent notifier unavailable";
const STATUS_ERROR_CLASS: &str = "error";

/// Exit code used when a hook cannot persist its event.
///
/// A notifier must never fail an agent turn, so a harness only gets a non-zero
/// code once its semantics are *verified* to surface it non-blockingly. Anything
/// unverified stays at 0. Never return 2: Claude Code treats it as a blocking
/// error on Stop hooks.
///
/// Claude Code documents exit 1 as non-blocking and surfaces stderr:
/// <https://code.claude.com/docs/en/hooks>.
fn hook_failure_exit_code(command: &CliCommand) -> i32 {
    match command {
        CliCommand::Hook | CliCommand::PiHook | CliCommand::StatusJson => 0,
        _ => 1,
    }
}

fn main() {
    let command = CliCommand::from_env();
    match app::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("agent-notifier: {error}");
            if command == CliCommand::StatusJson {
                println!("{UNAVAILABLE_STATUS_JSON}");
            }
            std::process::exit(hook_failure_exit_code(&command));
        }
    }
}
