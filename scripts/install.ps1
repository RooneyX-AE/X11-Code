$ErrorActionPreference = 'Stop'

$Repo = if ($env:X11_REPO) { $env:X11_REPO } else { 'RooneyX-AE/X11-Code' }
$Version = if ($env:X11_VERSION) { $env:X11_VERSION } else { 'latest' }
$InstallDir = if ($env:X11_INSTALL_DIR) { $env:X11_INSTALL_DIR } else { Join-Path $env:USERPROFILE '.x11\bin' }
$BinaryName = 'x11.exe'

function Info([string]$Message) { Write-Host "[x11] $Message" }
function Fail([string]$Message) { throw "[x11] $Message" }

if ($Version -eq 'latest') {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $ReleaseTag = $release.tag_name
    if (-not $ReleaseTag) { Fail 'no GitHub release found; publish an X11 Code release before binary installation is available' }
} else {
    $ReleaseTag = $Version
}

$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    'X64' { 'x86_64-pc-windows-msvc' }
    'Arm64' { 'aarch64-pc-windows-msvc' }
    default { Fail 'unsupported Windows architecture' }
}

$Asset = "x11-$arch.zip"
$Base = "https://github.com/$Repo/releases/download/$ReleaseTag"
$Temp = Join-Path $env:TEMP ("x11-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Temp | Out-Null

try {
    $Archive = Join-Path $Temp $Asset
    $Checksums = Join-Path $Temp 'SHA256SUMS'
    Info "downloading $Asset ($ReleaseTag)"
    Invoke-WebRequest -Uri "$Base/$Asset" -OutFile $Archive
    Invoke-WebRequest -Uri "$Base/SHA256SUMS" -OutFile $Checksums

    $entry = Select-String -Path $Checksums -Pattern [regex]::Escape($Asset) | Select-Object -First 1
    if (-not $entry) { Fail "checksum entry missing for $Asset" }
    $expected = ($entry.Line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $Archive).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { Fail 'checksum verification failed' }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Expand-Archive -Path $Archive -DestinationPath $Temp -Force
    Copy-Item (Join-Path $Temp $BinaryName) (Join-Path $InstallDir $BinaryName) -Force

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ -ne '' })
    if ($parts -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $InstallDir) -join ';'), 'User')
        Info "added $InstallDir to the user PATH; restart PowerShell to apply"
    }

    Info "installed $(Join-Path $InstallDir $BinaryName)"
} finally {
    Remove-Item -Recurse -Force $Temp -ErrorAction SilentlyContinue
}
