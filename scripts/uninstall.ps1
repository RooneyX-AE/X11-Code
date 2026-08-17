$ErrorActionPreference = 'Stop'

$InstallDir = if ($env:X11_INSTALL_DIR) { $env:X11_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }
$Binary = Join-Path $InstallDir 'x11.exe'

if (Test-Path $Binary) {
    Remove-Item -Force $Binary
    Write-Host "[x11] removed $Binary"
} else {
    Write-Host "[x11] binary not found at $Binary"
}

Write-Host '[x11] project/user data was preserved'
Write-Host '[x11] remove .x11\ in a project or your X11 data directory manually if desired'
