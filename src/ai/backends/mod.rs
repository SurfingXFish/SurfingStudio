//! 所属模块：ai / 后端注册
//! 文件用途：
//! - 声明并注册全部 AI 协议后端，新协议只需在此追加。
//!
//! 函数清单：
//! - `standard`：返回按默认顺序排列的后端列表。
//!
//! Module: ai / backend registry
//! Purpose:
//! - Declare and register every AI protocol backend; a new protocol is appended here only.
//!
//! Functions:
//! - `standard`: return the backend list in default order.

mod openai_compat;

use super::AiBackend;

/// 所属模块：ai / 后端注册
/// 功能：构造标准后端列表；注册顺序即协议查找顺序。
/// 输入：无。
/// 输出/副作用：后端列表；不访问网络。
/// 复杂度：时间 O(后端数)，空间 O(后端数)。
///
/// Module: ai / backend registry
/// Purpose: Build the standard backend list; registration order is the protocol lookup order.
/// Inputs: none.
/// Outputs / side effects: backend list; no network access.
/// Complexity: O(backends) time and space.
pub(super) fn standard() -> Vec<Box<dyn AiBackend>> {
    vec![Box::new(openai_compat::OpenAiCompatible)]
}
