#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Help,
    Version,
    Hook,
    PiHook,
    ClaudeHook,
    StatusJson,
    ListJson,
    ListDisplayJson,
    VersionJson,
    FocusLatest,
    FocusId(String),
    FocusIdMissing,
    MarkRead(String),
    MarkReadMissing,
    FocusedWindowRead,
    WatchFocusedWindow,
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
            Some("status-json") => Self::StatusJson,
            Some("list-json") => Self::ListJson,
            Some("list-display-json") => Self::ListDisplayJson,
            Some("version-json") => Self::VersionJson,
            Some("focus-latest") => Self::FocusLatest,
            Some("focus-id") => args.next().map_or(Self::FocusIdMissing, Self::FocusId),
            Some("mark-read") => args.next().map_or(Self::MarkReadMissing, Self::MarkRead),
            Some("focused-window-read") => Self::FocusedWindowRead,
            Some("watch-focused-window") => Self::WatchFocusedWindow,
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
mod tests;
