; SurfingStudio 安装包脚本（Inno Setup 6）
; 由 tools/package.ps1 调用，也可手工执行：
;   ISCC.exe /DAppVersion=1.3.3a /DStageDir=..\target\package\stage installer\surfing-studio.iss
;
; SurfingStudio installer script (Inno Setup 6).
; Invoked by tools/package.ps1, or manually:
;   ISCC.exe /DAppVersion=1.3.3a /DStageDir=..\target\package\stage installer\surfing-studio.iss

#ifndef AppVersion
  #define AppVersion "1.3.3a"
#endif
#ifndef StageDir
  #define StageDir "..\target\package\stage"
#endif

[Setup]
; 固定 AppId：升级安装时复用同一卸载项，不会出现两个 SurfingStudio。
; Stable AppId so upgrades reuse the same uninstall entry.
AppId={{6B0E7A2C-9D4F-4C1E-9A6B-3F2D8C5A1E74}
AppName=SurfingStudio
AppVersion={#AppVersion}
AppPublisher=SurfingStudio
DefaultDirName={autopf}\SurfingStudio
DefaultGroupName=SurfingStudio
DisableProgramGroupPage=yes
; 安装到 Program Files 需要管理员权限；运行期数据写入用户目录，安装目录保持只读。
; Installing into Program Files requires elevation; runtime data goes to user directories and the
; install directory stays read-only.
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#StageDir}\..
OutputBaseFilename=SurfingStudio-{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\SurfingStudio.exe
UninstallDisplayName=SurfingStudio {#AppVersion}
CloseApplications=yes
RestartApplications=no
; 安装向导按当前用户的 Windows 界面语言自动选择；仅当没有任何语言匹配时才显示语言选择框。
; 每次安装都重新按系统语言选择，不沿用上一次的手动选择。
; The wizard auto-selects by the current user's Windows UI language; the language dialog appears
; only when no entry matches. Every install re-detects the system language instead of reusing the
; previous manual choice.
LanguageDetectionMethod=uilanguage
ShowLanguageDialog=auto
UsePreviousLanguage=no

[Languages]
; 英文使用 Inno Setup 自带消息；简体中文随仓库提供，兼容未安装中文语言文件的 Inno Setup 6。
; English uses the bundled Inno Setup messages; Simplified Chinese ships with the repository so it
; also works with Inno Setup 6 installations that do not include Chinese language files.
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "zh"; MessagesFile: "{#SourcePath}\languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; 瘦启动器、运行时 DLL 与 AI 资料 DLL 必须位于同一目录；启动器按相对路径加载。
; The thin launcher, runtime DLL and AI-data DLL must share one directory; the launcher loads them
; by relative path.
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\SurfingStudio.exe"; Description: "{cm:LaunchProgram,SurfingStudio}"; Flags: nowait postinstall skipifsilent
