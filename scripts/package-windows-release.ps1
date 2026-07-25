[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")]
    [string] $Version,

    [Parameter(Mandatory = $true)]
    [string] $Executable,

    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,

    [Parameter(Mandatory = $true)]
    [string] $IsccPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$executablePath = [System.IO.Path]::GetFullPath($Executable)
$outputPath = [System.IO.Path]::GetFullPath($OutputDirectory)
$compilerPath = [System.IO.Path]::GetFullPath($IsccPath)
$installerScript = Join-Path $repositoryRoot "packaging/windows/CodexPeek.iss"

foreach ($requiredFile in @(
    $executablePath
    $compilerPath
    $installerScript
    (Join-Path $repositoryRoot "README.md")
    (Join-Path $repositoryRoot "README.ko.md")
    (Join-Path $repositoryRoot "LICENSE")
    (Join-Path $repositoryRoot "SECURITY.md")
    (Join-Path $repositoryRoot "THIRD_PARTY_NOTICES.md")
)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "required release input is missing: $requiredFile"
    }
}

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
$portableName = "codex-peek-v$Version-windows-x86_64-portable.zip"
$installerName = "CodexPeek-Setup-v$Version-x64.exe"
$checksumName = "SHA256SUMS.txt"
$portableArchive = Join-Path $outputPath $portableName
$installer = Join-Path $outputPath $installerName
$checksumManifest = Join-Path $outputPath $checksumName
foreach ($artifact in @($portableArchive, $installer, $checksumManifest)) {
    if (Test-Path -LiteralPath $artifact) {
        throw "release artifact already exists: $artifact"
    }
}

$stagingRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "codex-release-{0}" -f [guid]::NewGuid().ToString("N")
)
$completed = $false
try {
    New-Item -ItemType Directory -Path $stagingRoot | Out-Null
    Copy-Item -LiteralPath $executablePath `
        -Destination (Join-Path $stagingRoot "codex-peek.exe")
    foreach ($document in @(
        "README.md"
        "README.ko.md"
        "LICENSE"
        "SECURITY.md"
        "THIRD_PARTY_NOTICES.md"
    )) {
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $document) `
            -Destination (Join-Path $stagingRoot $document)
    }
    Compress-Archive -Path (Join-Path $stagingRoot "*") `
        -DestinationPath $portableArchive

    $compilerArguments = @(
        "/DAppVersion=$Version"
        "/DSourceExe=$executablePath"
        "/DOutputDir=$outputPath"
        $installerScript
    )
    & $compilerPath @compilerArguments
    $compilerSucceeded = $?
    if (-not $compilerSucceeded -or (
        [System.IO.Path]::GetExtension($compilerPath) -ne ".ps1" -and $LASTEXITCODE -ne 0
    )) {
        throw "Inno Setup compiler failed"
    }
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
        throw "Inno Setup did not create the expected installer: $installer"
    }

    $checksumLines = foreach ($name in @($portableName, $installerName) | Sort-Object) {
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath (
            Join-Path $outputPath $name
        )).Hash.ToLowerInvariant()
        "$hash  $name"
    }
    [System.IO.File]::WriteAllLines(
        $checksumManifest,
        $checksumLines,
        [System.Text.UTF8Encoding]::new($false)
    )
    $completed = $true
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
    if (-not $completed) {
        foreach ($artifact in @($portableArchive, $installer, $checksumManifest)) {
            Remove-Item -LiteralPath $artifact -Force -ErrorAction SilentlyContinue
        }
    }
}

[pscustomobject]@{
    PortableArchive = $portableArchive
    Installer = $installer
    Checksums = $checksumManifest
}
