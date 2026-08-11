$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "codex-release-packaging-{0}" -f [guid]::NewGuid().ToString("N")
)
$fixtureExe = Join-Path $testRoot "codex-peek.exe"
$outputDirectory = Join-Path $testRoot "output"
$fakeIscc = Join-Path $testRoot "fake-iscc.ps1"
$expandedArchive = Join-Path $testRoot "expanded"
$installerDefinition = Join-Path $repositoryRoot "packaging/windows/CodexPeek.iss"

try {
    New-Item -ItemType Directory -Path $testRoot | Out-Null
    Set-Content -LiteralPath $fixtureExe -Value "fixture executable" -NoNewline
    @'
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CompilerArguments
)

$version = $null
$outputDirectory = $null
foreach ($argument in $CompilerArguments) {
    if ($argument.StartsWith("/DAppVersion=")) {
        $version = $argument.Substring("/DAppVersion=".Length)
    }
    if ($argument.StartsWith("/DOutputDir=")) {
        $outputDirectory = $argument.Substring("/DOutputDir=".Length)
    }
}
if ([string]::IsNullOrWhiteSpace($version) -or [string]::IsNullOrWhiteSpace($outputDirectory)) {
    throw "required Inno Setup definitions were not supplied"
}
if ($version -eq "9.9.9") {
    throw "fixture compiler failure"
}
$installer = Join-Path $outputDirectory "CodexPeek-Setup-v$version-x64.exe"
Set-Content -LiteralPath $installer -Value "fixture installer" -NoNewline
'@ | Set-Content -LiteralPath $fakeIscc

    & (Join-Path $repositoryRoot "scripts/package-windows-release.ps1") `
        -Version "1.2.3" `
        -Executable $fixtureExe `
        -OutputDirectory $outputDirectory `
        -IsccPath $fakeIscc

    $portableName = "codex-peek-v1.2.3-windows-x86_64-portable.zip"
    $rawExecutableName = "codex-peek-v1.2.3-windows-x86_64.exe"
    $installerName = "CodexPeek-Setup-v1.2.3-x64.exe"
    $checksumName = "SHA256SUMS.txt"
    foreach ($name in @($portableName, $rawExecutableName, $installerName, $checksumName)) {
        $path = Join-Path $outputDirectory $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "missing release artifact: $name"
        }
    }

    Expand-Archive -LiteralPath (Join-Path $outputDirectory $portableName) `
        -DestinationPath $expandedArchive
    $archiveFiles = Get-ChildItem -LiteralPath $expandedArchive -File |
        ForEach-Object Name |
        Sort-Object
    $expectedArchiveFiles = @(
        "codex-peek.exe"
        "LICENSE"
        "README.ko.md"
        "README.md"
        "SECURITY.md"
        "THIRD_PARTY_NOTICES.md"
    ) | Sort-Object
    if (Compare-Object $expectedArchiveFiles $archiveFiles) {
        throw "portable archive contents did not match the release contract"
    }
    if ((Get-Content -LiteralPath (
        Join-Path $outputDirectory $rawExecutableName
    ) -Raw) -ne "fixture executable") {
        throw "raw executable did not preserve the built release executable"
    }

    $checksums = @{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $outputDirectory $checksumName)) {
        if ($line -notmatch "^([0-9a-f]{64})  (.+)$") {
            throw "invalid checksum line: $line"
        }
        $checksums[$Matches[2]] = $Matches[1]
    }
    foreach ($name in @($portableName, $rawExecutableName, $installerName)) {
        $expectedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (
            Join-Path $outputDirectory $name
        )).Hash.ToLowerInvariant()
        if ($checksums[$name] -ne $expectedHash) {
            throw "checksum mismatch for $name"
        }
    }
    if ($checksums.Count -ne 3) {
        throw "checksum manifest must contain exactly three release assets"
    }

    $installerText = Get-Content -LiteralPath $installerDefinition -Raw
    foreach ($requiredDirective in @(
        "AppMutex=Local\CodexUsageMonitor.SingleInstance.v1"
        "DefaultDirName={localappdata}\Programs\CodexUsageMonitor"
        "PrivilegesRequired=lowest"
        "RegDeleteValue("
        "CurStepChanged"
        "RegQueryStringValue"
        "RegWriteStringValue"
        "codex-usage-monitor.exe"
        "codex-peek.exe"
        "'CodexUsageMonitor'"
    )) {
        if (-not $installerText.Contains($requiredDirective)) {
            throw "installer contract is missing: $requiredDirective"
        }
    }
    foreach ($forbiddenDirective in @("{autodesktop}", "{commondesktop}", "AppMutexes=")) {
        if ($installerText.Contains($forbiddenDirective)) {
            throw "installer contract contains forbidden behavior: $forbiddenDirective"
        }
    }

    $failedOutput = Join-Path $testRoot "failed-output"
    $failedAsExpected = $false
    try {
        & (Join-Path $repositoryRoot "scripts/package-windows-release.ps1") `
            -Version "9.9.9" `
            -Executable $fixtureExe `
            -OutputDirectory $failedOutput `
            -IsccPath $fakeIscc
    }
    catch {
        $failedAsExpected = $true
    }
    if (-not $failedAsExpected) {
        throw "fixture compiler failure unexpectedly succeeded"
    }
    if (@(Get-ChildItem -LiteralPath $failedOutput -File).Count -ne 0) {
        throw "failed packaging left partial release assets behind"
    }
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}
