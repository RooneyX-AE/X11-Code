# x11-session

`x11-session` provides durable session checkpoints, integrity validation, session storage, and rollback-point metadata.

Session JSON is versioned and integrity-protected with SHA-256. Checkpoints record event position plus optional Git HEAD/diff identity. Persisted sessions are written through a temporary file before replacement.

Rollback points are separately versioned and integrity-protected. Loading corrupted or unsupported state is rejected instead of silently recovering from untrusted data.

The runtime uses `.x11/session.json` as the default single-session path and can use `SessionStore` for multiple durable sessions.
