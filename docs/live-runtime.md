# Live Runtime and Interactive Approval

## Event flow

`AgentRuntime` keeps the durable `Session` history and also publishes each `AgentEvent` through a bounded Tokio broadcast bus. Consumers can subscribe without changing agent execution semantics.

```text
AgentRuntime
   ├── Session history
   └── EventBus ──→ TUI / future GUI / telemetry
```

The event bus is deliberately bounded. Slow consumers receive a lag notification instead of blocking the agent indefinitely.

## Interactive approval

A TUI run enables the `ApprovalBroker`. When a tool reaches a policy decision of `Ask`, the agent emits `ApprovalRequested` and waits on the broker. The TUI resolves that request with `y` or `n` and the agent emits `ApprovalResolved` before executing or denying the tool.

```text
model
  ↓
tool request
  ↓
permission = Ask
  ↓
ApprovalRequested
  ↓
TUI y/n
  ↓
ApprovalResolved
  ↓
execute / deny
```

Non-TUI runs do not block waiting for an unavailable UI. In that mode an `Ask` decision is denied unless `--yes` or an equivalent automatic approval policy is active.

## CLI

```bash
x11 run "fix the failing tests" --tui
x11 run "inspect the repository" --mode plan --tui
x11 run "review the current diff" --mode review --tui
```

`q` or `Ctrl+C` leaves the TUI and cancels the running agent task. `y` and `n` resolve the active approval request.

## UI contract

The TUI consumes only `AgentEvent` and does not inspect internal agent state. This keeps the terminal frontend replaceable with a future desktop/cosmic frontend without coupling rendering logic to the agent loop.
