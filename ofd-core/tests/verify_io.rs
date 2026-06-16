//! verify 模块的 I/O 路径覆盖：签名完整性各状态、结构校验各错误分支、
//! `check_path` 文件入口，以及 `OfdReader::open` 文件打开。
//!
//! 测试用内存 ZIP 构包；`check_path` 需要真实文件，写入 `CARGO_TARGET_TMPDIR`
//! （位于 target/ 下，避免污染 /tmp）。

use std::io::{Cursor, Write};

use ofd_core::crypto::{base64_encode, sm3};
use ofd_core::verify::{CheckMethod, RefStatus, SigVerdict, check_path, check_reader};
use ofd_core::{OfdPackage, OfdReader};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

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
  </ofd:Pages>
</ofd:Document>"#;

const PAGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016"></ofd:Page>"#;

const RES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res"/>"#;

/// 构造一个 OFD.xml：可控 Version / DocType / 是否含 DocRoot / 是否含 Signatures。
fn ofd_xml(version: &str, doc_type: &str, doc_root: bool, signatures: Option<&str>) -> String {
    let root = if doc_root {
        "<ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>"
    } else {
        ""
    };
    let sigs = signatures
        .map(|s| format!("<ofd:Signatures>{s}</ofd:Signatures>"))
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="{version}" DocType="{doc_type}">
  <ofd:DocBody>
    <ofd:DocInfo><ofd:Title>t</ofd:Title></ofd:DocInfo>
    {root}
    {sigs}
  </ofd:DocBody>
</ofd:OFD>"#
    )
}

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn reader_of(files: &[(&str, &str)]) -> OfdReader<Cursor<Vec<u8>>> {
    OfdReader::new(OfdPackage::new(Cursor::new(zip_of(files))).unwrap()).unwrap()
}

/// 一套结构完整、可解析的基础文件集合（无签名）。
fn base_files() -> Vec<(&'static str, String)> {
    vec![
        ("OFD.xml", ofd_xml("1.0", "OFD", true, None)),
        ("Doc_0/Document.xml", DOCUMENT_XML.to_string()),
        ("Doc_0/Pages/Page_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/Tpls/Tpl_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/PublicRes.xml", RES_XML.to_string()),
    ]
}

fn as_refs<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .map(|(n, c)| (*n as &str, c.as_str()))
        .collect()
}

#[test]
fn structure_ok_has_no_problems() {
    let files = base_files();
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert!(report.conforms());
}

#[test]
fn structure_flags_bad_version_and_doctype() {
    let mut files = base_files();
    files[0].1 = ofd_xml("  ", "WEIRD", true, None);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.iter().any(|p| p.contains("Version")));
    assert!(report.problems.iter().any(|p| p.contains("DocType")));
}

#[test]
fn structure_flags_missing_doc_root() {
    let mut files = base_files();
    files[0].1 = ofd_xml("1.0", "OFD", false, None);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.iter().any(|p| p.contains("DocRoot")));
}

#[test]
fn structure_flags_unparseable_document() {
    let mut files = base_files();
    files[1].1 = "<not-a-document".to_string();
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.iter().any(|p| p.contains("文档根节点")));
}

#[test]
fn structure_flags_missing_page_template_resource() {
    // 删除页/模板/资源文件，使三类装载分别失败。
    let files: Vec<(&str, String)> = vec![
        ("OFD.xml", ofd_xml("1.0", "OFD", true, None)),
        ("Doc_0/Document.xml", DOCUMENT_XML.to_string()),
        // 不写 Pages/Tpls/PublicRes 对应文件。
    ];
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.iter().any(|p| p.contains("页 1")));
    assert!(report.problems.iter().any(|p| p.contains("模板页")));
    assert!(report.problems.iter().any(|p| p.contains("资源")));
}

/// 构造一份签名描述（可控 CheckMethod 与被保护文件引用），并返回相关文件三元组。
fn signed_files(check_method: &str, refs: &[(&str, &str)]) -> Vec<(&'static str, String)> {
    let refs_xml: String = refs
        .iter()
        .map(|(file_ref, cv)| {
            format!(
                r#"<ofd:Reference FileRef="{file_ref}"><ofd:CheckValue>{cv}</ofd:CheckValue></ofd:Reference>"#
            )
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
<ofd:Signatures xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Signature ID="1" Type="Sign" BaseLoc="/Doc_0/Signs/Sign_0/Signature.xml"/>
</ofd:Signatures>"#;

    let mut files = vec![
        (
            "OFD.xml",
            ofd_xml("1.0", "OFD", true, Some("/Doc_0/Signs/Signatures.xml")),
        ),
        ("Doc_0/Document.xml", DOCUMENT_XML.to_string()),
        ("Doc_0/Pages/Page_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/Tpls/Tpl_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/PublicRes.xml", RES_XML.to_string()),
        ("Doc_0/Signs/Signatures.xml", signatures_xml.to_string()),
        ("Doc_0/Signs/Sign_0/Signature.xml", signature_xml),
        ("Doc_0/Signs/Sign_0/SignedValue.dat", "dummy".to_string()),
    ];
    files.retain(|_| true);
    files
}

#[test]
fn signature_ref_ok() {
    let cv = base64_encode(&sm3(DOCUMENT_XML.as_bytes()));
    let files = signed_files("1.2.156.10197.1.401", &[("/Doc_0/Document.xml", &cv)]);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert_eq!(report.signatures[0].method, CheckMethod::Sm3);
    assert_eq!(report.signatures[0].verdict(), SigVerdict::Valid);
    assert!(matches!(
        report.signatures[0].references[0].status,
        RefStatus::Ok
    ));
}

#[test]
fn signature_ref_mismatch() {
    let files = signed_files("1.2.156.10197.1.401", &[("/Doc_0/Document.xml", "WRONG==")]);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert_eq!(report.signatures[0].verdict(), SigVerdict::Invalid);
    assert!(matches!(
        report.signatures[0].references[0].status,
        RefStatus::Mismatch { .. }
    ));
}

#[test]
fn signature_ref_missing_file() {
    let files = signed_files("1.2.156.10197.1.401", &[("/Doc_0/DoesNotExist.xml", "x")]);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(matches!(
        report.signatures[0].references[0].status,
        RefStatus::Missing
    ));
    assert_eq!(report.signatures[0].verdict(), SigVerdict::Invalid);
}

#[test]
fn signature_unsupported_method() {
    let files = signed_files("9.9.9.9", &[("/Doc_0/Document.xml", "x")]);
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(matches!(
        report.signatures[0].method,
        CheckMethod::Unsupported(_)
    ));
    assert!(matches!(
        report.signatures[0].references[0].status,
        RefStatus::Unsupported
    ));
    assert_eq!(report.signatures[0].verdict(), SigVerdict::Unverified);
    // Unverified 不影响整体合规。
    assert!(report.conforms());
}

#[test]
fn check_reader_reports_signature_load_failure() {
    // 声明了 Signatures，但 Signatures.xml 引用的 Signature.xml 不存在 → 装载失败。
    let files: Vec<(&str, String)> = vec![
        (
            "OFD.xml",
            ofd_xml("1.0", "OFD", true, Some("/Doc_0/Signs/Signatures.xml")),
        ),
        ("Doc_0/Document.xml", DOCUMENT_XML.to_string()),
        ("Doc_0/Pages/Page_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/Tpls/Tpl_0/Content.xml", PAGE_XML.to_string()),
        ("Doc_0/PublicRes.xml", RES_XML.to_string()),
        (
            "Doc_0/Signs/Signatures.xml",
            r#"<ofd:Signatures xmlns:ofd="http://www.ofdspec.org/2016"><ofd:Signature ID="1" BaseLoc="/Doc_0/Signs/Sign_0/Signature.xml"/></ofd:Signatures>"#.to_string(),
        ),
        // 缺 Signature.xml。
    ];
    let mut reader = reader_of(&as_refs(&files));
    let report = check_reader(&mut reader);
    assert!(report.problems.iter().any(|p| p.contains("签名装载失败")));
}

#[test]
fn check_path_on_real_file_and_open_failure() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let path = format!("{dir}/sample_ok.ofd");
    let files = base_files();
    std::fs::write(&path, zip_of(&as_refs(&files))).unwrap();

    // 真实文件路径入口。
    let report = check_path(&path);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert!(report.conforms());

    // OfdReader::open 成功路径。
    let opened = OfdReader::open(&path).unwrap();
    assert_eq!(opened.ofd().version, "1.0");

    // 打开不存在的文件 → 记入 problems，不 panic。
    let bad = check_path(format!("{dir}/no_such_file.ofd"));
    assert!(bad.problems.iter().any(|p| p.contains("无法打开")));
    assert!(!bad.conforms());
}
