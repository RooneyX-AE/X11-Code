# X11 Code Runtime

## Execution model

X11 Code runs a bounded loop: build context, request the model, execute approved tool calls, feed results back into context, and verify. Verification failures are returned to the model as fresh repair context instead of being treated as success.

## Workspace safety

Filesystem paths are rejected when absolute or containing parent-directory components. Existing paths are canonicalized before access. Writes validate the target parent against the canonical workspace.

## Tool output limits

Shell, search, Git, file reads, directory listings, and diffs are truncated to protect the model context from unbounded command output.

## Sessions

The CLI persists a session checkpoint to `.x11/session.json` by default. Use `--session <path>` to select another location. Checkpoints are updated after each completed iteration and terminal state.

## Permissions

Side-effecting tools remain protected by the permission policy. `--yes` enables automatic approval for local CLI runs. Hooks also use the same shell permission boundary and are disabled unless `--hooks` is explicitly provided.

## Verification

Every run has a verification plan. By default it runs `git diff --check`. Override it with repeated `--verify` options, for example:

```bash
x11 run "fix the failing tests" --verify "cargo test" --verify "git diff --check"
```

Verification commands are bounded by `--verification-timeout-ms`. A failed required verification step sends its output back into the agent context so the next iteration can diagnose and repair the failure.

## Skills and orchestration

The runtime installs built-in subagent roles and operating skills. Skills are injected into the model system context and provide behavioral guidance and preferred tools. The orchestration layer also provides lifecycle hook definitions for future workspace configuration and UI integration.

## Hooks

Hooks are opt-in because they execute arbitrary workspace commands. Enable them with `--hooks`; each hook is still subject to the shell permission policy and the runtime command timeout.

## Model providers

`x11-model` exposes a provider-neutral interface. The current network transport targets OpenAI-compatible `/chat/completions` APIs and validates malformed tool-call arguments instead of silently treating invalid JSON as valid input.
