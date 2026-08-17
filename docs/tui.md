# X11 Code TUI

The TUI is an event-driven renderer over `x11-protocol::AgentEvent`. It intentionally keeps rendering separate from the agent runtime so the same event stream can later drive a desktop UI.

## Run

```bash
x11 run "fix the tests" --tui
```

The current CLI entrypoint runs the agent and then renders the captured session event stream. This is deterministic and useful for replay/debugging. A future live mode will connect the runtime to a bounded async channel and render events as they arrive.

## Commands

The TUI command line supports `/plan`, `/review`, `/compact`, `/resume`, `/help`, and `/quit`. Approval prompts use `y` and `n`.

## Cosmic field

The cosmic background is a deterministic text renderer. It updates on protocol events and is intentionally CPU-light. The renderer is isolated from agent logic so a future GPU/desktop renderer can replace it without changing the agent core.
