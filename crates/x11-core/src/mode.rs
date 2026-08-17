use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMode {
    Normal,
    Plan,
    Auto,
    Review,
}

impl Default for AgentMode {
    fn default() -> Self { Self::Normal }
}

impl AgentMode {
    pub fn allows_writes(self) -> bool { matches!(self, Self::Normal | Self::Auto) }
    pub fn allows_shell(self) -> bool { matches!(self, Self::Normal | Self::Plan | Self::Auto) }
    pub fn allows_network(self) -> bool { !matches!(self, Self::Review) }
    pub fn is_read_only(self) -> bool { matches!(self, Self::Plan | Self::Review) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_have_explicit_capability_contract() {
        assert!(!AgentMode::Plan.allows_writes());
        assert!(AgentMode::Plan.allows_shell());
        assert!(!AgentMode::Review.allows_shell());
        assert!(!AgentMode::Review.allows_network());
        assert!(AgentMode::Auto.allows_writes());
        assert!(AgentMode::Review.is_read_only());
    }
}
