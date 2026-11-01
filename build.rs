//! 所属模块：构建脚本 / Windows 图标资源
//! 文件用途：
//! - 把 `assets/icon.ico` 编译成资源并链接进 `surfing-studio.exe`，让资源管理器/任务栏显示新图标。
//! - 找不到资源编译器时只发出警告，不影响其它平台与普通构建。
//!
//! Module: build script / Windows icon resource
//! Purpose:
//! - Compile `assets/icon.ico` and link it into `surfing-studio.exe` so Explorer and the taskbar
//!   show the new icon.
//! - Only warn when no resource compiler is available; other platforms and plain builds are
//!   unaffected.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    embed_icon();
}

/// 所属模块：构建脚本 / Windows 图标资源
/// 功能：用 rc.exe 或 windres 把图标资源链接到可执行文件。
/// 输入：无（读取 `CARGO_MANIFEST_DIR` 与 `OUT_DIR`）。
/// 输出/副作用：生成 `.res` / `.o` 并输出 `rustc-link-arg-bins`；失败时只打印警告。
/// 复杂度：时间 O(一次外部编译)，空间 O(1)。
///
/// Module: build script / Windows icon resource
/// Purpose: Link the icon resource through rc.exe or windres.
/// Inputs: none (reads `CARGO_MANIFEST_DIR` and `OUT_DIR`).
/// Outputs / side effects: writes `.res` / `.o` and emits `rustc-link-arg-bins`; only warns on
/// failure.
/// Complexity: O(one external compile) time and O(1) space.
#[cfg(windows)]
fn embed_icon() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icon = manifest.join("assets").join("icon.ico");
    if !icon.exists() {
        println!("cargo:warning=assets/icon.ico 不存在，跳过 Windows 图标资源");
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    let rc_file = out.join("app_icon.rc");
    let icon_text = icon.display().to_string().replace('\\', "\\\\");
    if std::fs::write(&rc_file, format!("IDI_ICON1 ICON \"{icon_text}\"\n")).is_err() {
        println!("cargo:warning=写入图标资源脚本失败，跳过 Windows 图标资源");
        return;
    }

    if let Some(rc) = find_rc() {
        let res = out.join("app_icon.res");
        let ok = Command::new(&rc)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res)
            .arg(&rc_file)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok && res.exists() {
            println!("cargo:rustc-link-arg-bins={}", res.display());
            return;
        }
        println!("cargo:warning=rc.exe 运行失败，尝试 windres");
    }

    let obj = out.join("app_icon.o");
    let ok = Command::new("windres")
        .arg("-i")
        .arg(&rc_file)
        .arg("-o")
        .arg(&obj)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if ok && obj.exists() {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        println!("cargo:warning=未找到 rc.exe / windres，可执行文件不会内嵌图标");
    }
}

/// 所属模块：构建脚本 / Windows 图标资源
/// 功能：定位 Windows SDK 的 rc.exe（取版本号最大的 x64 目录）。
/// 输入：无。
/// 输出/副作用：返回路径；找不到时返回 `None`。
/// 复杂度：时间 O(SDK 版本数)，空间 O(1)。
///
/// Module: build script / Windows icon resource
/// Purpose: Locate the Windows SDK rc.exe (highest version, x64).
/// Inputs: none.
/// Outputs / side effects: the path, or `None`.
/// Complexity: O(SDK versions) time and O(1) space.
#[cfg(windows)]
fn find_rc() -> Option<PathBuf> {
    if let Ok(program_files) = std::env::var("ProgramFiles(x86)") {
        let bin = Path::new(&program_files)
            .join("Windows Kits")
            .join("10")
            .join("bin");
        if let Ok(entries) = std::fs::read_dir(&bin) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_dir())
                .collect();
            versions.sort();
            versions.reverse();
            for version in versions {
                let candidate = version.join("x64").join("rc.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    Command::new("rc.exe")
        .arg("/?")
        .output()
        .ok()
        .filter(|output| output.status.success() || !output.stdout.is_empty())
        .map(|_| PathBuf::from("rc.exe"))
}
