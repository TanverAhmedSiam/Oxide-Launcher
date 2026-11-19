; Oxide Launcher installer (Inno Setup 6.4 or newer).
; Build it with build-installer.ps1, which downloads the Java runtimes into "staging" first.

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#define AppName "Oxide Launcher"
#define AppExe "oxide-launcher.exe"
#define Staging AddBackslash(SourcePath) + "staging"

[Setup]
AppId={{7C1D4E52-9A3B-4F0E-8B6A-52E9D3A1C7F4}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Oxide
DefaultDirName={autopf}\Oxide Launcher
DefaultGroupName=Oxide Launcher
DisableProgramGroupPage=yes
OutputDir=output
OutputBaseFilename=OxideLauncher-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
WizardImageFile=wizard.bmp
WizardSmallImageFile=wizard_small.bmp
; Installs for the current user (no administrator prompt). The user can still choose
; "for all users" in the dialog at the start.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
WelcomeLabel2=This will install [name/ver] on your computer.%n%nJava is included, so you do not need to install it separately. Your Minecraft accounts, worlds and mods are kept in your user profile and are not touched by updates.

[Types]
Name: "full"; Description: "Full installation (recommended): every Java version"
Name: "compact"; Description: "Compact: Java 17 and 21 only"
Name: "custom"; Description: "Custom installation"; Flags: iscustom

[Components]
Name: "main"; Description: "Oxide Launcher"; Types: full compact custom; Flags: fixed
#if DirExists(Staging + "\runtimes\java-25")
Name: "java25"; Description: "Java 25 (newest Minecraft versions)"; Types: full compact
#endif
#if DirExists(Staging + "\runtimes\java-21")
Name: "java21"; Description: "Java 21 (Minecraft 1.20.5 and newer)"; Types: full compact
#endif
#if DirExists(Staging + "\runtimes\java-17")
Name: "java17"; Description: "Java 17 (Minecraft 1.17 to 1.20.4)"; Types: full compact
#endif
#if DirExists(Staging + "\runtimes\java-8")
Name: "java8"; Description: "Java 8 (Minecraft 1.16 and older, old Forge)"; Types: full
#endif

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "staging\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion; Components: main
Source: "THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion; Components: main
#if DirExists(Staging + "\runtimes\java-25")
Source: "staging\runtimes\java-25\*"; DestDir: "{app}\runtimes\java-25"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: java25
#endif
#if DirExists(Staging + "\runtimes\java-21")
Source: "staging\runtimes\java-21\*"; DestDir: "{app}\runtimes\java-21"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: java21
#endif
#if DirExists(Staging + "\runtimes\java-17")
Source: "staging\runtimes\java-17\*"; DestDir: "{app}\runtimes\java-17"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: java17
#endif
#if DirExists(Staging + "\runtimes\java-8")
Source: "staging\runtimes\java-8\*"; DestDir: "{app}\runtimes\java-8"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: java8
#endif

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Start {#AppName}"; Flags: nowait postinstall skipifsilent

[Code]
// Uninstall removes the program but keeps your game data unless you choose to delete it.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if (CurUninstallStep = usPostUninstall) and (not UninstallSilent) then
  begin
    DataDir := ExpandConstant('{userappdata}\OxideLauncher');
    if DirExists(DataDir) then
      if MsgBox('Also delete your game data (accounts, worlds, mods, downloaded versions) in:' + #13#10 + DataDir + #13#10#13#10 + 'This cannot be undone.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;
