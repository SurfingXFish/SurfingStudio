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
; 安装到 Program Files 需要管理员权限；运行期数据按用途写在安装目录的顶层子目录，
; 因此 [Dirs] 为这些目录授予普通用户修改权限，其余程序文件保持只读。
; Installing into Program Files requires elevation; runtime data lives in per-purpose top-level
; sub-directories under the install directory, so [Dirs] grants users modify rights there while the
; program files stay read-only.
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

[Dirs]
; 数据目录按用途拆分在安装目录顶层，不再统一放进 data；授予普通用户修改权限。
; Per-purpose data directories at the install-directory top level instead of a single data folder;
; standard users get modify rights.
Name: "{app}\config"; Permissions: users-modify
Name: "{app}\cache"; Permissions: users-modify
Name: "{app}\logs"; Permissions: users-modify
Name: "{app}\temp"; Permissions: users-modify
Name: "{app}\Projects"; Permissions: users-modify
Name: "{app}\ai"; Permissions: users-modify

[Icons]
Name: "{group}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\SurfingStudio.exe"; Description: "{cm:LaunchProgram,SurfingStudio}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 只清理可再生的缓存与临时文件；config、ai、Projects 与 logs 保留给用户。
; Only remove regenerable cache/temp files; keep config, ai, Projects and logs for the user.
Type: filesandordirs; Name: "{app}\temp"
Type: filesandordirs; Name: "{app}\cache"
