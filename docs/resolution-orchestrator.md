# Resolution Orchestrator

The conflict resolution pipeline is deliberately split into deterministic and model-driven stages.

```text
ConflictReport
  -> ConflictHunk
  -> model ResolutionProposal
  -> ConflictResolutionGate
  -> preview
  -> checkpoint
  -> apply
  -> verification
  -> accept / rollback
```

The model never receives permission to write arbitrary files. It proposes a scoped replacement for a specific file and line range. `ConflictResolutionGate` validates the proposal against the conflict report and source-agent group. `ResolutionApplier` performs workspace and canonical-path checks before writing.

`ResolutionOrchestrator::can_retry` provides a hard attempt bound. Rollback is represented explicitly as a terminal state and must be performed by the checkpoint/session layer before retrying.

`AgentRuntime::preview_resolution` exposes the validated preview without applying it. This keeps UI and future model providers on the safe side of the execution boundary.
