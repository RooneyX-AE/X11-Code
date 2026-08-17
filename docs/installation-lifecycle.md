# X11 Code installation lifecycle

## Installation sources

The official install scripts install the native `x11` binary. The installation directory is controlled by `X11_INSTALL_DIR` on Unix/PowerShell environments.

The updater should only replace binaries installed by a compatible native installation source. Package-manager installations should be upgraded by that package manager rather than by a self-updater.

## Data safety

Uninstall removes the executable only. Project data under `.x11/` is preserved. Sessions, configuration, MCP declarations, plugins, skills, and logs must never be deleted as a side effect of removing the binary.

## Update safety

An update must:

1. Resolve a concrete release and platform asset.
2. Download the matching SHA256SUMS file from the same release.
3. Verify the downloaded archive before extraction.
4. Verify the extracted binary before replacement when the release provides a binary attestation.
5. Stage the new binary without overwriting the running executable prematurely.
6. Replace the current executable using platform-safe self-replacement semantics.
7. Remove temporary files after success or failure.
8. Preserve the existing executable when any validation step fails.

## Package-manager installations

Homebrew, npm, pnpm, Chocolatey, Scoop, and other package managers are installation sources, not project data stores. X11 should report the detected source and tell users to use the matching package manager for upgrades/uninstalls when the binary was not installed by the official native installer.
