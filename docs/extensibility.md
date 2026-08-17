# X11 Code Extensibility

X11 Code uses two extension layers.

## MCP

MCP servers expose external tools. X11 qualifies discovered tools as `mcp__<server>__<tool>` so permission rules can target one server or an entire namespace.

Project configuration can follow the familiar `.x11/mcp.json` pattern:

```json
{
  "mcpServers": {
    "github": {
      "command": "github-mcp",
      "args": [],
      "enabled": true
    }
  }
}
```

Only explicitly enabled servers should be started. Project-local servers execute local commands and therefore require the same trust boundary as hooks and shell tools.

## Plugins

An X11 plugin root contains `x11.plugin.json` and optional `skills/`, `agents/`, `commands/`, `hooks/`, and MCP declarations.

The loader validates that declared relative paths remain inside the plugin root. Third-party plugins should be treated as untrusted until explicitly enabled.

Example:

```text
my-plugin/
├── x11.plugin.json
├── skills/
│   └── review-code/
│       └── SKILL.md
├── agents/
│   └── reviewer.md
├── commands/
│   └── review.md
└── hooks/
    └── check.sh
```

This mirrors the useful extension primitives exposed by modern coding agents: Skills, custom agents, slash commands, hooks and MCP servers. See the Kimi Code documentation for the corresponding public model: plugins can package skills, agents, commands, system instructions, hooks and MCP servers. citeturn712041search0turn712041search3
