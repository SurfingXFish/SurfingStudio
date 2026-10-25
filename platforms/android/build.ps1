# Module: platforms/android / build script
# Purpose:
# - Cross-compile `surfing-studio-android` (arm64-v8a cdylib) with the NDK and package an installable
#   APK with aapt2 / zipalign / apksigner, without Gradle.
# - Optional `-Install` installs on the connected device; `-Run` also launches it.
# Kept ASCII-only so Windows PowerShell 5.1 parses it regardless of the console code page.
param(
    [switch]$Install,
    [switch]$Run,
    # Device ABI: arm64-v8a (tablets/phones) or x86_64 (emulator).
    [string]$Abi = 'arm64-v8a'
)

$ErrorActionPreference = 'Stop'

# Toolchain and SDK paths: environment variables first, then the verified local defaults.
$ndk = $env:ANDROID_NDK_HOME
if (-not $ndk) { $ndk = 'D:\android-ndk25\android-ndk-r27c' }
$sdk = $env:ANDROID_HOME
if (-not $sdk) { $sdk = 'D:\Android\SDK' }
# build-tools 35.0.0 is the version verified on this machine; fall back to the newest one present.
$buildTools = Join-Path $sdk 'build-tools\35.0.0'
if (-not (Test-Path $buildTools)) {
    $buildTools = Get-ChildItem (Join-Path $sdk 'build-tools') -Directory |
        Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$androidJar = Join-Path $sdk 'platforms\android-35\android.jar'
if (-not (Test-Path $androidJar)) { throw "android.jar not found: $androidJar" }

# A real JDK (javac + d8) is required for the tiny activity subclass that forwards
# onActivityResult for Storage Access Framework. Prefer a Zulu install when present.
$jdk = $env:JAVA_HOME
if (-not $jdk -or -not (Test-Path (Join-Path $jdk 'bin\javac.exe'))) {
    $jdk = 'C:\Program Files\Zulu\zulu-21'
}
if (-not (Test-Path (Join-Path $jdk 'bin\javac.exe'))) { throw "javac not found; set JAVA_HOME to a JDK" }
$env:JAVA_HOME = $jdk

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$abi = $Abi
$target = switch ($abi) {
    'arm64-v8a' { 'aarch64-linux-android' }
    'x86_64' { 'x86_64-linux-android' }
    default { throw "unsupported ABI: $abi" }
}
# Rust env-var casing uses underscores (aarch64_linux_android / x86_64_linux_android).
$targetEnv = $target.Replace('-', '_')
# AMidi (midir's Android backend) only provides the NDK `libamidi` from API 29 on, so the minimum
# API level is 29.
$apiLevel = 29
$libName = 'surfing_studio_android'

# The NDK clang is both the linker and the C/C++ compiler for build scripts (cpal's oboe backend).
$toolchain = Join-Path $ndk 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$env:ANDROID_NDK_HOME = $ndk
$env:ANDROID_NDK_ROOT = $ndk
$env:ANDROID_HOME = $sdk
$env:ANDROID_SDK_ROOT = $sdk
$clang = Join-Path $toolchain "$target$apiLevel-clang.cmd"
$clangxx = Join-Path $toolchain "$target$apiLevel-clang++.cmd"
Set-Item -Path "env:CARGO_TARGET_$($targetEnv.ToUpper())_LINKER" -Value $clang
Set-Item -Path "env:CC_$targetEnv" -Value $clang
Set-Item -Path "env:CXX_$targetEnv" -Value $clangxx
Set-Item -Path "env:AR_$targetEnv" -Value (Join-Path $toolchain 'llvm-ar.exe')

Write-Host "=== 1/5 cargo build ($target) ===" -ForegroundColor Cyan
Push-Location $root
try {
    cargo build -p surfing-studio-android --target $target --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally {
    Pop-Location
}

$so = Join-Path $root "target\$target\release\lib$libName.so"
if (-not (Test-Path $so)) { throw "missing artifact: $so" }

$outDir = Join-Path $root 'target\android'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$manifest = Join-Path $PSScriptRoot 'AndroidManifest.xml'
$unsigned = Join-Path $outDir "SurfingStudio-$abi-unsigned.apk"
$aligned = Join-Path $outDir "SurfingStudio-$abi-aligned.apk"
$signed = Join-Path $outDir "SurfingStudio-$abi.apk"
Remove-Item $unsigned, $aligned, $signed -ErrorAction SilentlyContinue

Write-Host '=== 2/6 aapt2 link manifest ===' -ForegroundColor Cyan
& (Join-Path $buildTools 'aapt2.exe') link -o $unsigned -I $androidJar --manifest $manifest
if ($LASTEXITCODE -ne 0) { throw 'aapt2 link failed' }

Write-Host '=== 3/6 javac + d8 (activity subclass) ===' -ForegroundColor Cyan
$classesDir = Join-Path $outDir 'classes'
$dexDir = Join-Path $outDir 'dex'
Remove-Item $classesDir, $dexDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $classesDir | Out-Null
New-Item -ItemType Directory -Force -Path $dexDir | Out-Null
$javaSources = Get-ChildItem (Join-Path $PSScriptRoot 'java') -Recurse -Filter *.java |
    ForEach-Object { $_.FullName }
& (Join-Path $jdk 'bin\javac.exe') -source 8 -target 8 -encoding UTF-8 `
    -bootclasspath $androidJar -d $classesDir $javaSources
if ($LASTEXITCODE -ne 0) { throw 'javac failed' }
$classFiles = Get-ChildItem $classesDir -Recurse -Filter *.class | ForEach-Object { $_.FullName }
if (-not $classFiles) { throw 'javac produced no class files' }
& (Join-Path $buildTools 'd8.bat') --lib $androidJar --min-api $apiLevel --output $dexDir $classFiles
if ($LASTEXITCODE -ne 0) { throw 'd8 failed' }

Write-Host '=== 4/6 add native library and classes.dex ===' -ForegroundColor Cyan
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::Open($unsigned, 'Update')
try {
    $entry = $zip.CreateEntry("lib/$abi/lib$libName.so", [System.IO.Compression.CompressionLevel]::Optimal)
    $stream = $entry.Open()
    try {
        $bytes = [System.IO.File]::ReadAllBytes($so)
        $stream.Write($bytes, 0, $bytes.Length)
    } finally {
        $stream.Dispose()
    }
    $dexEntry = $zip.CreateEntry('classes.dex', [System.IO.Compression.CompressionLevel]::Optimal)
    $dexStream = $dexEntry.Open()
    try {
        $dexBytes = [System.IO.File]::ReadAllBytes((Join-Path $dexDir 'classes.dex'))
        $dexStream.Write($dexBytes, 0, $dexBytes.Length)
    } finally {
        $dexStream.Dispose()
    }
} finally {
    $zip.Dispose()
}

Write-Host '=== 5/6 zipalign ===' -ForegroundColor Cyan
& (Join-Path $buildTools 'zipalign.exe') -p -f 4 $unsigned $aligned
if ($LASTEXITCODE -ne 0) { throw 'zipalign failed' }

Write-Host '=== 6/6 apksigner ===' -ForegroundColor Cyan
$keystore = Join-Path $env:USERPROFILE '.android\debug.keystore'
if (-not (Test-Path $keystore)) { throw "debug keystore not found: $keystore" }
& (Join-Path $buildTools 'apksigner.bat') sign `
    --ks $keystore --ks-pass pass:android --key-pass pass:android `
    --ks-key-alias androiddebugkey --out $signed $aligned
if ($LASTEXITCODE -ne 0) { throw 'apksigner failed' }

Remove-Item $unsigned, $aligned -ErrorAction SilentlyContinue
Write-Host "APK: $signed" -ForegroundColor Green

if ($Install -or $Run) {
    Write-Host '=== adb install ===' -ForegroundColor Cyan
    & adb install -r $signed
    if ($LASTEXITCODE -ne 0) { throw 'adb install failed' }
}
if ($Run) {
    Write-Host '=== launching ===' -ForegroundColor Cyan
    # The manifest declares the MainActivity subclass (it forwards onActivityResult for SAF), so the
    # launch component must name it instead of android.app.NativeActivity.
    & adb shell am start -n com.surfingstudio.app/com.surfingstudio.app.MainActivity
}
