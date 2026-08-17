# X11 Code Runtime

## Execution model

X11 Code runs a bounded loop: build context, request the model, execute approved tool calls, feed results back into context, and verify each iteration.

## Workspace safety

Filesystem paths are rejected when absolute or containing parent-directory components. Existing paths are canonicalized before access. Writes validate the target parent against the canonical workspace.

## Tool output limits

Shell, search, Git, file reads, directory listings, and diffs are truncated to protect the model context from unbounded command output.

## Sessions

The CLI persists a session checkpoint to `.x11/session.json` by default. Use `--session <path>` to select another location. Checkpoints are updated after each completed iteration and terminal state.

## Permissions

Side-effecting tools remain protected by the permission policy. `--yes` enables automatic approval for local CLI runs. A future interactive UI will consume `ApprovalRequested` protocol events and return explicit decisions without bypassing the policy boundary.

## Model providers

`x11-model` exposes a provider-neutral interface. The current network transport targets OpenAI-compatible `/chat/completions` APIs and validates malformed tool-call arguments instead of silently treating invalid JSON as valid input.
