#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Help,
    Version,
    Hook,
    PiHook,
    ClaudeHook,
    Waybar,
    ListJson,
    ListDisplayJson,
    FocusLatest,
    FocusId(String),
    FocusIdMissing,
    MarkRead(String),
    MarkReadMissing,
    ActiveWindowRead,
    WatchActiveWindow,
    ClearRead,
    ClearAll,
    PruneStale,
    Unknown,
}

impl CliCommand {
    pub(crate) fn from_env() -> Self {
        Self::from_args(std::env::args().skip(1))
    }

    pub(crate) fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = args.into_iter();
        let command = match args.next().as_deref() {
            Some("--help" | "-h") => Self::Help,
            Some("--version" | "-V") => Self::Version,
            Some("hook") => Self::Hook,
            Some("pi-hook") => Self::PiHook,
            Some("claude-hook") => Self::ClaudeHook,
            Some("waybar") => Self::Waybar,
            Some("list-json") => Self::ListJson,
            Some("list-display-json") => Self::ListDisplayJson,
            Some("focus-latest") => Self::FocusLatest,
            Some("focus-id") => args.next().map_or(Self::FocusIdMissing, Self::FocusId),
            Some("mark-read") => args.next().map_or(Self::MarkReadMissing, Self::MarkRead),
            Some("active-window-read") => Self::ActiveWindowRead,
            Some("watch-active-window") => Self::WatchActiveWindow,
            Some("clear-read") => Self::ClearRead,
            Some("clear-all") => Self::ClearAll,
            Some("prune-stale") => Self::PruneStale,
            _ => Self::Unknown,
        };
        if args.next().is_some() {
            return Self::Unknown;
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CliCommand {
        CliCommand::from_args(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn parses_commands_without_arguments() {
        assert_eq!(parse(&["hook"]), CliCommand::Hook);
        assert_eq!(parse(&["pi-hook"]), CliCommand::PiHook);
        assert_eq!(parse(&["claude-hook"]), CliCommand::ClaudeHook);
        assert_eq!(parse(&["waybar"]), CliCommand::Waybar);
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
    fn parses_help_and_version() {
        assert_eq!(parse(&["--help"]), CliCommand::Help);
        assert_eq!(parse(&["-h"]), CliCommand::Help);
        assert_eq!(parse(&["--version"]), CliCommand::Version);
    }

    #[test]
    fn rejects_extra_arguments() {
        assert_eq!(parse(&["waybar", "extra"]), CliCommand::Unknown);
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
}
