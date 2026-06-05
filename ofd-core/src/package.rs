//! OFD 容器方案（见 GB/T 33190—2016 第 6 章）。
//!
//! 容器功能由一个 ZIP 文件实现，包内各部件以 XML 文件形式组织。
//! [`OfdPackage`] 封装底层 ZIP 归档，提供按路径读取字节、读取文本以及
//! 直接反序列化为数据模型的能力。

use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use serde::de::DeserializeOwned;
use zip::ZipArchive;

use crate::error::{OfdError, Result};

/// OFD 包，封装一个 ZIP 归档。
///
/// 范型参数 `R` 为底层数据源，需实现 [`Read`] + [`Seek`]，
/// 常见为 [`std::fs::File`] 或 [`std::io::Cursor`]。
pub struct OfdPackage<R> {
    archive: ZipArchive<R>,
}

impl OfdPackage<File> {
    /// 从文件系统路径打开一个 OFD 包。
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::new(File::open(path)?)
    }
}

impl<R: Read + Seek> OfdPackage<R> {
    /// 基于任意 [`Read`] + [`Seek`] 数据源创建 OFD 包。
    pub fn new(reader: R) -> Result<Self> {
        Ok(OfdPackage {
            archive: ZipArchive::new(reader)?,
        })
    }

    /// 读取包内指定条目的原始字节。
    ///
    /// `path` 允许以 `/` 开头，内部会归一化为 ZIP 条目名。
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let name = normalize(path);
        let mut entry = self
            .archive
            .by_name(&name)
            .map_err(|_| OfdError::EntryNotFound(name.clone()))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// 读取包内指定条目为 UTF-8 文本。
    pub fn read_to_string(&mut self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes)
            .map_err(|e| OfdError::Structure(format!("entry {path} is not valid UTF-8: {e}")))
    }

    /// 读取并将包内 XML 条目反序列化为数据模型 `T`。
    pub fn parse<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        let text = self.read_to_string(path)?;
        Ok(quick_xml::de::from_str(&text)?)
    }

    /// 判断包内是否存在指定条目。
    pub fn contains(&self, path: &str) -> bool {
        let name = normalize(path);
        self.archive.file_names().any(|n| n == name)
    }

    /// 返回包内所有条目名。
    pub fn entries(&self) -> Vec<String> {
        self.archive.file_names().map(|s| s.to_string()).collect()
    }

    /// 包内条目数量。
    pub fn len(&self) -> usize {
        self.archive.len()
    }

    /// 包是否为空。
    pub fn is_empty(&self) -> bool {
        self.archive.is_empty()
    }
}

/// 归一化条目名：去除前导 `/`。
fn normalize(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}
