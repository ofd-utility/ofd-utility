//! `ofd-cli` 端到端集成测试：用 assert_cmd 驱动真实二进制，覆盖 info/tree/check/
//! render 四个子命令及其关键分支。
//!
//! 夹具为内存构造的 .ofd（ZIP）文件，写入 `CARGO_TARGET_TMPDIR`（target/ 下，
//! 不污染 /tmp）。CLI 正文经 tracing（默认 stdout）与 tree 的 println 均输出到
//! stdout，故断言统一查 stdout。

use std::io::{Cursor, Write};
use std::path::PathBuf;

use assert_cmd::Command;
use ofd_core::crypto::{base64_encode, sm3};
use predicates::str::contains;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const PAGE_CONTENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content>
    <ofd:Layer ID="100" Type="Body">
      <ofd:PathObject ID="102" Boundary="50 50 100 100" Fill="true">
        <ofd:FillColor Value="255 0 0"/>
        <ofd:AbbreviatedData>M 0 0 L 100 0 L 100 100 L 0 100 C</ofd:AbbreviatedData>
      </ofd:PathObject>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#;

const PAGE_BLANK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016"></ofd:Page>"#;

const DOCUMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData>
    <ofd:MaxUnitID>1000</ofd:MaxUnitID>
    <ofd:PageArea><ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox></ofd:PageArea>
    <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>
    <ofd:TemplatePage ID="10" BaseLoc="Tpls/Tpl_0/Content.xml"/>
  </ofd:CommonData>
  <ofd:Pages>
    <ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/>
    <ofd:Page ID="2" BaseLoc="Pages/Page_1/Content.xml"/>
  </ofd:Pages>
</ofd:Document>"#;

/// 无 PageArea 的文档（覆盖 info 的“未声明”分支）。
const DOCUMENT_NO_AREA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData>
    <ofd:MaxUnitID>10</ofd:MaxUnitID>
  </ofd:CommonData>
  <ofd:Pages><ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/></ofd:Pages>
</ofd:Document>"#;

const RES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res"/>"#;

/// 富内容主入口：含两个 DocBody（第二个无 DocRoot），覆盖 DocInfo 各字段、
/// Versions、Signatures、关键词与自定义元数据等打印分支。
fn ofd_full() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.0" DocType="OFD-A">
  <ofd:DocBody>
    <ofd:DocInfo>
      <ofd:DocID>9f3c0e1a2b3c4d5e6f7a8b9c0d1e2f30</ofd:DocID>
      <ofd:Title>测试文档</ofd:Title>
      <ofd:Author>张三</ofd:Author>
      <ofd:Creator>ofd-core</ofd:Creator>
      <ofd:CreatorVersion>0.1</ofd:CreatorVersion>
      <ofd:CreationDate>2026-06-01</ofd:CreationDate>
      <ofd:Keywords><ofd:Keyword>OFD</ofd:Keyword><ofd:Keyword>版式</ofd:Keyword></ofd:Keywords>
      <ofd:CustomDatas><ofd:CustomData Name="区域">华东</ofd:CustomData></ofd:CustomDatas>
    </ofd:DocInfo>
    <ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>
    <ofd:Versions><ofd:Version ID="9" Index="1" Current="true" BaseLoc="Doc_0/V/V.xml"/></ofd:Versions>
    <ofd:Signatures>/Doc_0/Signs/Signatures.xml</ofd:Signatures>
  </ofd:DocBody>
  <ofd:DocBody>
    <ofd:DocInfo><ofd:Title>无根</ofd:Title></ofd:DocInfo>
  </ofd:DocBody>
</ofd:OFD>"#
        .to_string()
}

/// 简单主入口：单 DocBody，可控 DocType 与 DocRoot 目标文档。
fn ofd_simple(doc_type: &str, document_path: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.0" DocType="{doc_type}">
  <ofd:DocBody>
    <ofd:DocInfo><ofd:Title>t</ofd:Title></ofd:DocInfo>
    <ofd:DocRoot>{document_path}</ofd:DocRoot>
  </ofd:DocBody>
</ofd:OFD>"#
    )
}

fn tmp_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

/// 将一组 (条目名, 内容) 写为 .ofd（ZIP）文件，返回路径。
fn write_ofd(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, content) in files {
            zip.start_file(*entry, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    let path = tmp_dir().join(name);
    std::fs::write(&path, buf).unwrap();
    path
}

/// 标准内容文件集合（页/模板/资源齐全），配合给定主入口与文档。
fn content_files<'a>(ofd_xml: &'a str, document_xml: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("OFD.xml", ofd_xml),
        ("Doc_0/Document.xml", document_xml),
        ("Doc_0/Pages/Page_0/Content.xml", PAGE_CONTENT),
        ("Doc_0/Pages/Page_1/Content.xml", PAGE_BLANK),
        ("Doc_0/Tpls/Tpl_0/Content.xml", PAGE_BLANK),
        ("Doc_0/PublicRes.xml", RES_XML),
    ]
}

fn bin() -> Command {
    Command::cargo_bin("ofd-cli").unwrap()
}

#[test]
fn info_prints_full_structure() {
    let path = write_ofd("info_full.ofd", &content_files(&ofd_full(), DOCUMENT_XML));
    bin()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("OFD 主入口"))
        .stdout(contains("存档规范")) // DocType=OFD-A
        .stdout(contains("空白页")) // 第二页 PAGE_BLANK
        .stdout(contains("华东")) // CustomData
        .stdout(contains("(无 DocRoot")); // 第二个 DocBody
}

#[test]
fn info_handles_doc_without_page_area() {
    let files = vec![
        ("OFD.xml", ofd_simple("OFD", "Doc_0/Document.xml")),
        ("Doc_0/Document.xml", DOCUMENT_NO_AREA.to_string()),
        ("Doc_0/Pages/Page_0/Content.xml", PAGE_CONTENT.to_string()),
    ];
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (*a, b.as_str())).collect();
    let path = write_ofd("info_no_area.ofd", &refs);
    bin()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("未声明"));
}

#[test]
fn info_on_invalid_file_fails() {
    let path = write_ofd("not_a_zip.ofd", &[]);
    // 写入非 ZIP 字节。
    std::fs::write(&path, b"this is not a zip").unwrap();
    bin()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(contains("失败"));
}

#[test]
fn tree_lists_entries() {
    let path = write_ofd("tree.ofd", &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML));
    bin()
        .args(["tree", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("OFD.xml"))
        .stdout(contains("个目录"))
        .stdout(contains("个文件"));
}

#[test]
fn tree_with_size_shows_bytes() {
    let path = write_ofd("tree_size.ofd", &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML));
    bin()
        .args(["tree", "--size", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains(" B)"));
}

/// 构造一份签名（可控 CheckMethod 与被保护文件引用及其 CheckValue）。
fn signed_content(check_method: &str, refs: &[(&str, &str)]) -> Vec<(String, String)> {
    let refs_xml: String = refs
        .iter()
        .map(|(file_ref, cv)| {
            format!(r#"<ofd:Reference FileRef="{file_ref}"><ofd:CheckValue>{cv}</ofd:CheckValue></ofd:Reference>"#)
        })
        .collect();
    let signature_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Signature xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:SignedInfo>
    <ofd:Provider ProviderName="test"/>
    <ofd:References CheckMethod="{check_method}">{refs_xml}</ofd:References>
  </ofd:SignedInfo>
  <ofd:SignedValue>/Doc_0/Signs/Sign_0/SignedValue.dat</ofd:SignedValue>
</ofd:Signature>"#
    );
    let signatures_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Signatures xmlns:ofd="http://www.ofdspec.org/2016"><ofd:Signature ID="1" Type="Sign" BaseLoc="/Doc_0/Signs/Sign_0/Signature.xml"/></ofd:Signatures>"#;

    vec![
        ("OFD.xml".into(), ofd_simple_signed()),
        ("Doc_0/Document.xml".into(), DOCUMENT_XML.into()),
        ("Doc_0/Pages/Page_0/Content.xml".into(), PAGE_CONTENT.into()),
        ("Doc_0/Pages/Page_1/Content.xml".into(), PAGE_BLANK.into()),
        ("Doc_0/Tpls/Tpl_0/Content.xml".into(), PAGE_BLANK.into()),
        ("Doc_0/PublicRes.xml".into(), RES_XML.into()),
        ("Doc_0/Signs/Signatures.xml".into(), signatures_xml.into()),
        ("Doc_0/Signs/Sign_0/Signature.xml".into(), signature_xml),
        ("Doc_0/Signs/Sign_0/SignedValue.dat".into(), "dummy".into()),
    ]
}

fn ofd_simple_signed() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.0" DocType="OFD">
  <ofd:DocBody>
    <ofd:DocInfo><ofd:Title>t</ofd:Title></ofd:DocInfo>
    <ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>
    <ofd:Signatures>/Doc_0/Signs/Signatures.xml</ofd:Signatures>
  </ofd:DocBody>
</ofd:OFD>"#
        .to_string()
}

fn write_signed(name: &str, files: Vec<(String, String)>) -> PathBuf {
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    write_ofd(name, &refs)
}

#[test]
fn check_passes_on_valid_signature() {
    let cv = base64_encode(&sm3(DOCUMENT_XML.as_bytes()));
    let path = write_signed(
        "check_ok.ofd",
        signed_content("1.2.156.10197.1.401", &[("/Doc_0/Document.xml", &cv)]),
    );
    bin()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("完整性校验通过"))
        .stdout(contains("[通过]"))
        .stdout(contains("均符合规范"));
}

#[test]
fn check_fails_on_tampered_signature() {
    let path = write_signed(
        "check_bad.ofd",
        signed_content("1.2.156.10197.1.401", &[("/Doc_0/Document.xml", "WRONG==")]),
    );
    bin()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(contains("被篡改"))
        .stdout(contains("[失败]"))
        .stdout(contains("不符合规范的文件"));
}

#[test]
fn check_reports_unsupported_method() {
    let path = write_signed(
        "check_unsup.ofd",
        signed_content("9.9.9.9", &[("/Doc_0/Document.xml", "x")]),
    );
    // Unverified 不算不合规 → 退出码 0。
    bin()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("无法校验"));
}

#[test]
fn check_multiple_files_mixed() {
    let cv = base64_encode(&sm3(DOCUMENT_XML.as_bytes()));
    let ok = write_signed(
        "check_multi_ok.ofd",
        signed_content("1.2.156.10197.1.401", &[("/Doc_0/Document.xml", &cv)]),
    );
    // 结构损坏：删页文件使结构校验失败。
    let broken = write_ofd(
        "check_multi_broken.ofd",
        &[
            ("OFD.xml", &ofd_simple("OFD", "Doc_0/Document.xml")),
            ("Doc_0/Document.xml", DOCUMENT_XML),
            // 缺少 Pages/Tpls/PublicRes 文件。
        ],
    );
    bin()
        .args(["check", ok.to_str().unwrap(), broken.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(contains("不符合规范的文件（1/2）"));
}

#[test]
fn cat_pretty_prints_xml() {
    let path = write_ofd(
        "cat_xml.ofd",
        &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML),
    );
    bin()
        .args(["cat", path.to_str().unwrap(), "Document\\.xml"])
        .assert()
        .success()
        .stdout(contains("==> Doc_0/Document.xml <=="))
        .stdout(contains("<ofd:Document"))
        // 重排后子节点带缩进。
        .stdout(contains("  <ofd:CommonData>"));
}

#[test]
fn cat_raw_outputs_source() {
    let path = write_ofd(
        "cat_raw.ofd",
        &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML),
    );
    bin()
        .args(["cat", path.to_str().unwrap(), "Document\\.xml", "--raw"])
        .assert()
        .success()
        // 原始源含其 4 空格缩进片段（未重排为 2 空格）。
        .stdout(contains("    <ofd:MaxUnitID>1000</ofd:MaxUnitID>"));
}

#[test]
fn cat_extracts_binary_to_cwd() {
    let ofd_xml = ofd_simple("OFD", "Doc_0/Document.xml");
    let mut files = content_files(&ofd_xml, DOCUMENT_XML);
    files.push(("Doc_0/Res/img.dat", "BINARY-PAYLOAD"));
    let path = write_ofd("cat_bin.ofd", &files);

    let dest = tmp_dir().join("img.dat");
    let _ = std::fs::remove_file(&dest);

    bin()
        .current_dir(tmp_dir())
        .args(["cat", path.to_str().unwrap(), "img\\.dat"])
        .assert()
        .success()
        .stdout(contains("已导出"));
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "BINARY-PAYLOAD");
}

#[test]
fn cat_no_match_fails() {
    let path = write_ofd(
        "cat_nomatch.ofd",
        &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML),
    );
    bin()
        .args(["cat", path.to_str().unwrap(), "nonexistent-entry"])
        .assert()
        .failure()
        .stdout(contains("失败"));
}

#[test]
fn cat_invalid_regex_fails() {
    let path = write_ofd(
        "cat_badre.ofd",
        &content_files(&ofd_simple("OFD", "Doc_0/Document.xml"), DOCUMENT_XML),
    );
    bin()
        .args(["cat", path.to_str().unwrap(), "["])
        .assert()
        .failure()
        .stdout(contains("无效的正则"));
}

#[test]
fn render_writes_images() {
    let path = write_ofd(
        "render.ofd",
        &content_files(&ofd_full(), DOCUMENT_XML),
    );
    let out = tmp_dir().join("render_out");
    let _ = std::fs::remove_dir_all(&out);

    // 默认前缀（None 分支）+ png。
    bin()
        .args(["render", path.to_str().unwrap(), out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("完成"));
    let count = std::fs::read_dir(&out).unwrap().count();
    assert!(count >= 2, "应至少渲染两页, 实得 {count}");

    // 指定前缀（Some 分支）+ jpg 格式 + 自定义 dpi。
    bin()
        .args([
            "render",
            path.to_str().unwrap(),
            out.to_str().unwrap(),
            "--prefix",
            "custom",
            "--format",
            "jpg",
            "--dpi",
            "72",
        ])
        .assert()
        .success();
    assert!(
        std::fs::read_dir(&out)
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().starts_with("custom")),
        "应存在自定义前缀输出"
    );
}
