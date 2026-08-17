use std::collections::BTreeMap;

use x11_agent::swarm_events::{SwarmEvent, SwarmEventKind};

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
            SwarmEventKind::ResolverStarted
            | SwarmEventKind::ResolverProposed
            | SwarmEventKind::ResolverApplied => Self::Resolving,
            SwarmEventKind::ResolverRolledBack
            | SwarmEventKind::TaskFailed
            | SwarmEventKind::TaskCancelled
            | SwarmEventKind::VerificationFailed => Self::Failed,
            SwarmEventKind::VerificationStarted => Self::Verifying,
            SwarmEventKind::VerificationPassed | SwarmEventKind::TaskCompleted => Self::Completed,
            SwarmEventKind::SwarmStarted
            | SwarmEventKind::TaskStarted
            | SwarmEventKind::TaskQueued
            | SwarmEventKind::SwarmResumed => Self::Running,
            SwarmEventKind::SwarmCompleted => Self::Completed,
            SwarmEventKind::TaskBlocked => Self::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmicAgent {
    pub id: String,
    pub state: String,
    pub progress: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmicTask {
    pub id: String,
    pub agent_id: Option<String>,
    pub state: String,
    pub progress: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmicConflict {
    pub task_id: Option<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmicTimelineEntry {
    pub kind: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CosmicTopology {
    pub agents: BTreeMap<String, CosmicAgent>,
    pub tasks: BTreeMap<String, CosmicTask>,
    pub conflicts: Vec<CosmicConflict>,
    pub timeline: Vec<CosmicTimelineEntry>,
}

impl CosmicTopology {
    pub fn apply(&mut self, event: &SwarmEvent) {
        self.timeline.push(CosmicTimelineEntry {
            kind: format!("{:?}", event.kind),
            agent_id: event.agent_id.clone(),
            task_id: event.task_id.clone(),
            state: event.state.clone(),
        });
        if self.timeline.len() > 64 {
            let excess = self.timeline.len() - 64;
            self.timeline.drain(0..excess);
        }

        match event.kind {
            SwarmEventKind::TaskQueued
            | SwarmEventKind::TaskStarted
            | SwarmEventKind::VerificationStarted
            | SwarmEventKind::VerificationPassed
            | SwarmEventKind::VerificationFailed
            | SwarmEventKind::TaskCompleted
            | SwarmEventKind::TaskFailed
            | SwarmEventKind::TaskCancelled
            | SwarmEventKind::TaskBlocked
            | SwarmEventKind::ResolverStarted
            | SwarmEventKind::ResolverProposed
            | SwarmEventKind::ResolverApplied
            | SwarmEventKind::ResolverRolledBack => {
                let Some(task_id) = event.task_id.clone() else { return };
                let task = self.tasks.entry(task_id.clone()).or_insert_with(|| CosmicTask {
                    id: task_id.clone(),
                    agent_id: event.agent_id.clone(),
                    state: "queued".into(),
                    progress: 0,
                });
                if event.agent_id.is_some() {
                    task.agent_id = event.agent_id.clone();
                }
                if let Some(progress) = event.progress {
                    task.progress = progress.min(100);
                }
                task.state = event
                    .state
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", event.kind).to_ascii_lowercase());
                if let Some(agent_id) = event.agent_id.clone() {
                    let agent = self.agents.entry(agent_id.clone()).or_insert_with(|| CosmicAgent {
                        id: agent_id.clone(),
                        state: "running".into(),
                        progress: 0,
                    });
                    agent.progress = task.progress;
                    agent.state = task.state.clone();
                }
            }
            SwarmEventKind::ConflictDetected => {
                let evidence = event
                    .evidence
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "conflict".into());
                self.conflicts.push(CosmicConflict {
                    task_id: event.task_id.clone(),
                    evidence,
                });
            }
            SwarmEventKind::SwarmStarted
            | SwarmEventKind::SwarmResumed
            | SwarmEventKind::SwarmCompleted => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn maps_runtime_events_to_visual_phases() {
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::SwarmStarted), CosmicPhase::Running);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::ConflictDetected), CosmicPhase::Conflict);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::ResolverStarted), CosmicPhase::Resolving);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::VerificationStarted), CosmicPhase::Verifying);
        assert_eq!(CosmicPhase::from_event(&SwarmEventKind::SwarmCompleted), CosmicPhase::Completed);
    }

    #[test]
    fn topology_tracks_agent_task_conflict_and_timeline() {
        let swarm = Uuid::new_v4();
        let mut topology = CosmicTopology::default();
        topology.apply(
            &SwarmEvent::new(swarm, SwarmEventKind::TaskStarted)
                .task("task-1")
                .agent("agent-1")
                .progress(25)
                .state("running"),
        );
        assert_eq!(topology.tasks["task-1"].agent_id.as_deref(), Some("agent-1"));
        assert_eq!(topology.agents["agent-1"].progress, 25);
        assert_eq!(topology.timeline.len(), 1);

        topology.apply(
            &SwarmEvent::new(swarm, SwarmEventKind::ConflictDetected)
                .task("task-1")
                .evidence("src/lib.rs <- a, b"),
        );
        assert_eq!(topology.conflicts.len(), 1);
        assert_eq!(topology.conflicts[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(topology.timeline.len(), 2);
    }

    #[test]
    fn timeline_is_bounded() {
        let swarm = Uuid::new_v4();
        let mut topology = CosmicTopology::default();
        for _ in 0..100 {
            topology.apply(&SwarmEvent::new(swarm, SwarmEventKind::TaskQueued).task("task"));
        }
        assert_eq!(topology.timeline.len(), 64);
    }
}
