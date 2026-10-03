; Inno Setup script for the Neural-Thinker Community edition.
; Built by scripts/package-windows.ps1, which passes:
;   /DAppVersion=<x.y.z[-pre]>  /DAppNumericVersion=<x.y.z>  /DStageDir=<folder with the exe, LICENSE.txt, NOTICE.txt>
;   /DOutputDir=<folder for the installer>

#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef AppNumericVersion
  #define AppNumericVersion AppVersion
#endif
#ifndef StageDir
  #error StageDir is required
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Never change AppId: Windows uses it to find and upgrade earlier installs.
AppId={{333000EE-4D4E-4D62-971C-FAEA40A4E11E}
AppName=Neural-Thinker
AppVersion={#AppVersion}
AppVerName=Neural-Thinker {#AppVersion}
AppPublisher=0rtizSys
AppPublisherURL=https://github.com/0rtizSys/Neural-Thinker
AppSupportURL=https://github.com/0rtizSys/Neural-Thinker/issues
AppCopyright=Copyright (c) 2026 0rtizSys
VersionInfoVersion={#AppNumericVersion}
VersionInfoDescription=Neural-Thinker Community edition installer
; Per-user install: no administrator rights needed.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Neural-Thinker
DefaultGroupName=Neural-Thinker
DisableProgramGroupPage=yes
; The license must be accepted before installing.
LicenseFile={#StageDir}\LICENSE.txt
InfoBeforeFile={#StageDir}\NOTICE.txt
OutputDir={#OutputDir}
OutputBaseFilename=neural-thinker-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\neural-thinker.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\neural-thinker.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\NOTICE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Neural-Thinker"; Filename: "{app}\neural-thinker.exe"
Name: "{autodesktop}\Neural-Thinker"; Filename: "{app}\neural-thinker.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\neural-thinker.exe"; Description: "{cm:LaunchProgram,Neural-Thinker}"; Flags: nowait postinstall skipifsilent
