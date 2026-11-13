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
; 安装到 Program Files 需要管理员权限；数据目录在 [Dirs] 中授予普通用户修改权限。
; Installing into Program Files requires elevation; [Dirs] grants users modify rights on data.
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

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式 (Create a desktop shortcut)"; GroupDescription: "附加任务 (Additional tasks):"; Flags: unchecked

[Files]
; 瘦启动器、运行时 DLL 与 AI 资料 DLL 必须位于同一目录；启动器按相对路径加载。
; The thin launcher, runtime DLL and AI-data DLL must share one directory; the launcher loads them
; by relative path.
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Dirs]
; 强制数据目录：用户配置、工程、日志、缓存与 AI 技能/智能体都写入这里。
; Enforced data directory: settings, projects, logs, cache and AI agents/skills all live here.
Name: "{app}\data"; Permissions: users-modify
Name: "{app}\data\ai"; Permissions: users-modify
Name: "{app}\data\logs"; Permissions: users-modify
Name: "{app}\data\cache"; Permissions: users-modify
Name: "{app}\data\temp"; Permissions: users-modify
Name: "{app}\data\Projects"; Permissions: users-modify

[Icons]
Name: "{group}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\SurfingStudio"; Filename: "{app}\SurfingStudio.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\SurfingStudio.exe"; Description: "启动 SurfingStudio (Launch SurfingStudio)"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 只清理可再生的缓存与临时文件；settings、ai、Projects 与 logs 保留给用户。
; Only remove regenerable cache/temp files; keep settings, ai, Projects and logs for the user.
Type: filesandordirs; Name: "{app}\data\temp"
Type: filesandordirs; Name: "{app}\data\cache"
