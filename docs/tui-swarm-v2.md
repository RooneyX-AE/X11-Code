# TUI Swarm View v2

The TUI swarm view is intentionally separated from agent execution.

Runtime ownership:

```text
AgentManager / Resolver
        ↓
   SwarmEventBus
        ↓
   SwarmView reducer
        ↓
    TUI bridge
        ↓
   Cosmic renderer
```

The UI should never inspect scheduler internals, child runtime state, or filesystem locks directly.

Current protocol data is sufficient to render:

- task queue and task state;
- child-agent state and progress;
- conflict/resolver lifecycle;
- verification state;
- swarm completion/failure counts;
- resumable swarm identity.

The next integration step is exporting the swarm event modules from `x11-agent` and consuming their broadcast receiver directly from the TUI event loop. No polling loop should be introduced for this path.
