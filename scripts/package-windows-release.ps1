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
    # 한국어 README는 docs/translations에 있지만 아카이브에는 README.ko.md로 포함됨
    (Join-Path $repositoryRoot "docs/translations/README.ko.md")
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
$rawExecutableName = "codex-peek-v$Version-windows-x86_64.exe"
$installerName = "CodexPeek-Setup-v$Version-x64.exe"
$checksumName = "SHA256SUMS.txt"
$portableArchive = Join-Path $outputPath $portableName
$rawExecutable = Join-Path $outputPath $rawExecutableName
$installer = Join-Path $outputPath $installerName
$checksumManifest = Join-Path $outputPath $checksumName
foreach ($artifact in @($portableArchive, $rawExecutable, $installer, $checksumManifest)) {
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
    # Source: 리포지토리 루트 기준 원본 경로, Name: 아카이브에 포함될 파일명(릴리스 계약)
    $releaseDocuments = @(
        @{ Source = "README.md";                      Name = "README.md" }
        @{ Source = "docs/translations/README.ko.md"; Name = "README.ko.md" }
        @{ Source = "LICENSE";                        Name = "LICENSE" }
        @{ Source = "SECURITY.md";                    Name = "SECURITY.md" }
        @{ Source = "THIRD_PARTY_NOTICES.md";         Name = "THIRD_PARTY_NOTICES.md" }
    )
    foreach ($document in $releaseDocuments) {
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $document.Source) `
            -Destination (Join-Path $stagingRoot $document.Name)
    }
    Compress-Archive -Path (Join-Path $stagingRoot "*") `
        -DestinationPath $portableArchive
    Copy-Item -LiteralPath $executablePath -Destination $rawExecutable

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

    $checksumLines = foreach ($name in @(
        $portableName
        $rawExecutableName
        $installerName
    ) | Sort-Object) {
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
        foreach ($artifact in @(
            $portableArchive
            $rawExecutable
            $installer
            $checksumManifest
        )) {
            Remove-Item -LiteralPath $artifact -Force -ErrorAction SilentlyContinue
        }
    }
}

[pscustomobject]@{
    PortableArchive = $portableArchive
    RawExecutable = $rawExecutable
    Installer = $installer
    Checksums = $checksumManifest
}
