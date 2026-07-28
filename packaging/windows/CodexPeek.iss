#ifndef AppVersion
  #error AppVersion must be supplied with /DAppVersion=<version>
#endif
#ifndef SourceExe
  #error SourceExe must be supplied with /DSourceExe=<path>
#endif
#ifndef OutputDir
  #error OutputDir must be supplied with /DOutputDir=<path>
#endif

#define AppName "Codex Usage Monitor"
#define AppExeName "codex-peek.exe"
#define AppRepository "https://github.com/lch5518/CodexPeek"

[Setup]
AppId={{B4A07110-2028-46C9-9268-02C9322E48EA}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=lch5518
AppPublisherURL={#AppRepository}
AppSupportURL={#AppRepository}/issues
AppUpdatesURL={#AppRepository}/releases
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
AppMutex=Local\CodexUsageMonitor.SingleInstance.v1
CloseApplications=yes
Compression=lzma2/max
DefaultDirName={localappdata}\Programs\CodexUsageMonitor
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile=LICENSE
MinVersion=10.0
OutputBaseFilename=CodexPeek-Setup-v{#AppVersion}-x64
OutputDir={#OutputDir}
PrivilegesRequired=lowest
RestartApplications=no
SetupLogging=yes
SolidCompression=yes
SourceDir=..\..
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
VersionInfoVersion={#AppVersion}
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
; 한국어 README는 docs/translations에 위치하며 설치 시 파일명은 README.ko.md로 유지
Source: "docs/translations/README.ko.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "SECURITY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
Type: files; Name: "{app}\codex-usage-monitor.exe"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var
  ExistingCommand: string;
begin
  if
    (CurStep = ssPostInstall) and
    RegQueryStringValue(
      HKCU,
      'Software\Microsoft\Windows\CurrentVersion\Run',
      'CodexUsageMonitor',
      ExistingCommand
    )
  then
    RegWriteStringValue(
      HKCU,
      'Software\Microsoft\Windows\CurrentVersion\Run',
      'CodexUsageMonitor',
      ExpandConstant('"{app}\{#AppExeName}" --startup')
    );
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RegDeleteValue(
      HKCU,
      'Software\Microsoft\Windows\CurrentVersion\Run',
      'CodexUsageMonitor'
    );
end;
