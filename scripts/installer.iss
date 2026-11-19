; NoDAW Plugins installer (Inno Setup 6). Built by the CI release job:
;   ISCC /DAppVersion=1.0.0 /DPkgDir=<dist\windows\pkg> /DOutDir=<out> scripts\installer.iss
; Installs every VST3 bundle to Common Files\VST3 and every .clap to Common Files\CLAP.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef PkgDir
  #define PkgDir "..\dist\windows\pkg"
#endif
#ifndef OutDir
  #define OutDir "..\out"
#endif

[Setup]
AppId={{6E0C4B1A-5D2F-4A8E-9C37-4F1D2B8A6C90}
AppName=NoDAW Plugins
AppVersion={#AppVersion}
AppPublisher=NoDAW Labs
AppPublisherURL=https://myaiplug.github.io/nodaw-vst/
DefaultDirName={commonpf64}\NoDAW Labs\Plugins
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
OutputDir={#OutDir}
OutputBaseFilename=NoDAW-Plugins-win64-trial-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Types]
Name: "full"; Description: "All plugins"
Name: "custom"; Description: "Choose plugins"; Flags: iscustom

[Components]
Name: "screw"; Description: "NoDAW Screw"; Types: full
Name: "racks"; Description: "NoDAW Racks"; Types: full
Name: "hs"; Description: "NoDAW Harmonic Space"; Types: full
Name: "cdeq"; Description: "NoDAW Chronos Dynamic EQ"; Types: full
Name: "tpp"; Description: "NoDAW Temporal Pitch Portal"; Types: full
Name: "cw"; Description: "NoDAW Chronos Weaver"; Types: full

[Files]
Source: "{#PkgDir}\NoDAW Screw.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Screw.vst3"; Components: screw; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Screw.clap"; DestDir: "{commoncf64}\CLAP"; Components: screw; Flags: ignoreversion
Source: "{#PkgDir}\NoDAW Racks.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Racks.vst3"; Components: racks; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Racks.clap"; DestDir: "{commoncf64}\CLAP"; Components: racks; Flags: ignoreversion
Source: "{#PkgDir}\NoDAW Harmonic Space.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Harmonic Space.vst3"; Components: hs; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Harmonic Space.clap"; DestDir: "{commoncf64}\CLAP"; Components: hs; Flags: ignoreversion
Source: "{#PkgDir}\NoDAW Chronos Dynamic EQ.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Chronos Dynamic EQ.vst3"; Components: cdeq; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Chronos Dynamic EQ.clap"; DestDir: "{commoncf64}\CLAP"; Components: cdeq; Flags: ignoreversion
Source: "{#PkgDir}\NoDAW Temporal Pitch Portal.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Temporal Pitch Portal.vst3"; Components: tpp; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Temporal Pitch Portal.clap"; DestDir: "{commoncf64}\CLAP"; Components: tpp; Flags: ignoreversion
Source: "{#PkgDir}\NoDAW Chronos Weaver.vst3\*"; DestDir: "{commoncf64}\VST3\NoDAW Chronos Weaver.vst3"; Components: cw; Flags: recursesubdirs ignoreversion
Source: "{#PkgDir}\NoDAW Chronos Weaver.clap"; DestDir: "{commoncf64}\CLAP"; Components: cw; Flags: ignoreversion

[Messages]
WelcomeLabel2=This installs the NoDAW plugins (VST3 and CLAP, 64-bit) into the standard Common Files folders, where every DAW looks.%n%nEach plugin is a full 14-day trial from its first launch. After that, activate it with your Gumroad license key, or it keeps working with a short silence every 45 seconds.%n%nThe plugins' interface needs the Microsoft Edge WebView2 runtime (already on Windows 10/11 in most cases).
