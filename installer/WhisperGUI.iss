[Setup]
AppId={{F9D57E6A-4C47-4D1E-9E68-2B9A3B7F7A41}
AppName=Whisper GUI
AppVersion=0.0.1
AppPublisher=Minecraft-1314
VersionInfoVersion=0.0.1
DefaultDirName={autopf}\WhisperGUI
DefaultGroupName=Whisper GUI
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=WhisperGUI-Setup
SetupIconFile=..\app.ico
UninstallDisplayIcon={app}\app.ico
UninstallDisplayName=Whisper GUI
Uninstallable=yes
CreateUninstallRegKey=yes
WizardStyle=modern dynamic windows11
ShowLanguageDialog=yes
Compression=lzma2
SolidCompression=yes
CloseApplications=yes

[Languages]
Name: "zh"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\dist\WhisperGUI-win-x64\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\target\release\uninstall.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Whisper GUI"; Filename: "{app}\WhisperGUI.exe"; IconFilename: "{app}\app.ico"
Name: "{group}\Uninstall Whisper GUI"; Filename: "{uninstallexe}"; IconFilename: "{app}\app.ico"
Name: "{autodesktop}\Whisper GUI"; Filename: "{app}\WhisperGUI.exe"; IconFilename: "{app}\app.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\WhisperGUI.exe"; Description: "{cm:LaunchProgram,Whisper GUI}"; Flags: nowait postinstall skipifsilent
