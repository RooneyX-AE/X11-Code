# Swarm Event Protocol 2.0

X11 Code swarm state is represented by `SwarmEvent` rather than requiring the TUI to inspect scheduler internals.

## Event identity

Every event contains:

- `event_id`: unique event identifier.
- `swarm_id`: swarm identity.
- `task_id`: optional task identity.
- `agent_id`: optional child-agent identity.
- `parent_task_id`: optional parent task identity.
- `timestamp_ms`: wall-clock timestamp.
- `kind`: lifecycle event.
- `progress`: optional 0..100 progress value.
- `state`: presentation-neutral state string.
- `evidence`: bounded textual evidence for the UI/reviewer.

## Lifecycle

```text
SwarmStarted
  -> TaskQueued
  -> TaskStarted
  -> TaskCompleted | TaskFailed | TaskCancelled
  -> ConflictDetected
  -> ResolverStarted
  -> ResolverProposed
  -> ResolverApplied | ResolverRolledBack
  -> VerificationStarted
  -> VerificationPassed | VerificationFailed
  -> SwarmCompleted
```

`SwarmEventBus` is bounded and uses Tokio broadcast. A slow subscriber must not block the swarm runtime.

`SwarmView` folds events into renderable task/agent state. The TUI should consume the reducer state and never inspect `AgentManager` internals.

## Resolver rule

Resolver events are evidence events only. Applying a resolution remains controlled by `ResolutionOrchestrator` and `ResolutionApplier`.
