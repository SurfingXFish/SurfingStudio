//! 所属模块：app / 多语言
//! 文件用途：
//! - 加载内嵌的 YAML 语言目录，并按“当前语言 → 中文 → 代码原文”的顺序回退。
//! - 识别系统语言，提供界面语言切换与查询。
//!
//! 函数清单：
//! - `t` / `t_owned` / `t_args`：查询翻译文本（静态键、动态键与占位符）。
//! - `language` / `set_language` / `language_name`：当前语言查询与切换。
//! - `normalize_code` / `language_for_locale` / `language_for_system` / `system_language`：
//!   语言代码解析与系统语言识别。
//! - `catalogs` / `parse_catalog` / `resolve_translation`：YAML 目录加载与回退链。
//!
//! Module: app / localization
//! Purpose:
//! - Load the embedded YAML language catalogs and fall back active → Chinese → source text.
//! - Detect the system language and expose language switching/lookup for the UI.
//!
//! Functions:
//! - `t` / `t_owned` / `t_args`: look up translations (static keys, dynamic keys, placeholders).
//! - `language` / `set_language` / `language_name`: query and switch the active language.
//! - `normalize_code` / `language_for_locale` / `language_for_system` / `system_language`:
//!   resolve language codes and detect the system language.
//! - `catalogs` / `parse_catalog` / `resolve_translation`: load YAML catalogs and the fallback chain.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 源语言：所有界面文案的中文原文。
/// Source language: the Chinese original of every UI string.
pub const SOURCE_LANGUAGE: &str = "zh-CN";

/// 系统语言没有对应目录时使用的语言。
/// Language used when the system locale has no matching catalog.
pub const FALLBACK_LANGUAGE: &str = "en-US";

/// 可选界面语言。
/// Selectable UI languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageInfo {
    /// 持久化与日志使用的稳定代码。
    /// Stable code used in settings and logs.
    pub code: &'static str,
    /// 语言自身的名称，用于语言下拉框。
    /// Endonym shown in the language selector.
    pub native_name: &'static str,
}

/// 随程序内嵌的语言列表；顺序即设置页下拉框顺序。
/// Languages embedded in the binary; the order is the order shown in settings.
pub const LANGUAGES: [LanguageInfo; 2] = [
    LanguageInfo {
        code: "zh-CN",
        native_name: "简体中文",
    },
    LanguageInfo {
        code: "en-US",
        native_name: "English",
    },
];

/// 内嵌的 YAML 目录：代码与文件内容一一对应。
/// Embedded YAML catalogs: (code, file contents).
const BUILTIN_CATALOGS: [(&str, &str); 2] = [
    ("zh-CN", include_str!("../../assets/i18n/zh-CN.yml")),
    ("en-US", include_str!("../../assets/i18n/en-US.yml")),
];

/// 当前语言在 `LANGUAGES` 中的下标。
/// Index of the active language inside `LANGUAGES`.
static ACTIVE_LANGUAGE: AtomicUsize = AtomicUsize::new(0);

/// 解析后的语言目录；首次查询时初始化。
/// Parsed language catalogs; initialized on the first lookup.
static CATALOGS: OnceLock<Vec<Catalog>> = OnceLock::new();

/// 单个语言的字符串表（中文原文 → 译文）。
/// String table of one language (Chinese source → translation).
struct Catalog {
    code: &'static str,
    strings: HashMap<String, String>,
}

/// YAML 目录的文件结构；`language` 元数据只供人阅读，解析时忽略。
/// YAML catalog file layout; the `language` metadata is for humans and ignored here.
#[derive(Deserialize)]
struct CatalogFile {
    #[serde(default)]
    strings: Option<BTreeMap<String, String>>,
}

/// 所属模块：app / 多语言
/// 功能：查询静态键的译文；缺失时回退中文目录，仍缺失则返回代码中的中文原文。
/// 输入：`source`：中文原文键（必须是 `'static` 字面量）。
/// 输出/副作用：译文或中文原文；不修改状态。
/// 复杂度：时间 O(1) 均摊（哈希查找），空间 O(1)。
///
/// Module: app / localization
/// Purpose: Look up a static key; fall back to the Chinese catalog, then to the source text.
/// Inputs: `source`: Chinese source key (a `'static` literal).
/// Outputs / side effects: the translation or Chinese source; no state change.
/// Complexity: O(1) amortized time (hash lookup), O(1) space.
pub fn t(source: &'static str) -> &'static str {
    resolve_translation(catalogs(), active_index(), source).unwrap_or(source)
}

/// 所属模块：app / 多语言
/// 功能：查询动态字符串的译文；缺失时回退中文目录，仍缺失则复制原文。
/// 输入：`source`：运行期得到的文本（通常是中文原文）。
/// 输出/副作用：译文或原文的拥有副本；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / localization
/// Purpose: Look up a dynamic string; fall back to the Chinese catalog, then copy the source.
/// Inputs: `source`: runtime text (usually the Chinese original).
/// Outputs / side effects: an owned translation or source copy; no state change.
/// Complexity: O(text length) time and space.
pub fn t_owned(source: &str) -> String {
    resolve_translation(catalogs(), active_index(), source)
        .unwrap_or(source)
        .to_string()
}

/// 所属模块：app / 多语言
/// 功能：查询带占位符的静态键，并按顺序替换译文中的 `{}`。
/// 输入：
/// - `source`：中文原文键，译文必须保留相同数量的 `{}`。
/// - `args`：依次填入占位符的文本。
/// 输出/副作用：替换后的文本；不修改状态。
/// 复杂度：时间 O(译文长度 + 参数总长度)，空间 O(译文长度)。
///
/// Module: app / localization
/// Purpose: Look up a static key with placeholders and replace `{}` in order.
/// Inputs:
/// - `source`: Chinese source key; the translation must keep the same number of `{}`.
/// - `args`: texts filled into the placeholders in order.
/// Outputs / side effects: the substituted text; no state change.
/// Complexity: O(translation length + total argument length) time, O(translation length) space.
pub fn t_args(source: &'static str, args: &[&str]) -> String {
    let mut text = t(source).to_string();
    for arg in args {
        let Some(position) = text.find("{}") else {
            break;
        };
        text.replace_range(position..position + 2, arg);
    }
    text
}

/// 所属模块：app / 多语言
/// 功能：返回当前界面语言代码。
/// 输入：无。
/// 输出/副作用：语言代码；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Return the active UI language code.
/// Inputs: none.
/// Outputs / side effects: the language code; no state change.
/// Complexity: O(1) time and space.
pub fn language() -> &'static str {
    LANGUAGES[active_index()].code
}

/// 所属模块：app / 多语言
/// 功能：切换界面语言；未知代码被忽略。
/// 输入：`code`：语言代码（大小写不敏感，可带区域后缀）。
/// 输出/副作用：写入 `ACTIVE_LANGUAGE`；返回是否切换成功。
/// 复杂度：时间 O(语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Switch the UI language; unknown codes are ignored.
/// Inputs: `code`: language code (case-insensitive, may carry a region suffix).
/// Outputs / side effects: writes `ACTIVE_LANGUAGE`; returns whether the switch succeeded.
/// Complexity: O(languages) time, O(1) space.
pub fn set_language(code: &str) -> bool {
    let Some(canonical) = normalize_code(code) else {
        return false;
    };
    let index = LANGUAGES
        .iter()
        .position(|info| info.code == canonical)
        .unwrap_or(0);
    ACTIVE_LANGUAGE.store(index, Ordering::Relaxed);
    true
}

/// 所属模块：app / 多语言
/// 功能：把语言代码规范化为 `LANGUAGES` 中的稳定代码。
/// 输入：`code`：语言代码或区域标识（如 `en-us`、`zh-Hans-CN`）。
/// 输出/副作用：稳定代码或 `None`；不修改状态。
/// 复杂度：时间 O(语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Normalize a language code to a stable code in `LANGUAGES`.
/// Inputs: `code`: a language or locale id (e.g. `en-us`, `zh-Hans-CN`).
/// Outputs / side effects: the stable code or `None`; no state change.
/// Complexity: O(languages) time, O(1) space.
pub fn normalize_code(code: &str) -> Option<&'static str> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(info) = LANGUAGES
        .iter()
        .find(|info| info.code.eq_ignore_ascii_case(trimmed))
    {
        return Some(info.code);
    }
    language_for_locale(trimmed)
}

/// 所属模块：app / 多语言
/// 功能：按语言主标签匹配目录；同一语言存在多个区域时取列表中的第一个（如 `zh-TW` → `zh-CN`）。
/// 输入：`locale`：系统区域标识（`zh-Hans-CN`、`en_US.UTF-8` 等）。
/// 输出/副作用：目录代码或 `None`；不修改状态。
/// 复杂度：时间 O(语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Match a catalog by primary language; the first listed region wins (e.g. `zh-TW` → `zh-CN`).
/// Inputs: `locale`: system locale id (`zh-Hans-CN`, `en_US.UTF-8`, ...).
/// Outputs / side effects: the catalog code or `None`; no state change.
/// Complexity: O(languages) time, O(1) space.
pub fn language_for_locale(locale: &str) -> Option<&'static str> {
    let lower = locale.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    let primary = lower
        .split(|character| matches!(character, '-' | '_' | '.'))
        .next()
        .unwrap_or("");
    if primary.is_empty() {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|info| {
            info.code
                .split('-')
                .next()
                .is_some_and(|code| code.eq_ignore_ascii_case(primary))
        })
        .map(|info| info.code)
}

/// 所属模块：app / 多语言
/// 功能：把系统区域标识解析为界面语言；无对应目录时使用 `FALLBACK_LANGUAGE`。
/// 输入：`system_locale`：系统区域标识，未知时为 `None`。
/// 输出/副作用：语言代码；不修改状态。
/// 复杂度：时间 O(语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Resolve a system locale to a UI language; use `FALLBACK_LANGUAGE` when unmatched.
/// Inputs: `system_locale`: system locale id, or `None` when unknown.
/// Outputs / side effects: the language code; no state change.
/// Complexity: O(languages) time, O(1) space.
pub fn language_for_system(system_locale: Option<&str>) -> &'static str {
    system_locale
        .and_then(language_for_locale)
        .unwrap_or(FALLBACK_LANGUAGE)
}

/// 所属模块：app / 多语言
/// 功能：读取系统语言并解析为界面语言；首次启动由设置层调用。
/// 输入：无。
/// 输出/副作用：语言代码；会读取一次系统区域信息。
/// 复杂度：时间 O(系统调用 + 语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Read the system language and resolve it to a UI language; used on first launch.
/// Inputs: none.
/// Outputs / side effects: the language code; reads the system locale once.
/// Complexity: O(system call + languages) time, O(1) space.
pub fn system_language() -> &'static str {
    let locale = crate::platform::system_locale();
    language_for_system(locale.as_deref())
}

/// 所属模块：app / 多语言
/// 功能：返回语言自身的名称；未知代码返回 `None`。
/// 输入：`code`：语言代码。
/// 输出/副作用：展示名称或 `None`；不修改状态。
/// 复杂度：时间 O(语言数)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Return the language endonym; `None` for unknown codes.
/// Inputs: `code`: language code.
/// Outputs / side effects: the display name or `None`; no state change.
/// Complexity: O(languages) time, O(1) space.
pub fn language_name(code: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .find(|info| info.code.eq_ignore_ascii_case(code))
        .map(|info| info.native_name)
}

/// 所属模块：app / 多语言
/// 功能：返回当前语言下标，越界时回到 0。
/// 输入：无。
/// 输出/副作用：下标；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / localization
/// Purpose: Return the active language index, clamped to 0.
/// Inputs: none.
/// Outputs / side effects: the index; no state change.
/// Complexity: O(1) time and space.
fn active_index() -> usize {
    ACTIVE_LANGUAGE
        .load(Ordering::Relaxed)
        .min(LANGUAGES.len() - 1)
}

/// 所属模块：app / 多语言
/// 功能：返回解析后的语言目录；首次调用时解析全部内嵌 YAML。
/// 输入：无。
/// 输出/副作用：静态目录切片；首次调用会分配哈希表。
/// 复杂度：时间 O(语言文件总大小)，空间 O(键值总数)。
///
/// Module: app / localization
/// Purpose: Return the parsed catalogs; the first call parses every embedded YAML file.
/// Inputs: none.
/// Outputs / side effects: a static catalog slice; the first call allocates hash tables.
/// Complexity: O(total catalog size) time and space.
fn catalogs() -> &'static [Catalog] {
    CATALOGS.get_or_init(|| {
        BUILTIN_CATALOGS
            .iter()
            .map(|(code, text)| parse_catalog(code, text))
            .collect()
    })
}

/// 所属模块：app / 多语言
/// 功能：解析单个 YAML 目录；内容损坏时返回空目录并继续使用中文回退。
/// 输入：
/// - `code`：语言代码。
/// - `text`：YAML 文本。
/// 输出/副作用：目录结构；不修改状态。
/// 复杂度：时间 O(文件大小)，空间 O(键值数)。
///
/// Module: app / localization
/// Purpose: Parse one YAML catalog; a broken file yields an empty catalog and falls back to Chinese.
/// Inputs:
/// - `code`: language code.
/// - `text`: YAML text.
/// Outputs / side effects: the catalog; no state change.
/// Complexity: O(file size) time and O(entries) space.
fn parse_catalog(code: &'static str, text: &str) -> Catalog {
    let strings = serde_yaml_ng::from_str::<CatalogFile>(text)
        .ok()
        .and_then(|file| file.strings)
        .map(|entries| entries.into_iter().collect())
        .unwrap_or_default();
    Catalog { code, strings }
}

/// 所属模块：app / 多语言
/// 功能：按“当前语言 → 中文 → 无结果”执行回退链。
/// 输入：
/// - `catalogs`：全部语言目录。
/// - `active`：当前语言下标。
/// - `source`：中文原文键。
/// 输出/副作用：命中译文，未命中返回 `None`；不修改状态。
/// 复杂度：时间 O(1) 均摊（两次哈希查找），空间 O(1)。
///
/// Module: app / localization
/// Purpose: Apply the fallback chain active language → Chinese → none.
/// Inputs:
/// - `catalogs`: all language catalogs.
/// - `active`: active language index.
/// - `source`: Chinese source key.
/// Outputs / side effects: the translation, or `None` when nothing matches; no state change.
/// Complexity: O(1) amortized time (two hash lookups), O(1) space.
fn resolve_translation<'a>(
    catalogs: &'a [Catalog],
    active: usize,
    source: &str,
) -> Option<&'a str> {
    if let Some(catalog) = catalogs.get(active) {
        if let Some(value) = catalog.strings.get(source) {
            return Some(value.as_str());
        }
    }
    catalogs
        .iter()
        .find(|catalog| catalog.code == SOURCE_LANGUAGE)
        .and_then(|catalog| catalog.strings.get(source))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：内嵌目录全部可解析，英文目录包含实际翻译。
    /// Covers: every embedded catalog parses and the English catalog carries real translations.
    #[test]
    fn builtin_catalogs_parse() {
        let catalogs = catalogs();
        assert_eq!(catalogs.len(), LANGUAGES.len());
        let english = catalogs
            .iter()
            .find(|catalog| catalog.code == "en-US")
            .expect("en-US catalog");
        assert!(
            english.strings.len() > 100,
            "en-US catalog too small: {}",
            english.strings.len()
        );
        assert_eq!(
            english.strings.get("界面语言").map(String::as_str),
            Some("Language")
        );
    }

    /// 覆盖：当前语言缺失时回退中文目录，中文也缺失时回退代码原文。
    /// Covers: fall back to Chinese when the active catalog misses a key, then to the source text.
    #[test]
    fn translation_falls_back_to_chinese_then_source() {
        let catalogs = vec![
            parse_catalog(
                "zh-CN",
                "strings:\n  \"仅中文\": \"中文值\"\n  \"都缺失\": \"中文兜底\"\n  \"两处都有\": \"中文值\"\n",
            ),
            parse_catalog(
                "en-US",
                "strings:\n  \"两处都有\": \"English value\"\n  \"仅英文\": \"English only\"\n",
            ),
        ];
        assert_eq!(
            resolve_translation(&catalogs, 1, "两处都有"),
            Some("English value")
        );
        assert_eq!(resolve_translation(&catalogs, 1, "仅中文"), Some("中文值"));
        assert_eq!(
            resolve_translation(&catalogs, 1, "都缺失"),
            Some("中文兜底")
        );
        assert_eq!(resolve_translation(&catalogs, 1, "代码原文"), None);
    }

    /// 覆盖：损坏的 YAML 目录不 panic，返回空表。
    /// Covers: a broken YAML catalog does not panic and yields an empty table.
    #[test]
    fn broken_catalog_is_empty() {
        let catalog = parse_catalog("en-US", "strings: [unterminated");
        assert!(catalog.strings.is_empty());
    }

    /// 覆盖：占位符按顺序替换，缺少参数时保留剩余占位符。
    /// Covers: placeholders are replaced in order and leftovers are kept when arguments run out.
    #[test]
    fn placeholders_are_replaced_in_order() {
        set_language("zh-CN");
        assert_eq!(t_args("加载 {}", &["A"]), "加载 A");
        assert_eq!(t_args("{} / {}", &["A"]), "A / {}");
    }

    /// 覆盖：系统区域解析支持大小写、下划线与区域后缀；未知语言回退英语。
    /// Covers: locale resolution handles case, underscores and region suffixes; unknown falls back to English.
    #[test]
    fn locale_resolution_matches_primary_language() {
        assert_eq!(language_for_locale("zh-Hans-CN"), Some("zh-CN"));
        assert_eq!(language_for_locale("EN_us.UTF-8"), Some("en-US"));
        assert_eq!(language_for_locale("ja-JP"), None);
        assert_eq!(language_for_system(Some("ja-JP")), FALLBACK_LANGUAGE);
        assert_eq!(language_for_system(None), FALLBACK_LANGUAGE);
    }

    /// 覆盖：语言切换接受规范化代码，未知代码不改变当前语言。
    /// Covers: switching accepts normalized codes and unknown codes leave the language unchanged.
    #[test]
    fn language_switch_ignores_unknown_codes() {
        assert!(set_language("zh-CN"));
        assert!(!set_language("xx-YY"));
        assert_eq!(language(), "zh-CN");
        assert!(set_language("en-us"));
        assert_eq!(language(), "en-US");
        assert!(set_language("zh-CN"));
    }
}
