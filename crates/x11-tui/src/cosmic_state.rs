use x11_agent::swarm_events::SwarmEventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CosmicPhase {
    Idle,
    Running,
    Conflict,
    Resolving,
    Verifying,
    Completed,
    Failed,
}

impl CosmicPhase {
    pub fn from_event(kind: &SwarmEventKind) -> Self {
        match kind {
            SwarmEventKind::ConflictDetected => Self::Conflict,
            SwarmEventKind::ResolverStarted | SwarmEventKind::ResolverProposed | SwarmEventKind::ResolverApplied => Self::Resolving,
            SwarmEventKind::ResolverRolledBack | SwarmEventKind::TaskFailed | SwarmEventKind::TaskCancelled | SwarmEventKind::VerificationFailed => Self::Failed,
            SwarmEventKind::VerificationStarted => Self::Verifying,
            SwarmEventKind::VerificationPassed | SwarmEventKind::TaskCompleted => Self::Completed,
            SwarmEventKind::SwarmStarted | SwarmEventKind::TaskStarted | SwarmEventKind::TaskQueued | SwarmEventKind::SwarmResumed => Self::Running,
            SwarmEventKind::SwarmCompleted => Self::Completed,
            SwarmEventKind::TaskBlocked => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_runtime_events_to_visual_phases() {
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::SwarmStarted), CosmicPhase::Running);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::ConflictDetected), CosmicPhase::Conflict);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::ResolverStarted), CosmicPhase::Resolving);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::VerificationStarted), CosmicPhase::Verifying);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::SwarmCompleted), CosmicPhase::Completed);
    }
}
