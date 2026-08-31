; Inno Setup script for OptiScaler Manager.
;
; Compiled by the release workflow as:
;   ISCC.exe /DAppVersion=<x.y.z> /DExeSource=<path to exe> ^
;            /DOutputName=<basename> /O<output dir> installer.iss
;
; Installs per-user (no UAC prompt), which also lets the in-app updater run
; the new installer without elevation. CloseApplications lets the installer
; shut the running app down during an update.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExeSource
  #define ExeSource "..\..\target\release\optiscaler-manager.exe"
#endif
#ifndef OutputName
  #define OutputName "optiscaler-manager-setup"
#endif

[Setup]
AppId={{B7A0C9D4-52E7-4B34-9C1D-7E2F0A6B8C31}
AppName=OptiScaler Manager
AppVersion={#AppVersion}
AppPublisher=Diego Luna
AppPublisherURL=https://github.com/diegopluna/optiscaler-manager-gpui
AppSupportURL=https://github.com/diegopluna/optiscaler-manager-gpui/issues
DefaultDirName={autopf}\OptiScaler Manager
DefaultGroupName=OptiScaler Manager
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
; Lets a silent update close the running app via Restart Manager and bring
; it back once the files are swapped.
RestartApplications=yes
SetupIconFile=..\icon\optiscaler-manager.ico
UninstallDisplayIcon={app}\optiscaler-manager.exe
LicenseFile=..\..\LICENSE
WizardStyle=modern

[Files]
Source: "{#ExeSource}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: desktopicon; Description: "Create a &desktop shortcut"; Flags: unchecked

[Icons]
Name: "{group}\OptiScaler Manager"; Filename: "{app}\optiscaler-manager.exe"
Name: "{autodesktop}\OptiScaler Manager"; Filename: "{app}\optiscaler-manager.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\optiscaler-manager.exe"; Description: "Launch OptiScaler Manager"; Flags: nowait postinstall skipifsilent
