//! 所属模块：core / 数据模型 / 声道布局
//! 文件用途：
//! - 定义工程输出声道布局（立体声 / 5.1 / 7.1）与通道对路由名称。
//! - 为引擎、导出与界面提供纯数据的声道数与通道对描述，不依赖音频或 UI 运行时。
//!
//! 函数清单：
//! - `ChannelLayout::ALL` / `channels` / `pairs` / `label` / `channel_names` / `pair_label` / `from_id` / `id`。
//! - `default_layout`：serde 默认值（立体声）。
//!
//! Module: core / data model / channel layout
//! Purpose:
//! - Define the project output layout (stereo / 5.1 / 7.1) and channel-pair routing names.
//! - Provide pure channel counts and pair descriptions for the engine, export and UI without
//!   depending on audio or UI runtimes.
//!
//! Functions:
//! - `ChannelLayout::ALL` / `channels` / `pairs` / `label` / `channel_names` / `pair_label` /
//!   `from_id` / `id`.
//! - `default_layout`: serde default (stereo).

use serde::{Deserialize, Serialize};
use std::fmt;

/// 工程输出声道布局。
/// Project output channel layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayout {
    /// 立体声：前左 / 前右。
    /// Stereo: front left / front right.
    #[default]
    Stereo,
    /// 5.1 环绕：前左 / 前右 / 中置 / 低频 / 环绕左 / 环绕右。
    /// 5.1 surround: FL / FR / C / LFE / SL / SR.
    Surround51,
    /// 7.1 环绕：5.1 加后左 / 后右。
    /// 7.1 surround: 5.1 plus back left / back right.
    Surround71,
}

impl ChannelLayout {
    /// 全部布局；界面下拉框按此顺序展示。
    /// Every layout; the UI dropdown lists them in this order.
    pub const ALL: [ChannelLayout; 3] = [
        ChannelLayout::Stereo,
        ChannelLayout::Surround51,
        ChannelLayout::Surround71,
    ];

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回持久化用的稳定标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return the stable persisted id.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            ChannelLayout::Stereo => "stereo",
            ChannelLayout::Surround51 => "surround51",
            ChannelLayout::Surround71 => "surround71",
        }
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回面向用户的中文名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return the user-facing Chinese label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            ChannelLayout::Stereo => "立体声（2.0）",
            ChannelLayout::Surround51 => "环绕声（5.1）",
            ChannelLayout::Surround71 => "环绕声（7.1）",
        }
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回声道总数。
    /// 输入：`self`。
    /// 输出/副作用：2 / 6 / 8；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return the total channel count.
    /// Inputs: `self`.
    /// Outputs / side effects: 2 / 6 / 8; no state change.
    /// Complexity: O(1) time and space.
    pub fn channels(self) -> usize {
        match self {
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround51 => 6,
            ChannelLayout::Surround71 => 8,
        }
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回可路由的立体声通道对数量。
    /// 输入：`self`。
    /// 输出/副作用：1 / 3 / 4；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return the number of routable stereo channel pairs.
    /// Inputs: `self`.
    /// Outputs / side effects: 1 / 3 / 4; no state change.
    /// Complexity: O(1) time and space.
    pub fn pairs(self) -> usize {
        self.channels() / 2
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回声道名称（按标准电影顺序）。
    /// 输入：`self`。
    /// 输出/副作用：静态名称切片；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return channel names in standard film order.
    /// Inputs: `self`.
    /// Outputs / side effects: a static name slice; no state change.
    /// Complexity: O(1) time and space.
    pub fn channel_names(self) -> &'static [&'static str] {
        match self {
            ChannelLayout::Stereo => &["L", "R"],
            ChannelLayout::Surround51 => &["FL", "FR", "C", "LFE", "SL", "SR"],
            ChannelLayout::Surround71 => &["FL", "FR", "C", "LFE", "SL", "SR", "BL", "BR"],
        }
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：返回第 `pair` 个立体声通道对的中文名称。
    /// 输入：`pair`：通道对下标（越界时回退到前左/前右）。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Return the Chinese name of the `pair`-th stereo channel pair (out-of-range falls back
    /// to front left/right).
    /// Inputs: `pair`: pair index.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn pair_label(self, pair: usize) -> &'static str {
        match (self, pair) {
            (_, 0) => "前左 / 前右",
            (ChannelLayout::Surround51 | ChannelLayout::Surround71, 1) => "中置 / 低频",
            (ChannelLayout::Surround51, 2) => "环绕左 / 环绕右",
            (ChannelLayout::Surround71, 2) => "环绕左 / 环绕右",
            (ChannelLayout::Surround71, 3) => "后左 / 后右",
            _ => "前左 / 前右",
        }
    }

    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：把持久化标识还原为布局；未知标识回退立体声。
    /// 输入：`id`：设置或工程中的标识。
    /// 输出/副作用：布局；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Restore a layout from its persisted id; unknown ids fall back to stereo.
    /// Inputs: `id`: the stored id.
    /// Outputs / side effects: the layout; no state change.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        match id {
            "surround51" => ChannelLayout::Surround51,
            "surround71" => ChannelLayout::Surround71,
            _ => ChannelLayout::Stereo,
        }
    }
}

impl fmt::Display for ChannelLayout {
    /// 所属模块：core / 数据模型 / 声道布局
    /// 功能：以中文名称输出布局。
    /// 输入：`self` / `formatter`。
    /// 输出/副作用：写入格式化器；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / channel layout
    /// Purpose: Write the Chinese label.
    /// Inputs: `self` / `formatter`.
    /// Outputs / side effects: writes to the formatter; no state change.
    /// Complexity: O(1) time and space.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// 所属模块：core / 数据模型 / 声道布局
/// 功能：`Project::output_layout` 的 serde 默认值：立体声。
/// 输入：无。
/// 输出/副作用：默认布局；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / channel layout
/// Purpose: serde default for `Project::output_layout`: stereo.
/// Inputs: none.
/// Outputs / side effects: the default layout; no state change.
/// Complexity: O(1) time and space.
pub fn default_layout() -> ChannelLayout {
    ChannelLayout::Stereo
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：声道数、通道对与标识往返一致。
    /// Covers: channel counts, pair counts and id round-trips stay consistent.
    #[test]
    fn layout_counts_and_ids_match() {
        assert_eq!(ChannelLayout::Stereo.channels(), 2);
        assert_eq!(ChannelLayout::Surround51.channels(), 6);
        assert_eq!(ChannelLayout::Surround71.channels(), 8);
        for layout in ChannelLayout::ALL {
            assert_eq!(layout.pairs() * 2, layout.channels());
            assert_eq!(ChannelLayout::from_id(layout.id()), layout);
            assert_eq!(layout.channel_names().len(), layout.channels());
        }
        assert_eq!(ChannelLayout::Surround71.pair_label(3), "后左 / 后右");
    }
}
