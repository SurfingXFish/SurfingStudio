//! 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树
//! 文件用途：
//! - 读取 SoundBug `comp.dat` 解压后的 `EDIT` 树。
//! - 提供节点、值类型与二进制读取器。
//!
//! 函数清单：
//! - `parse_edit`：解析 `EDIT` 根节点。
//! - `Node::prop` / `prop_str` / `prop_f64` / `prop_bool` / `prop_u64` / `child`：节点查询。
//! - `Value::as_str` / `as_f64` / `as_bool` / `as_u64`：值读取。
//! - `Reader::new` / `read_bytes` / `read_u8` / `read_u32_le` / `read_cstr` / `read_f64_le` / `read_count` / `read_value` / `read_node`：二进制读取。
//! - `decode_value`：按类型标签解码属性负载。
//! - `trim_nul`：移除字节串尾部 NUL。
//!
//! Module: core / project runtime / SoundBug backend / EDIT tree
//! Purpose:
//! - Read the `EDIT` tree from decompressed `comp.dat`.
//! - Provide nodes, values and a binary reader.
//!
//! Functions:
//! - `parse_edit`: parse the `EDIT` root.
//! - `Node::prop` / `prop_str` / `prop_f64` / `prop_bool` / `prop_u64` / `child`: node queries.
//! - `Value::as_str` / `as_f64` / `as_bool` / `as_u64`: value accessors.
//! - `Reader::new` / `read_bytes` / `read_u8` / `read_u32_le` / `read_cstr` / `read_f64_le` / `read_count` / `read_value` / `read_node`: binary reads.
//! - `decode_value`: decode a property payload by type tag.
//! - `trim_nul`: trim trailing NUL bytes.

/// EDIT 树的最大递归深度。
/// Maximum recursion depth of the EDIT tree.
const MAX_NODE_DEPTH: usize = 128;

/// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树
/// 功能：解析解压后的 EDIT 树根节点。
/// 输入：
/// - `data`：解压后的完整字节。
///
/// 输出/副作用：返回 `EDIT` 根节点；要求所有字节都被消费。
///
/// 复杂度：时间 O(字节数)，空间 O(节点数)。
///
/// Module: core / project runtime / SoundBug backend / EDIT tree
/// Purpose: Parse the root of the decompressed EDIT tree.
/// Inputs:
/// - `data`: the full decompressed byte stream.
///
/// Outputs / side effects: returns the `EDIT` root node; all bytes must be consumed.
///
/// Complexity: O(bytes) time and O(node count) space.
pub(super) fn parse_edit(data: &[u8]) -> Result<Node, String> {
    let mut reader = Reader::new(data);
    let root_name = reader.read_cstr()?;
    if root_name != "EDIT" {
        return Err(format!("不支持的 SoundBug 工程根节点: {root_name}"));
    }
    let root = reader.read_node(root_name)?;
    if reader.pos != data.len() {
        return Err(format!(
            "SoundBug 工程数据尾部存在未解析内容（偏移 {:#X}）",
            reader.pos
        ));
    }
    Ok(root)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树
/// 功能：移除字节串末尾的 NUL。
/// 输入：`value` 原始字节。
///
/// 输出/副作用：返回去掉尾部 NUL 的切片。
///
/// 复杂度：时间 O(尾部 NUL 数)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / EDIT tree
/// Purpose: Trim trailing NUL bytes.
/// Inputs: `value` — raw bytes.
///
/// Outputs / side effects: returns the slice without trailing NULs.
///
/// Complexity: O(trailing NUL count) time and O(1) space.
pub(super) fn trim_nul(value: &[u8]) -> Vec<u8> {
    let mut end = value.len();
    while end > 0 && value[end - 1] == 0 {
        end -= 1;
    }
    value[..end].to_vec()
}

/// EDIT 树节点：名称 + 属性 + 子节点。
/// An EDIT tree node: name, properties and children.
pub(super) struct Node {
    pub(super) name: String,
    pub(super) props: Vec<(String, Value)>,
    pub(super) children: Vec<Node>,
}

impl Node {
    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取第一个属性。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回属性值引用；不修改状态。
    ///
    /// 复杂度：时间 O(属性数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Look up the first property by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns a value reference.
    ///
    /// Complexity: O(property count) time and O(1) space.
    pub(super) fn prop(&self, key: &str) -> Option<&Value> {
        self.props
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取字符串属性。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回字符串；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(属性数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Read a string property by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns the string, or `None` on a type mismatch.
    ///
    /// Complexity: O(property count) time and O(1) space.
    pub(super) fn prop_str(&self, key: &str) -> Option<String> {
        self.prop(key).and_then(Value::as_str)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取数值属性。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回 `f64`；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(属性数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Read a numeric property by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns `f64`, or `None` on a type mismatch.
    ///
    /// Complexity: O(property count) time and O(1) space.
    pub(super) fn prop_f64(&self, key: &str) -> Option<f64> {
        self.prop(key).and_then(Value::as_f64)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取布尔属性。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回布尔值；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(属性数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Read a boolean property by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns the boolean, or `None` on a type mismatch.
    ///
    /// Complexity: O(property count) time and O(1) space.
    pub(super) fn prop_bool(&self, key: &str) -> Option<bool> {
        self.prop(key).and_then(Value::as_bool)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取无符号整数属性。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回 `u64`；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(属性数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Read an unsigned integer property by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns `u64`, or `None` on a type mismatch.
    ///
    /// Complexity: O(property count) time and O(1) space.
    pub(super) fn prop_u64(&self, key: &str) -> Option<u64> {
        self.prop(key).and_then(Value::as_u64)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取二进制属性（插件状态块）。
    /// 输入：`key` 属性名。
    ///
    /// 输出/副作用：返回字节副本；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(字节长度)，空间 O(字节长度)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Read a binary property (plugin state blob) by name.
    /// Inputs: `key` — property name.
    ///
    /// Outputs / side effects: returns a byte copy, or `None` on a type mismatch.
    ///
    /// Complexity: O(blob length) time and space.
    pub(super) fn prop_bytes(&self, key: &str) -> Option<Vec<u8>> {
        match self.prop(key) {
            Some(Value::Bytes(value)) => Some(value.clone()),
            _ => None,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 节点查询
    /// 功能：按名称取第一个子节点。
    /// 输入：`name` 子节点名。
    ///
    /// 输出/副作用：返回子节点引用；不修改状态。
    ///
    /// 复杂度：时间 O(子节点数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / node query
    /// Purpose: Look up the first child by name.
    /// Inputs: `name` — child name.
    ///
    /// Outputs / side effects: returns a child reference.
    ///
    /// Complexity: O(child count) time and O(1) space.
    pub(super) fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }
}

/// EDIT 树中的值。
/// A value inside the EDIT tree.
pub(super) enum Value {
    Str(String),
    Float(f64),
    Int(i32),
    Bool(bool),
    /// 插件的二进制状态块。
    /// Binary plugin state blob.
    Bytes(Vec<u8>),
}

impl Value {
    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 值转换
    /// 功能：按字符串读取。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回字符串副本；类型不符时为 `None`。
    ///
    /// 复杂度：时间 O(字符串长度)，空间 O(字符串长度)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / value conversion
    /// Purpose: Read the value as a string.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns a string copy, or `None` on a type mismatch.
    ///
    /// Complexity: O(string length) time and space.
    fn as_str(&self) -> Option<String> {
        match self {
            Value::Str(value) => Some(value.clone()),
            Value::Int(value) => Some(value.to_string()),
            Value::Float(value) => Some(value.to_string()),
            Value::Bool(value) => Some(if *value { "1" } else { "0" }.to_string()),
            Value::Bytes(_) => None,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 值转换
    /// 功能：按数值读取，兼容字符串形式。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 `f64`；无法解析时为 `None`。
    ///
    /// 复杂度：时间 O(字符串长度)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / value conversion
    /// Purpose: Read the value as a number, accepting string forms.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns `f64`, or `None` when unparsable.
    ///
    /// Complexity: O(string length) time and O(1) space.
    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(value) => Some(*value),
            Value::Int(value) => Some(*value as f64),
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Str(value) => value.trim().parse::<f64>().ok(),
            Value::Bytes(_) => None,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 值转换
    /// 功能：按布尔读取，兼容 "0" / "1" 字符串。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回布尔值；无法解析时为 `None`。
    ///
    /// 复杂度：时间 O(字符串长度)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / value conversion
    /// Purpose: Read the value as a boolean, accepting "0" / "1" strings.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the boolean, or `None` when unparsable.
    ///
    /// Complexity: O(string length) time and O(1) space.
    fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            Value::Int(value) => Some(*value != 0),
            Value::Float(value) => Some(*value != 0.0),
            Value::Str(value) => match value.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" | "" => Some(false),
                other => other.parse::<f64>().ok().map(|number| number != 0.0),
            },
            Value::Bytes(_) => None,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 值转换
    /// 功能：按无符号整数读取。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 `u64`；负数或无法解析时为 `None`。
    ///
    /// 复杂度：时间 O(字符串长度)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / value conversion
    /// Purpose: Read the value as an unsigned integer.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns `u64`, or `None` for negatives/unparsable values.
    ///
    /// Complexity: O(string length) time and O(1) space.
    fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Int(value) if *value >= 0 => Some(*value as u64),
            Value::Float(value) if *value >= 0.0 => Some(*value as u64),
            Value::Str(value) => value.trim().parse::<u64>().ok(),
            _ => None,
        }
    }
}

/// EDIT 树的二进制读取器。
/// Binary reader for the EDIT tree.
pub(super) struct Reader<'a> {
    pub(super) data: &'a [u8],
    pub(super) pos: usize,
    depth: usize,
}

impl<'a> Reader<'a> {
    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：在字节切片上创建读取器。
    /// 输入：`data` 完整字节。
    ///
    /// 输出/副作用：返回读取器。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Create a reader over a byte slice.
    /// Inputs: `data` — full byte stream.
    ///
    /// Outputs / side effects: returns the reader.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            depth: 0,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取指定长度的字节。
    /// 输入：`length` 字节数。
    ///
    /// 输出/副作用：返回切片；越界时报错。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read a fixed number of bytes.
    /// Inputs: `length` — byte count.
    ///
    /// Outputs / side effects: returns a slice; errors on out-of-bounds.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(length)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| format!("SoundBug 工程数据在偏移 {:#X} 处提前结束", self.pos))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个 u8。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回字节；越界时报错。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read one u8.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the byte; errors on out-of-bounds.
    ///
    /// Complexity: O(1) time and space.
    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_bytes(1)?[0])
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个小端 u32。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回数值；越界时报错。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read a little-endian u32.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the value; errors on out-of-bounds.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn read_u32_le(&mut self) -> Result<u32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个 NUL 结尾的字符串。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 UTF-8（非法字节用替换字符）；缺少 NUL 时报错。
    ///
    /// 复杂度：时间 O(字符串长度)，空间 O(字符串长度)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read a NUL-terminated string.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns UTF-8 (invalid bytes replaced); errors when the
    /// terminator is missing.
    ///
    /// Complexity: O(string length) time and space.
    pub(super) fn read_cstr(&mut self) -> Result<String, String> {
        let relative = self.data[self.pos..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| format!("SoundBug 字符串在偏移 {:#X} 处缺少终止符", self.pos))?;
        let end = self.pos + relative;
        let text = String::from_utf8_lossy(&self.data[self.pos..end]).to_string();
        self.pos = end + 1;
        Ok(text)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个小端 f64。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回数值；越界时报错。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read a little-endian f64.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the value; errors on out-of-bounds.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn read_f64_le(&mut self) -> Result<f64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取属性/子节点的数量前缀。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回数量；编码为 `00` / `01 u8` / `02 u16` / `03 u32`。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read the count prefix used by properties and children.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the count; encodings are `00` / `01 u8` /
    /// `02 u16` / `03 u32`.
    ///
    /// Complexity: O(1) time and space.
    fn read_count(&mut self) -> Result<usize, String> {
        let tag = self.read_u8()?;
        let count = match tag {
            0 => 0,
            1 => self.read_u8()? as usize,
            2 => {
                let bytes = self.read_bytes(2)?;
                u16::from_le_bytes([bytes[0], bytes[1]]) as usize
            }
            3 => self.read_u32_le()? as usize,
            other => {
                return Err(format!(
                    "SoundBug 数量前缀无效（{other:#X}，偏移 {:#X}）",
                    self.pos - 1
                ))
            }
        };
        if count > self.data.len() {
            return Err(format!("SoundBug 数量异常：{count}"));
        }
        Ok(count)
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个属性值。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 `Value`；未知标签时报错。
    ///
    /// 复杂度：时间 O(值长度)，空间 O(值长度)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read one property value.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns a `Value`; errors on unknown tags.
    ///
    /// Complexity: O(value length) time and space.
    fn read_value(&mut self) -> Result<Value, String> {
        match self.read_u8()? {
            1 => {
                let size = self.read_u8()? as usize;
                if size == 0 {
                    return Err("SoundBug 属性长度无效".into());
                }
                let typ = self.read_u8()?;
                let payload = self.read_bytes(size - 1)?.to_vec();
                Ok(decode_value(typ, payload))
            }
            2 => {
                let bytes = self.read_bytes(2)?;
                let size = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
                if size == 0 {
                    return Err("SoundBug 属性长度无效".into());
                }
                let typ = self.read_u8()?;
                let payload = self.read_bytes(size - 1)?.to_vec();
                Ok(decode_value(typ, payload))
            }
            3 => {
                // 插件状态块使用 NUL 结尾的二进制；保留内容供导入使用。
                // Plugin state uses a NUL-terminated binary blob; keep it for import.
                let start = self.pos;
                let relative = self.data[start..]
                    .iter()
                    .position(|byte| *byte == 0)
                    .ok_or_else(|| format!("SoundBug 状态块在偏移 {start:#X} 处缺少终止符"))?;
                let end = start + relative;
                self.pos = end + 1;
                Ok(Value::Bytes(self.data[start..end].to_vec()))
            }
            tag => Err(format!(
                "SoundBug 属性标签无效（{tag:#X}，偏移 {:#X}）",
                self.pos - 1
            )),
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 读取器
    /// 功能：读取一个节点（属性表 + 子节点表）。
    /// 输入：`name` 已读取的节点名。
    ///
    /// 输出/副作用：返回节点；递归深度超限时报错。
    ///
    /// 复杂度：时间 O(节点内容)，空间 O(节点内容)。
    ///
    /// Module: core / project runtime / SoundBug backend / EDIT tree / reader
    /// Purpose: Read a node (property table + child table).
    /// Inputs: `name` — the already-read node name.
    ///
    /// Outputs / side effects: returns the node; errors when recursion is too deep.
    ///
    /// Complexity: O(node content) time and space.
    fn read_node(&mut self, name: String) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > MAX_NODE_DEPTH {
            return Err("SoundBug 工程嵌套层级过深".into());
        }
        let property_count = self.read_count()?;
        let mut props = Vec::with_capacity(property_count.min(1024));
        for _ in 0..property_count {
            let key = self.read_cstr()?;
            let value = self.read_value()?;
            props.push((key, value));
        }
        let child_count = self.read_count()?;
        let mut children = Vec::with_capacity(child_count.min(4096));
        for _ in 0..child_count {
            let child_name = self.read_cstr()?;
            children.push(self.read_node(child_name)?);
        }
        self.depth -= 1;
        Ok(Node {
            name,
            props,
            children,
        })
    }
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / EDIT 树 / 值解码
/// 功能：按类型标签解码属性负载。
/// 输入：
/// - `typ`：类型标签。
/// - `payload`：负载字节。
///
/// 输出/副作用：返回 `Value`。
///
/// 复杂度：时间 O(负载长度)，空间 O(负载长度)。
///
/// Module: core / project runtime / SoundBug backend / EDIT tree / value decoding
/// Purpose: Decode a property payload by its type tag.
/// Inputs:
/// - `typ`: type tag.
/// - `payload`: payload bytes.
///
/// Outputs / side effects: returns a `Value`.
///
/// Complexity: O(payload length) time and space.
fn decode_value(typ: u8, payload: Vec<u8>) -> Value {
    match typ {
        1 | 3 if payload.len() == 4 => Value::Int(i32::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3],
        ])),
        1 if payload.len() == 1 => Value::Int(payload[0] as i32),
        2 => Value::Bool(payload.iter().any(|byte| *byte != 0)),
        4 if payload.len() == 8 => Value::Float(f64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7],
        ])),
        5 => {
            let text = String::from_utf8_lossy(&trim_nul(&payload)).to_string();
            Value::Str(text)
        }
        _ => Value::Bytes(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：值类型的字符串/数值/布尔兼容读取。
    /// Covers: string/number/boolean compatibility of value accessors.
    #[test]
    fn value_accessors_are_lenient() {
        assert_eq!(Value::Str("1".into()).as_bool(), Some(true));
        assert_eq!(Value::Str("0".into()).as_bool(), Some(false));
        assert_eq!(Value::Str("124.0".into()).as_f64(), Some(124.0));
        assert_eq!(Value::Int(-3).as_u64(), None);
    }
}
