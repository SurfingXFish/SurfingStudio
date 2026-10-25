//! 所属模块：core / 工程运行时 / SurfingStudio 后端 / 清单
//! 文件用途：
//! - 定义 SFS2 清单（MessagePack）与波形峰值表的序列化结构。
//! - 提供按稳定顺序遍历插件引用的辅助函数。
//!
//! 函数清单：
//! - `plugin_refs` / `plugin_refs_mut`：稳定顺序遍历插件引用。
//!
//! Module: core / project runtime / SurfingStudio backend / manifest
//! Purpose:
//! - Define the serialized SFS2 manifest (MessagePack) and waveform peak table.
//! - Provide helpers that traverse plugin references in a stable order.
//!
//! Functions:
//! - `plugin_refs` / `plugin_refs_mut`: traverse plugin references in stable order.

use crate::core::model::{InstrumentRef, PluginRef, Project};
use crate::core::project_runtime::WaveformPeak;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(super) struct ManifestRef<'a> {
    pub(super) project: &'a Project,
    /// 目录表中插件状态块的索引，顺序与 `plugin_refs` 一致。
    /// TOC indices of the plugin-state chunks, in [`plugin_refs`] order.
    pub(super) plugin_state_chunks: Vec<u32>,
}

#[derive(Deserialize)]
pub(super) struct Manifest {
    pub(super) project: Project,
    pub(super) plugin_state_chunks: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PeakTable {
    pub(super) peaks: Vec<WaveformPeak>,
}

/// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 清单
/// 功能：按保存与读取共用的稳定顺序遍历插件引用。
/// 输入：`project` 工程引用。
///
/// 输出/副作用：返回插件引用列表；不修改工程。
///
/// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(插件引用数)。
///
/// Module: core / project runtime / SurfingStudio backend / manifest
/// Purpose: Traverse plugin references in the stable order shared by save and load.
/// Inputs: `project` — project reference.
///
/// Outputs / side effects: returns the reference list; the project is unchanged.
///
/// Complexity: O(track count + effect count) time and O(plugin count) space.
pub(super) fn plugin_refs(project: &Project) -> Vec<&PluginRef> {
    let mut refs = Vec::new();
    for track in &project.tracks {
        if let InstrumentRef::Plugin(pref) = &track.instrument {
            refs.push(pref);
        }
        refs.extend(track.effects.iter().filter_map(|effect| effect.plugin()));
    }
    if let Some(pref) = project
        .master_effect
        .as_ref()
        .and_then(|effect| effect.plugin())
    {
        refs.push(pref);
    }
    refs
}

/// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 清单
/// 功能：按保存与读取共用的稳定顺序可变遍历插件引用。
/// 输入：`project` 工程可变引用。
///
/// 输出/副作用：返回可变插件引用列表；会借用工程。
///
/// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(插件引用数)。
///
/// Module: core / project runtime / SurfingStudio backend / manifest
/// Purpose: Mutably traverse plugin references in the stable save/load order.
/// Inputs: `project` — mutable project reference.
///
/// Outputs / side effects: returns mutable references; borrows the project.
///
/// Complexity: O(track count + effect count) time and O(plugin count) space.
pub(super) fn plugin_refs_mut(project: &mut Project) -> Vec<&mut PluginRef> {
    let mut refs = Vec::new();
    for track in &mut project.tracks {
        if let InstrumentRef::Plugin(pref) = &mut track.instrument {
            refs.push(pref);
        }
        refs.extend(
            track
                .effects
                .iter_mut()
                .filter_map(|effect| effect.plugin_mut()),
        );
    }
    if let Some(pref) = project
        .master_effect
        .as_mut()
        .and_then(|effect| effect.plugin_mut())
    {
        refs.push(pref);
    }
    refs
}
