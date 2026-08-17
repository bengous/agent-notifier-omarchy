mod claude;
mod codex;
mod pi;

use crate::event::Agent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionIdField {
    SessionId,
    SessionFile,
    LeafId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitleSource {
    ClaudeTranscript,
    CodexSessions,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Profile {
    pub(crate) id: &'static str,
    pub(crate) session_id_fields: &'static [SessionIdField],
    pub(crate) session_id_env_var: &'static str,
    pub(crate) title_source: Option<TitleSource>,
}

pub(crate) fn profile(agent: Agent) -> &'static Profile {
    match agent {
        Agent::Claude => &claude::PROFILE,
        Agent::Codex => &codex::PROFILE,
        Agent::Pi => &pi::PROFILE,
    }
}
