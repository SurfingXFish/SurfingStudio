//! 所属模块：plugin / 内置插件资料库
//! 文件用途：
//! - 内置常见 VST 厂商、插件名称与分类的离线资料表，用于补全缺少 `moduleinfo.json`
//!   或厂商/分类为空、类型未知的扫描结果。
//! - 只做只读匹配，不实例化插件、不访问网络。
//!
//! 函数清单：
//! - `lookup`：按插件名称匹配资料条目。
//! - `normalize`：把名称规范化为小写并去掉多余空格，便于子串匹配。
//!
//! Module: plugin / built-in plugin catalog
//! Purpose:
//! - Offline catalog of common VST vendors, plugin names and categories, used to fill scan
//!   results that lack `moduleinfo.json` or have empty vendor/category/unknown kind.
//! - Read-only matching only: no plugin instantiation and no network access.
//!
//! Functions:
//! - `lookup`: match one catalog entry by plugin name.
//! - `normalize`: lower-case a name and collapse whitespace for substring matching.

/// 一条插件资料：匹配键、厂商、分类、是否为乐器与说明。
/// One catalog entry: match key, vendor, category, instrument flag and description.
pub struct CatalogEntry {
    /// 用于在插件名称中做不区分大小写的子串匹配。
    /// Case-insensitive substring matched against the plugin name.
    pub key: &'static str,
    pub vendor: &'static str,
    /// VST3 子分类风格的描述，例如 `Instrument / Synth`、`Fx / EQ`。
    /// VST3 sub-category style description, e.g. `Instrument / Synth`, `Fx / EQ`.
    pub category: &'static str,
    pub is_instrument: bool,
    pub description: &'static str,
}

/// 常见插件离线资料表。顺序无关；`lookup` 取第一个匹配项，因此更具体的键应排在前面。
/// Offline catalog of common plugins. Order-independent; `lookup` returns the first match,
/// so more specific keys come first.
pub const ENTRIES: &[CatalogEntry] = &[
    // ---- 合成器 / 采样器 ----
    CatalogEntry {
        key: "serum",
        vendor: "Xfer Records",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "波表合成器",
    },
    CatalogEntry {
        key: "vital",
        vendor: "Matt Tytel",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "免费波表合成器",
    },
    CatalogEntry {
        key: "sylenth",
        vendor: "LennarDigital",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "虚拟模拟合成器",
    },
    CatalogEntry {
        key: "massive",
        vendor: "Native Instruments",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "波表合成器",
    },
    CatalogEntry {
        key: "kontakt",
        vendor: "Native Instruments",
        category: "Instrument / Sampler",
        is_instrument: true,
        description: "采样器",
    },
    CatalogEntry {
        key: "omnisphere",
        vendor: "Spectrasonics",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "综合音源",
    },
    CatalogEntry {
        key: "keyscape",
        vendor: "Spectrasonics",
        category: "Instrument / Piano",
        is_instrument: true,
        description: "键盘与钢琴音源",
    },
    CatalogEntry {
        key: "trilian",
        vendor: "Spectrasonics",
        category: "Instrument / Bass",
        is_instrument: true,
        description: "贝斯音源",
    },
    CatalogEntry {
        key: "pigments",
        vendor: "Arturia",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "多引擎合成器",
    },
    CatalogEntry {
        key: "analog lab",
        vendor: "Arturia",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "经典合成器合集",
    },
    CatalogEntry {
        key: "diva",
        vendor: "u-he",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "模拟建模合成器",
    },
    CatalogEntry {
        key: "repro",
        vendor: "u-he",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "模拟复音合成器",
    },
    CatalogEntry {
        key: "zebra",
        vendor: "u-he",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "模块化合成器",
    },
    CatalogEntry {
        key: "hive",
        vendor: "u-he",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "波表合成器",
    },
    CatalogEntry {
        key: "spire",
        vendor: "Reveal Sound",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "混合合成器",
    },
    CatalogEntry {
        key: "phase plant",
        vendor: "Kilohearts",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "模块化合成器",
    },
    CatalogEntry {
        key: "avenger",
        vendor: "Vengeance Sound",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "综合合成器",
    },
    CatalogEntry {
        key: "nexus",
        vendor: "reFX",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "综合音源",
    },
    CatalogEntry {
        key: "sytrus",
        vendor: "Image-Line",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "FM 合成器",
    },
    CatalogEntry {
        key: "harmor",
        vendor: "Image-Line",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "加法合成器",
    },
    CatalogEntry {
        key: "surge",
        vendor: "Surge Synth Team",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "开源混合合成器",
    },
    CatalogEntry {
        key: "dexed",
        vendor: "Digital Suburban",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "FM 合成器",
    },
    CatalogEntry {
        key: "helm",
        vendor: "Matt Tytel",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "开源合成器",
    },
    // ---- 钢琴 / 吉他 / 贝斯 / 弦乐 ----
    CatalogEntry {
        key: "pianoteq",
        vendor: "Modartt",
        category: "Instrument / Piano",
        is_instrument: true,
        description: "物理建模钢琴",
    },
    CatalogEntry {
        key: "addictive keys",
        vendor: "XLN Audio",
        category: "Instrument / Piano",
        is_instrument: true,
        description: "键盘音源",
    },
    CatalogEntry {
        key: "ample bass",
        vendor: "Ample Sound",
        category: "Instrument / Bass",
        is_instrument: true,
        description: "贝斯音源",
    },
    CatalogEntry {
        key: "ample guitar",
        vendor: "Ample Sound",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "吉他音源",
    },
    CatalogEntry {
        key: "ample metal",
        vendor: "Ample Sound",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "金属吉他音源",
    },
    CatalogEntry {
        key: "realstrat",
        vendor: "MusicLab",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "电吉他音源",
    },
    CatalogEntry {
        key: "realguitar",
        vendor: "MusicLab",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "木吉他音源",
    },
    CatalogEntry {
        key: "realeight",
        vendor: "MusicLab",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "吉他音源",
    },
    CatalogEntry {
        key: "realbass",
        vendor: "MusicLab",
        category: "Instrument / Bass",
        is_instrument: true,
        description: "贝斯音源",
    },
    CatalogEntry {
        key: "shreddage",
        vendor: "Impact Soundworks",
        category: "Instrument / Guitar",
        is_instrument: true,
        description: "电吉他音源",
    },
    CatalogEntry {
        key: "stradivari",
        vendor: "Native Instruments",
        category: "Instrument / Strings",
        is_instrument: true,
        description: "独奏小提琴",
    },
    CatalogEntry {
        key: "swam violin",
        vendor: "Audio Modeling",
        category: "Instrument / Strings",
        is_instrument: true,
        description: "物理建模小提琴",
    },
    CatalogEntry {
        key: "swam cello",
        vendor: "Audio Modeling",
        category: "Instrument / Strings",
        is_instrument: true,
        description: "物理建模大提琴",
    },
    CatalogEntry {
        key: "cinematic studio strings",
        vendor: "Cinematic Studio Series",
        category: "Instrument / Strings",
        is_instrument: true,
        description: "弦乐音源",
    },
    CatalogEntry {
        key: "session strings",
        vendor: "Native Instruments",
        category: "Instrument / Strings",
        is_instrument: true,
        description: "弦乐音源",
    },
    CatalogEntry {
        key: "bbc symphony",
        vendor: "Spitfire Audio",
        category: "Instrument / Orchestra",
        is_instrument: true,
        description: "管弦乐音源",
    },
    CatalogEntry {
        key: "albion",
        vendor: "Spitfire Audio",
        category: "Instrument / Orchestra",
        is_instrument: true,
        description: "管弦乐音源",
    },
    // ---- 鼓 / 打击乐 ----
    CatalogEntry {
        key: "superior drummer",
        vendor: "Toontrack",
        category: "Instrument / Drums",
        is_instrument: true,
        description: "鼓音源",
    },
    CatalogEntry {
        key: "ezdrummer",
        vendor: "Toontrack",
        category: "Instrument / Drums",
        is_instrument: true,
        description: "鼓音源",
    },
    CatalogEntry {
        key: "addictive drums",
        vendor: "XLN Audio",
        category: "Instrument / Drums",
        is_instrument: true,
        description: "鼓音源",
    },
    CatalogEntry {
        key: "battery",
        vendor: "Native Instruments",
        category: "Instrument / Drums",
        is_instrument: true,
        description: "鼓采样器",
    },
    CatalogEntry {
        key: "steven slate drums",
        vendor: "Steven Slate Drums",
        category: "Instrument / Drums",
        is_instrument: true,
        description: "鼓音源",
    },
    // ---- 效果器：EQ / 动态 / 限制 ----
    CatalogEntry {
        key: "pro-q",
        vendor: "FabFilter",
        category: "Fx / EQ",
        is_instrument: false,
        description: "均衡器",
    },
    CatalogEntry {
        key: "pro-c",
        vendor: "FabFilter",
        category: "Fx / Dynamics",
        is_instrument: false,
        description: "压缩器",
    },
    CatalogEntry {
        key: "pro-l",
        vendor: "FabFilter",
        category: "Fx / Limiter",
        is_instrument: false,
        description: "限制器",
    },
    CatalogEntry {
        key: "pro-mb",
        vendor: "FabFilter",
        category: "Fx / Dynamics",
        is_instrument: false,
        description: "多段动态",
    },
    CatalogEntry {
        key: "pro-ds",
        vendor: "FabFilter",
        category: "Fx / De-esser",
        is_instrument: false,
        description: "齿音消除",
    },
    CatalogEntry {
        key: "pro-g",
        vendor: "FabFilter",
        category: "Fx / Gate",
        is_instrument: false,
        description: "噪声门",
    },
    CatalogEntry {
        key: "pro-r",
        vendor: "FabFilter",
        category: "Fx / Reverb",
        is_instrument: false,
        description: "混响",
    },
    CatalogEntry {
        key: "saturn",
        vendor: "FabFilter",
        category: "Fx / Saturation",
        is_instrument: false,
        description: "多段饱和",
    },
    CatalogEntry {
        key: "timeless",
        vendor: "FabFilter",
        category: "Fx / Delay",
        is_instrument: false,
        description: "延时",
    },
    CatalogEntry {
        key: "volcano",
        vendor: "FabFilter",
        category: "Fx / Filter",
        is_instrument: false,
        description: "滤波器",
    },
    CatalogEntry {
        key: "twin",
        vendor: "FabFilter",
        category: "Fx / Synth",
        is_instrument: false,
        description: "合成器",
    },
    CatalogEntry {
        key: "fabfilter one",
        vendor: "FabFilter",
        category: "Instrument / Synth",
        is_instrument: true,
        description: "合成器",
    },
    CatalogEntry {
        key: "tdr nova",
        vendor: "Tokyo Dawn Records",
        category: "Fx / EQ",
        is_instrument: false,
        description: "动态均衡",
    },
    CatalogEntry {
        key: "kotelnikov",
        vendor: "Tokyo Dawn Records",
        category: "Fx / Dynamics",
        is_instrument: false,
        description: "压缩器",
    },
    CatalogEntry {
        key: "limiter 6",
        vendor: "Tokyo Dawn Records",
        category: "Fx / Limiter",
        is_instrument: false,
        description: "限制器",
    },
    CatalogEntry {
        key: "soothe",
        vendor: "oeksound",
        category: "Fx / Dynamic EQ",
        is_instrument: false,
        description: "动态共振抑制",
    },
    CatalogEntry {
        key: "spiff",
        vendor: "oeksound",
        category: "Fx / Transient",
        is_instrument: false,
        description: "瞬态整形",
    },
    CatalogEntry {
        key: "gullfoss",
        vendor: "Soundtheory",
        category: "Fx / EQ",
        is_instrument: false,
        description: "智能均衡",
    },
    CatalogEntry {
        key: "ozone",
        vendor: "iZotope",
        category: "Fx / Mastering",
        is_instrument: false,
        description: "母带处理",
    },
    CatalogEntry {
        key: "neutron",
        vendor: "iZotope",
        category: "Fx / Channel Strip",
        is_instrument: false,
        description: "混音通道条",
    },
    CatalogEntry {
        key: "nectar",
        vendor: "iZotope",
        category: "Fx / Vocal",
        is_instrument: false,
        description: "人声处理",
    },
    CatalogEntry {
        key: "rx ",
        vendor: "iZotope",
        category: "Fx / Repair",
        is_instrument: false,
        description: "音频修复",
    },
    CatalogEntry {
        key: "1176",
        vendor: "Universal Audio",
        category: "Fx / Compressor",
        is_instrument: false,
        description: "压缩器",
    },
    CatalogEntry {
        key: "la-2a",
        vendor: "Universal Audio",
        category: "Fx / Compressor",
        is_instrument: false,
        description: "光学压缩器",
    },
    CatalogEntry {
        key: "pultec",
        vendor: "Universal Audio",
        category: "Fx / EQ",
        is_instrument: false,
        description: "均衡器",
    },
    CatalogEntry {
        key: "bx_console",
        vendor: "Brainworx",
        category: "Fx / Channel Strip",
        is_instrument: false,
        description: "调音台通道条",
    },
    CatalogEntry {
        key: "bx_digital",
        vendor: "Brainworx",
        category: "Fx / EQ",
        is_instrument: false,
        description: "母带均衡",
    },
    // ---- 效果器：混响 / 延时 / 调制 / 失真 ----
    CatalogEntry {
        key: "valhalla vintageverb",
        vendor: "Valhalla DSP",
        category: "Fx / Reverb",
        is_instrument: false,
        description: "复古混响",
    },
    CatalogEntry {
        key: "valhalla room",
        vendor: "Valhalla DSP",
        category: "Fx / Reverb",
        is_instrument: false,
        description: "房间混响",
    },
    CatalogEntry {
        key: "valhalla shimmer",
        vendor: "Valhalla DSP",
        category: "Fx / Reverb",
        is_instrument: false,
        description: "微光混响",
    },
    CatalogEntry {
        key: "valhalla delay",
        vendor: "Valhalla DSP",
        category: "Fx / Delay",
        is_instrument: false,
        description: "延时",
    },
    CatalogEntry {
        key: "supermassive",
        vendor: "Valhalla DSP",
        category: "Fx / Delay",
        is_instrument: false,
        description: "免费延时混响",
    },
    CatalogEntry {
        key: "decapitator",
        vendor: "Soundtoys",
        category: "Fx / Distortion",
        is_instrument: false,
        description: "失真饱和",
    },
    CatalogEntry {
        key: "echoboy",
        vendor: "Soundtoys",
        category: "Fx / Delay",
        is_instrument: false,
        description: "延时",
    },
    CatalogEntry {
        key: "little alterboy",
        vendor: "Soundtoys",
        category: "Fx / Pitch",
        is_instrument: false,
        description: "变声",
    },
    CatalogEntry {
        key: "devil-loc",
        vendor: "Soundtoys",
        category: "Fx / Distortion",
        is_instrument: false,
        description: "压缩失真",
    },
    CatalogEntry {
        key: "sie-q",
        vendor: "Soundtoys",
        category: "Fx / EQ",
        is_instrument: false,
        description: "均衡器",
    },
    CatalogEntry {
        key: "auto-tune",
        vendor: "Antares",
        category: "Fx / Pitch",
        is_instrument: false,
        description: "音高修正",
    },
    CatalogEntry {
        key: "chorus",
        vendor: "Waves",
        category: "Fx / Modulation",
        is_instrument: false,
        description: "合唱",
    },
    CatalogEntry {
        key: "h-delay",
        vendor: "Waves",
        category: "Fx / Delay",
        is_instrument: false,
        description: "延时",
    },
    CatalogEntry {
        key: "rc-20",
        vendor: "XLN Audio",
        category: "Fx / Lo-Fi",
        is_instrument: false,
        description: "低保真效果",
    },
    CatalogEntry {
        key: "sketchcassette",
        vendor: "Aberrant DSP",
        category: "Fx / Lo-Fi",
        is_instrument: false,
        description: "磁带模拟",
    },
    CatalogEntry {
        key: "kelvin",
        vendor: "Tone Projects",
        category: "Fx / Saturation",
        is_instrument: false,
        description: "饱和器",
    },
    CatalogEntry {
        key: "guitar rig",
        vendor: "Native Instruments",
        category: "Fx / Amp Sim",
        is_instrument: false,
        description: "吉他音箱模拟",
    },
    CatalogEntry {
        key: "archetype",
        vendor: "Neural DSP",
        category: "Fx / Amp Sim",
        is_instrument: false,
        description: "吉他音箱模拟",
    },
    CatalogEntry {
        key: "bias fx",
        vendor: "Positive Grid",
        category: "Fx / Amp Sim",
        is_instrument: false,
        description: "吉他音箱模拟",
    },
    CatalogEntry {
        key: "th-u",
        vendor: "Overloud",
        category: "Fx / Amp Sim",
        is_instrument: false,
        description: "吉他音箱模拟",
    },
];

/// 所属模块：plugin / 内置插件资料库
/// 功能：把插件名称规范化为小写并压缩空白，便于稳定的子串匹配。
/// 输入：`name` 插件显示名。
/// 输出/副作用：返回规范化字符串；不修改外部状态。
/// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
///
/// Module: plugin / built-in plugin catalog
/// Purpose: Lower-case a plugin name and collapse whitespace for stable substring matching.
/// Inputs: `name` — plugin display name.
/// Outputs / side effects: normalized string; no external state change.
/// Complexity: O(name length) time and space.
fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_space = false;
    for character in name.chars() {
        if character.is_whitespace() {
            if !last_space && !out.is_empty() {
                out.push(' ');
            }
            last_space = true;
        } else {
            for lower in character.to_lowercase() {
                out.push(lower);
            }
            last_space = false;
        }
    }
    out.trim_end().to_string()
}

/// 所属模块：plugin / 内置插件资料库
/// 功能：按插件名称查找资料；匹配为不区分大小写的子串匹配。
/// 输入：`name` 插件显示名。
/// 输出/副作用：返回命中的资料条目；未命中时为 `None`；不修改状态。
/// 复杂度：时间 O(资料条数 × 名称长度)，空间 O(名称长度)。
///
/// Module: plugin / built-in plugin catalog
/// Purpose: Look up a plugin by name using case-insensitive substring matching.
/// Inputs: `name` — plugin display name.
/// Outputs / side effects: the matched entry, or `None`; no state change.
/// Complexity: O(catalog entries × name length) time and O(name length) space.
pub fn lookup(name: &str) -> Option<&'static CatalogEntry> {
    let name = normalize(name);
    if name.is_empty() {
        return None;
    }
    ENTRIES.iter().find(|entry| name.contains(entry.key))
}
