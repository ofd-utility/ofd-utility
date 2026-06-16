//! XML 美化（重排格式）。
//!
//! OFD 包内各部件多以紧凑（无缩进、可能单行）的 XML 存储，调试查看不便。
//! [`pretty_xml`] 复用 `quick-xml` 的事件流读写，将任意 XML 重排为 2 空格缩进的
//! 多行形式，保留声明、注释、CDATA 等节点。

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::Event;

use crate::error::{OfdError, Result};

/// 将 XML 文本重排为带缩进的多行形式。
///
/// 逐事件读入并回写：先去除原有文本节点首尾空白，再由带缩进的写入器统一排版。
/// 输入非法（如标签未闭合）时返回 [`OfdError::Structure`]。
pub fn pretty_xml(input: &str) -> Result<String> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);

    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => writer
                .write_event(event)
                .map_err(|e| OfdError::Structure(format!("xml format error: {e}")))?,
            Err(e) => return Err(OfdError::Structure(format!("xml format error: {e}"))),
        }
    }

    // 写入器内容源自合法 UTF-8 的 `&str` 事件流，故必为合法 UTF-8（无损转换）。
    Ok(String::from_utf8_lossy(&writer.into_inner()).into_owned())
}
