# X11 Code Architecture

X11 Code is organized as a Rust workspace with strict separation between the agent runtime, tools, model providers, context, sessions, permissions, protocol, and integrations.

## Execution flow

1. CLI receives a goal.
2. Agent creates or restores a session.
3. Context manager assembles repository and conversation context.
4. Model provider proposes a plan and tool calls.
5. Permission policy evaluates side effects.
6. Tool registry executes approved operations.
7. Results become protocol events and new context.
8. Agent verifies the result and continues or stops.
9. Session persists state for recovery.

## Safety boundary

Filesystem, shell, network, and Git writes are treated as side effects. The permission crate owns policy decisions so the UI and model cannot silently bypass them.

## UI direction

The future TUI/GUI consumes `x11-protocol` events. Cosmic animation is presentation-only and must never become part of the agent's correctness path.
