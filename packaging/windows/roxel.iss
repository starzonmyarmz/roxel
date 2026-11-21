; Build: ISCC /DAppVersion=1.2.3 /O<outdir> packaging\windows\roxel.iss
; Expects target\release\Roxel.exe to exist.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; Never change AppId: Windows uses it to find the installed copy on upgrade.
AppId={{2B0F9F5D-BE8D-464B-971B-10447CAE7994}
AppName=Roxel
AppVersion={#AppVersion}
AppPublisher=Daniel Marino
AppPublisherURL=https://github.com/starzonmyarmz/roxel
SourceDir=..\..
DefaultDirName={autopf}\Roxel
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputBaseFilename=Roxel-setup
SetupIconFile=assets\icons\roxel.ico
UninstallDisplayIcon={app}\Roxel.exe
ChangesAssociations=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: desktopicon; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "target\release\Roxel.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Roxel"; Filename: "{app}\Roxel.exe"
Name: "{autodesktop}\Roxel"; Filename: "{app}\Roxel.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.rox"; ValueType: string; ValueName: ""; ValueData: "Roxel.Project"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.rox\OpenWithProgids"; ValueType: string; ValueName: "Roxel.Project"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Roxel.Project"; ValueType: string; ValueName: ""; ValueData: "Roxel project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Roxel.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\Roxel.exe,0"
Root: HKA; Subkey: "Software\Classes\Roxel.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\Roxel.exe"" ""%1"""

[Run]
Filename: "{app}\Roxel.exe"; Description: "{cm:LaunchProgram,Roxel}"; Flags: nowait postinstall skipifsilent
