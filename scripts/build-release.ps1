[CmdletBinding()]
param(
    [switch] $Clean,
    [switch] $Run
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executablePath = Join-Path $repositoryRoot "target\release\codex-peek.exe"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found. Install Rust 1.85 or later and try again."
}

$running = Get-Process -Name "codex-peek" -ErrorAction SilentlyContinue
if ($running) {
    throw "codex-peek is running. Close it before building."
}

Push-Location -LiteralPath $repositoryRoot
try {
    if ($Clean) {
        & cargo clean
        if ($LASTEXITCODE -ne 0) {
            throw "cargo clean failed with exit code $LASTEXITCODE."
        }
    }

    & cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "release executable was not created: $executablePath"
}

Write-Host "Built: $executablePath"

if ($Run) {
    Start-Process -FilePath $executablePath -WorkingDirectory $repositoryRoot
    Write-Host "Started: $executablePath"
}
