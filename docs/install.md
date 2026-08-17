# Installing X11 Code

X11 Code is a native Rust CLI. Node.js is not required to run the X11 executable itself. Node and npm are treated as project-toolchain dependencies and are reported by `x11 doctor` when present or missing.

## Recommended binary installation

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/RooneyX-AE/X11-Code/main/scripts/install.sh | bash
```

The installer detects the OS and CPU architecture, downloads the matching GitHub Release asset, downloads `SHA256SUMS`, verifies the archive, installs the `x11` binary into `~/.local/bin`, and reports PATH requirements.

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/RooneyX-AE/X11-Code/main/scripts/install.ps1 | iex
```

The PowerShell installer selects the x64 or ARM64 release, verifies SHA-256, installs `x11.exe` under `%USERPROFILE%\.x11\bin`, and adds that directory to the user PATH.

## Verify the environment

```bash
x11 doctor
```

The doctor checks OS, CPU architecture, Git, ripgrep, Python, Node.js, npm, shell availability, workspace state, and model provider environment variables. Missing Node.js is a warning rather than a fatal error because the X11 binary itself does not depend on Node.

## Run

```bash
x11 run "inspect the repository and explain the architecture"
```

Interactive TUI:

```bash
x11 run "fix the failing tests" --tui
```

Single-shot/model configuration still uses the existing `X11_API_KEY` and `X11_BASE_URL` environment variables.

## Releases

Tagged releases (`vMAJOR.MINOR.PATCH`) are built for:

- Linux x86_64
- Linux arm64
- macOS Intel
- macOS Apple Silicon
- Windows x86_64
- Windows arm64

The release workflow publishes one archive per target plus `SHA256SUMS`. GitHub's hosted runner documentation confirms native arm64 hosted labels such as `ubuntu-24.04-arm` and `windows-11-arm`; using native runners avoids relying on an unconfigured cross-linker. citeturn663420search0turn663420search1

## Node.js and project tooling

X11 itself does not require Node.js. When a project uses JavaScript/TypeScript tooling, `x11 doctor` reports Node.js/npm so the agent can distinguish “X11 cannot start” from “this repository needs Node”. The installer therefore does not silently modify the system Node installation.

## Source installation

For contributors:

```bash
cargo install --path crates/x11-cli
```

This requires the Rust toolchain. Binary installation is preferred for normal users because it avoids requiring Rust during bootstrap.
