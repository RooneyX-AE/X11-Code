# x11-tools

`x11-tools` is the low-level tool surface used by X11 Code agents.

## Safety contract

All filesystem tools operate relative to a canonical workspace root and reject absolute paths, parent-directory traversal, and existing symlinks that resolve outside the workspace. File writes are bounded to 8 MiB and use temporary-file replacement.

Tool output is bounded to protect the model context. Shell commands have bounded length and execution timeout. `edit_file` requires exactly one match, so an ambiguous edit is rejected rather than guessed.

`ToolKind` is metadata used by the agent permission layer. `x11-tools` itself does not grant shell, write, Git, or network permission.

## Built-ins

`read_file`, `write_file`, `edit_file`, `list_files`, `shell`, `search`, `git`, `git_status`, and `git_diff`.

The registry sorts tool definitions deterministically before exposing them to model providers.
