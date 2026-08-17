# x11-mcp

`x11-mcp` provides the local stdio MCP client and server registry.

The client supports the current MCP `2026-07-28` modern era through `server/discover` and per-request metadata, with fallback to the legacy `initialize` handshake used by older MCP revisions. The modern revision is stateless and removed the initialize/session handshake; X11 keeps fallback compatibility for older servers. citeturn697910search0turn697910search11

Server and tool names are namespace-safe, request IDs are correlated, malformed responses are rejected, and request execution is bounded by a timeout.
