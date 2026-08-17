use crate::intake::agents::{Profile, SessionIdField};

pub(super) const PROFILE: Profile = Profile {
    id: "pi",
    session_id_fields: &[
        SessionIdField::SessionId,
        SessionIdField::SessionFile,
        SessionIdField::LeafId,
    ],
    session_id_env_var: Some("PI_SESSION_ID"),
    title_source: None,
};
