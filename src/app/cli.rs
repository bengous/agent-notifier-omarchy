use crate::event::Agent;
use crate::setup::WireTarget;

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
    Doctor,
    DoctorJson,
    Setup(WireTarget),
    SetupRemove(WireTarget),
    SetupMissing,
    SetupUnsupported,
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

    fn from_args<I>(args: I) -> Self
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
            Some("doctor") => args.next().map_or(Self::Doctor, |flag| {
                if flag == "--json" {
                    Self::DoctorJson
                } else {
                    Self::Unknown
                }
            }),
            Some("setup") => args.next().map_or(Self::SetupMissing, |harness| {
                Self::setup(&harness, args.next().as_deref())
            }),
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

    fn setup(harness: &str, flag: Option<&str>) -> Self {
        if matches!(flag, Some(other) if other != "--remove") {
            return Self::Unknown;
        }
        let Some(target) = WireTarget::from_harness_id(harness) else {
            return if Agent::from_id(harness).is_some() {
                Self::SetupUnsupported
            } else {
                Self::Unknown
            };
        };
        if flag.is_some() {
            Self::SetupRemove(target)
        } else {
            Self::Setup(target)
        }
    }
}

#[cfg(test)]
mod tests;
