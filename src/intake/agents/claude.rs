use crate::intake::agents::{Profile, SessionIdField, TitleSource};

pub(super) const PROFILE: Profile = Profile {
    id: "claude",
    session_id_fields: &[SessionIdField::SessionId],
    session_id_env_var: "CODEX_SESSION_ID",
    title_source: Some(TitleSource::ClaudeTranscript),
};
