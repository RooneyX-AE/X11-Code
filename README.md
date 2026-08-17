# X11 Code

X11 Code is an open-source autonomous coding agent built as a Rust workspace. It combines a guarded tool runtime, model-provider abstraction, durable sessions, context management, permissions, MCP integration, and a stream-oriented protocol for future TUI/GUI clients.

## Quick start

```bash
cargo run -p x11-cli -- run "inspect this repository and explain the next fixes"
```

For an OpenAI-compatible API:

```bash
export X11_API_KEY=...
export X11_BASE_URL=https://api.example.com/v1
cargo run -p x11-cli -- run "fix the failing tests" --yes
```

The `--yes` switch auto-approves side-effecting tools. Without it, side-effecting operations are denied until an interactive approval layer is connected.

## Design goals

- deterministic safety boundary around filesystem, shell, Git and network operations
- model-provider independence
- bounded agent iterations and context compaction
- durable, inspectable sessions
- native MCP stdio client
- small, testable Rust crates
- presentation separated from correctness so the future cosmic UI cannot interfere with execution

## License

MIT
