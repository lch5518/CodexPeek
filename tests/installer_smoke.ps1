[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")]
    [string] $Version,

    [Parameter(Mandatory = $true)]
    [string] $Installer,

    [switch] $AllowUserProfileMutation
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $AllowUserProfileMutation) {
    throw "installer smoke test mutates HKCU and the current user's Start Menu; pass -AllowUserProfileMutation on an isolated runner"
}

$installerPath = [System.IO.Path]::GetFullPath($Installer)
if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "installer is missing: $installerPath"
}

$testId = [guid]::NewGuid().ToString("N")
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "codex-installer-smoke-$testId"
$installDirectory = Join-Path $testRoot "install"
$installLog = Join-Path $testRoot "install.log"
$uninstallLog = Join-Path $testRoot "uninstall.log"
$installedExecutable = Join-Path $installDirectory "codex-peek.exe"
$legacyExecutable = Join-Path $installDirectory "codex-usage-monitor.exe"
$uninstaller = Join-Path $installDirectory "unins000.exe"
$startMenuDirectory = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Codex Usage Monitor"
$startMenuShortcut = Join-Path $startMenuDirectory "Codex Usage Monitor.lnk"
$uninstallRoot = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$runValueName = "CodexUsageMonitor"
$settingsDirectory = Join-Path $env:APPDATA "CodexPeek"
$settingsSentinel = Join-Path $settingsDirectory "packaging-smoke-sentinel-$testId.txt"
$startMenuDirectoryExisted = Test-Path -LiteralPath $startMenuDirectory
$settingsDirectoryExisted = Test-Path -LiteralPath $settingsDirectory

function Find-CodexUninstallEntry {
    if (-not (Test-Path -LiteralPath $uninstallRoot)) {
        return $null
    }
    Get-ChildItem -LiteralPath $uninstallRoot |
        ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath } |
        Where-Object DisplayName -EQ "Codex Usage Monitor" |
        Select-Object -First 1
}

$previousRunValue = $null
$hadPreviousRunValue = $false
if (Test-Path -LiteralPath $runKey) {
    $runProperties = Get-ItemProperty -LiteralPath $runKey
    if ($null -ne $runProperties.PSObject.Properties[$runValueName]) {
        $previousRunValue = $runProperties.$runValueName
        $hadPreviousRunValue = $true
    }
}

try {
    if ($null -ne (
        Get-Process -Name "codex-peek", "codex-usage-monitor" -ErrorAction SilentlyContinue
    )) {
        throw "Codex Usage Monitor is running; stop it before the installer smoke test"
    }
    if ($null -ne (Find-CodexUninstallEntry)) {
        throw "Codex Usage Monitor is already installed for this user"
    }
    if ($startMenuDirectoryExisted) {
        throw "Codex Usage Monitor Start Menu group already exists"
    }
    New-Item -ItemType Directory -Path $testRoot | Out-Null
    New-Item -ItemType Directory -Path $installDirectory | Out-Null
    Set-Content -LiteralPath $legacyExecutable -Value "legacy executable" -NoNewline
    New-Item -Path $runKey -Force | Out-Null
    New-ItemProperty -LiteralPath $runKey -Name $runValueName `
        -Value "`"$legacyExecutable`" --startup" -PropertyType String -Force | Out-Null

    $install = Start-Process -FilePath $installerPath -Wait -PassThru -ArgumentList @(
        "/VERYSILENT"
        "/SUPPRESSMSGBOXES"
        "/NORESTART"
        "/DIR=`"$installDirectory`""
        "/LOG=`"$installLog`""
    )
    if ($install.ExitCode -ne 0) {
        throw "installer exited with code $($install.ExitCode)"
    }
    if (-not (Test-Path -LiteralPath $installedExecutable -PathType Leaf)) {
        throw "installed executable is missing"
    }
    if (Test-Path -LiteralPath $legacyExecutable) {
        throw "legacy executable remained after upgrade"
    }
    $productVersion = (Get-Item -LiteralPath $installedExecutable).VersionInfo.ProductVersion
    if ($productVersion -ne $Version) {
        throw "installed ProductVersion $productVersion does not match $Version"
    }
    if (-not (Test-Path -LiteralPath $startMenuShortcut -PathType Leaf)) {
        throw "Start Menu shortcut is missing"
    }
    $uninstallEntry = Find-CodexUninstallEntry
    if ($null -eq $uninstallEntry -or $uninstallEntry.DisplayVersion -ne $Version) {
        throw "per-user uninstall registration is missing or has the wrong version"
    }
    $runProperties = Get-ItemProperty -LiteralPath $runKey
    $expectedRunValue = "`"$installedExecutable`" --startup"
    if ($runProperties.$runValueName -ne $expectedRunValue) {
        throw "autostart registry value was not migrated to the new executable"
    }

    New-Item -ItemType Directory -Path $settingsDirectory -Force | Out-Null
    Set-Content -LiteralPath $settingsSentinel -Value "preserve on uninstall" -NoNewline

    $uninstall = Start-Process -FilePath $uninstaller -Wait -PassThru -ArgumentList @(
        "/VERYSILENT"
        "/SUPPRESSMSGBOXES"
        "/NORESTART"
        "/LOG=`"$uninstallLog`""
    )
    if ($uninstall.ExitCode -ne 0) {
        throw "uninstaller exited with code $($uninstall.ExitCode)"
    }
    if (Test-Path -LiteralPath $installedExecutable) {
        throw "installed executable remained after uninstall"
    }
    if (Test-Path -LiteralPath $startMenuShortcut) {
        throw "Start Menu shortcut remained after uninstall"
    }
    if ($null -ne (Find-CodexUninstallEntry)) {
        throw "uninstall registration remained after uninstall"
    }
    if (Test-Path -LiteralPath $runKey) {
        $runProperties = Get-ItemProperty -LiteralPath $runKey
        if ($null -ne $runProperties.PSObject.Properties[$runValueName]) {
            throw "autostart registry value remained after uninstall"
        }
    }
    if (-not (Test-Path -LiteralPath $settingsSentinel -PathType Leaf)) {
        throw "user settings were removed during uninstall"
    }
}
finally {
    if (Test-Path -LiteralPath $uninstaller) {
        $cleanup = Start-Process -FilePath $uninstaller -Wait -PassThru -ArgumentList @(
            "/VERYSILENT"
            "/SUPPRESSMSGBOXES"
            "/NORESTART"
        )
        if ($cleanup.ExitCode -ne 0) {
            Write-Warning "cleanup uninstaller exited with code $($cleanup.ExitCode)"
        }
    }
    if ($hadPreviousRunValue) {
        New-Item -Path $runKey -Force | Out-Null
        New-ItemProperty -LiteralPath $runKey -Name $runValueName `
            -Value $previousRunValue -PropertyType String -Force | Out-Null
    }
    elseif (Test-Path -LiteralPath $runKey) {
        Remove-ItemProperty -LiteralPath $runKey -Name $runValueName -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $settingsSentinel -Force -ErrorAction SilentlyContinue
    if (-not $startMenuDirectoryExisted -and (Test-Path -LiteralPath $startMenuDirectory)) {
        Remove-Item -LiteralPath $startMenuDirectory -Recurse -Force
    }
    if (-not $settingsDirectoryExisted -and (Test-Path -LiteralPath $settingsDirectory)) {
        $remainingSettingsFiles = @(Get-ChildItem -LiteralPath $settingsDirectory -Force)
        if ($remainingSettingsFiles.Count -eq 0) {
            Remove-Item -LiteralPath $settingsDirectory -Force
        }
    }
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}
