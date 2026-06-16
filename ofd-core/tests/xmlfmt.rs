//! `xmlfmt::pretty_xml` 的行为测试：紧凑 XML 重排、声明保留、非法输入报错。

use ofd_core::pretty_xml;

#[test]
fn compact_xml_is_reindented() {
    let input = r#"<a><b>x</b><c/></a>"#;
    let out = pretty_xml(input).unwrap();
    // 重排后应为多行，且子节点带 2 空格缩进。
    assert!(out.contains('\n'), "输出应为多行: {out}");
    assert!(out.contains("  <b>x</b>"), "子节点应缩进 2 空格: {out}");
    assert!(out.contains("<a>") && out.contains("</a>"));
}

#[test]
fn declaration_is_preserved() {
    let input = r#"<?xml version="1.0" encoding="UTF-8"?><root><child>v</child></root>"#;
    let out = pretty_xml(input).unwrap();
    assert!(out.contains("<?xml"), "应保留 XML 声明: {out}");
    assert!(out.contains("  <child>v</child>"));
}

#[test]
fn malformed_xml_errors() {
    // 未闭合标签 → 读事件报错分支。
    assert!(pretty_xml("<a><b></a>").is_err());
}
