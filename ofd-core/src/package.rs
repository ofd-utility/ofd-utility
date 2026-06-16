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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    /// 构造一个含若干条目的内存 ZIP。
    fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, content) in files {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    fn pkg(files: &[(&str, &[u8])]) -> OfdPackage<Cursor<Vec<u8>>> {
        OfdPackage::new(Cursor::new(build_zip(files))).unwrap()
    }

    #[test]
    fn normalize_strips_leading_slash() {
        assert_eq!(normalize("/OFD.xml"), "OFD.xml");
        assert_eq!(normalize("Doc_0/Document.xml"), "Doc_0/Document.xml");
    }

    #[test]
    fn read_hit_and_miss() {
        let mut p = pkg(&[("OFD.xml", b"hello")]);
        // 前导 `/` 归一化后命中。
        assert_eq!(p.read("/OFD.xml").unwrap(), b"hello");
        // 未找到条目走 EntryNotFound 分支。
        let err = p.read("missing.xml").unwrap_err();
        assert!(matches!(err, OfdError::EntryNotFound(_)));
    }

    #[test]
    fn read_to_string_utf8_and_invalid() {
        let mut p = pkg(&[("a.txt", "文本".as_bytes()), ("bad.bin", &[0xff, 0xfe])]);
        assert_eq!(p.read_to_string("a.txt").unwrap(), "文本");
        // 非法 UTF-8 走 Structure 错误分支。
        assert!(matches!(
            p.read_to_string("bad.bin").unwrap_err(),
            OfdError::Structure(_)
        ));
    }

    #[test]
    fn parse_ok_and_xml_error() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Doc {
            #[serde(rename = "@v")]
            v: String,
        }
        let mut p = pkg(&[("ok.xml", br#"<Doc v="1"/>"#), ("bad.xml", b"<Doc")]);
        assert_eq!(p.parse::<Doc>("ok.xml").unwrap(), Doc { v: "1".into() });
        // XML 解析失败分支。
        assert!(p.parse::<Doc>("bad.xml").is_err());
    }

    #[test]
    fn contains_entries_len_empty() {
        let p = pkg(&[("OFD.xml", b"x"), ("Doc_0/Document.xml", b"y")]);
        assert!(p.contains("/OFD.xml"));
        assert!(!p.contains("nope.xml"));
        let mut entries = p.entries();
        entries.sort();
        assert_eq!(entries, vec!["Doc_0/Document.xml", "OFD.xml"]);
        assert_eq!(p.len(), 2);
        assert!(!p.is_empty());
        assert!(pkg(&[]).is_empty());
    }

    #[test]
    fn open_missing_file_errors() {
        // File::open 失败走 `?` 的 Io 错误分支。
        assert!(OfdPackage::open("/nonexistent/path/does-not-exist.ofd").is_err());
    }

    #[test]
    fn new_rejects_non_zip() {
        assert!(OfdPackage::new(Cursor::new(b"not a zip".to_vec())).is_err());
    }
}
