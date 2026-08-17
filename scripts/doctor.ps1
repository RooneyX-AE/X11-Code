$ErrorActionPreference = 'Continue'
$Failed = 0
$Warn = 0

function Ok([string]$Name, [string]$Value) { Write-Host ("✓ {0,-16} {1}" -f $Name, $Value) }
function Warn([string]$Name, [string]$Value) { $script:Warn++; Write-Host ("! {0,-16} {1}" -f $Name, $Value) }
function Fail([string]$Name, [string]$Value) { $script:Failed++; Write-Host ("✗ {0,-16} {1}" -f $Name, $Value) }

Write-Host 'X11 Code Doctor'
Write-Host ''

Ok 'os' 'Windows'
Ok 'arch' ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString())

foreach ($dep in @('git','rg','python','node','npm')) {
    $cmd = Get-Command $dep -ErrorAction SilentlyContinue
    if ($cmd) {
        try { $version = & $dep --version 2>$null | Select-Object -First 1 } catch { $version = 'installed' }
        Ok $dep $version
    } elseif ($dep -eq 'git') {
        Fail $dep 'missing'
    } elseif ($dep -eq 'rg' -or $dep -eq 'node' -or $dep -eq 'npm') {
        Warn $dep 'optional/missing'
    }
}

$bash = Get-Command bash -ErrorAction SilentlyContinue
if ($bash) { Ok 'shell' 'Git Bash' } else { Warn 'shell' 'bash not found; Git for Windows is recommended' }

if ($env:X11_API_KEY -and $env:X11_BASE_URL) { Ok 'model-env' 'X11_API_KEY + X11_BASE_URL configured' }
else { Warn 'model-env' 'provider environment is not configured' }

if (Test-Path (Join-Path (Get-Location) '.git')) { Ok 'workspace' 'git repository' } else { Warn 'workspace' 'not a git root' }

if ($Failed -eq 0) { exit 0 } else { exit 1 }
