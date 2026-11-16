#Requires -Version 5.1
<#
所属模块：tools / 打包
文件用途：
- 构建运行时 DLL、AI 资料 DLL 与瘦启动器，组装安装暂存目录并调用 Inno Setup 生成 setup.exe。
- 版本号默认从根 Cargo.toml 读取（1.3.3-a → 1.3.3a），也可用 -Version 覆盖。
- 自动在常见安装位置查找 ISCC.exe；找不到时给出安装提示并退出。

Module: tools / packaging
Purpose:
- Build the runtime DLL, AI-data DLL and thin launcher, stage them and run Inno Setup to produce
  setup.exe.
- Read the version from the root Cargo.toml (1.3.3-a -> 1.3.3a) unless -Version is supplied.
- Locate ISCC.exe in the usual install roots and fail with instructions when missing.
#>
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Get-VersionLabel {
    param([string]$Manifest)
    $line = Select-String -Path $Manifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $line) {
        throw "无法从 $Manifest 读取版本号"
    }
    $value = $line.Matches[0].Groups[1].Value
    if ($value.EndsWith("-a")) {
        return $value.Substring(0, $value.Length - 2) + "a"
    }
    if ($value.EndsWith("-t")) {
        return $value.Substring(0, $value.Length - 2) + "t"
    }
    return $value
}

function Find-Iscc {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 7\ISCC.exe",
        "C:\Program Files\Inno Setup 7\ISCC.exe"
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path $candidate)) {
            return $candidate
        }
    }
    throw "未找到 Inno Setup 的 ISCC.exe。请先安装 Inno Setup 6：winget install JRSoftware.InnoSetup"
}

if (-not $Version) {
    $Version = Get-VersionLabel (Join-Path $root "Cargo.toml")
}

Write-Host "==> 构建 SurfingStudio $Version" -ForegroundColor Cyan
cargo build --release -p surfing-studio -p surfing-ai-data -p surfing-studio-launcher
if ($LASTEXITCODE -ne 0) {
    throw "cargo build 失败"
}

$release = Join-Path $root "target\release"
$stage = Join-Path $root "target\package\stage"
if (Test-Path $stage) {
    Remove-Item $stage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stage | Out-Null

$files = @(
    @{ Source = Join-Path $release "surfing-studio.exe"; Name = "SurfingStudio.exe" },
    @{ Source = Join-Path $release "surfing_studio.dll"; Name = "surfing_studio.dll" },
    @{ Source = Join-Path $release "surfing_ai_data.dll"; Name = "surfing_ai_data.dll" }
)
foreach ($file in $files) {
    if (-not (Test-Path $file.Source)) {
        throw "缺少构建产物：$($file.Source)"
    }
    Copy-Item $file.Source (Join-Path $stage $file.Name)
}
Copy-Item (Join-Path $root "LICENSE") (Join-Path $stage "LICENSE")
if (Test-Path (Join-Path $root "README.md")) {
    Copy-Item (Join-Path $root "README.md") (Join-Path $stage "README.md")
}

$iscc = Find-Iscc
$iss = Join-Path $root "installer\surfing-studio.iss"
Write-Host "==> 生成安装包（ISCC: $iscc）" -ForegroundColor Cyan
& $iscc "/DAppVersion=$Version" "/DStageDir=$stage" $iss
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup 编译失败"
}

$setup = Join-Path $root "target\package\SurfingStudio-$Version-setup.exe"
if (-not (Test-Path $setup)) {
    throw "未生成安装包：$setup"
}
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item $setup $dist -Force
Write-Host "==> 完成：$setup" -ForegroundColor Green
Write-Host "==> 已复制到：$(Join-Path $dist (Split-Path $setup -Leaf))" -ForegroundColor Green
