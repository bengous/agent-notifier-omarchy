#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Agent {
    Claude,
    Codex,
    Pi,
}

impl Agent {
    pub(crate) fn from_id(id: &str) -> Self {
        match id {
            "claude" => Self::Claude,
            "pi" => Self::Pi,
            _ => Self::Codex,
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
        }
    }
}

#[cfg(test)]
mod tests;
