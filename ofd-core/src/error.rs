//! 错误类型定义。

use thiserror::Error;

/// 解析 OFD 文档过程中可能出现的错误。
#[derive(Debug, Error)]
pub enum OfdError {
    /// 底层 IO 错误。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP 容器读取错误（见第 6 章 容器方案）。
    #[error("zip container error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// XML 反序列化错误。
    #[error("xml parse error: {0}")]
    Xml(#[from] quick_xml::DeError),

    /// 基础数据类型解析失败（见 7.3）。
    #[error("invalid {ty} value {value:?}: {reason}")]
    BasicType {
        /// 数据类型名称，如 `ST_Box`。
        ty: &'static str,
        /// 原始字符串值。
        value: String,
        /// 失败原因。
        reason: String,
    },

    /// 包内缺少必需的文件。
    #[error("entry not found in package: {0}")]
    EntryNotFound(String),

    /// 文档结构不符合规范要求。
    #[error("invalid OFD structure: {0}")]
    Structure(String),
}

/// 本 crate 的统一 `Result` 别名。
pub type Result<T> = std::result::Result<T, OfdError>;
