#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Agent {
    Claude,
    Codex,
    Pi,
}

impl Agent {
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        [Self::Claude, Self::Codex, Self::Pi]
            .into_iter()
            .find(|agent| agent.id() == id)
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
        }
    }

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
        }
    }
}

#[cfg(test)]
mod tests;
