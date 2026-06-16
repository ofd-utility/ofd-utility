//! OFD 规范符合性校验与数字签名完整性校验（见 GB/T 33190—2016 第 7、18 章）。
//!
//! 本模块在解析能力之上提供两类校验：
//!
//! 1. **结构符合性**：容器能否打开、`OFD.xml` 主入口是否完整、各版式文档的根
//!    节点/页/模板/资源能否按声明的路径找到并解析；
//! 2. **签名完整性**：依据 `Signature.xml` 的 [`References`] 重新计算每个被保护
//!    文件的摘要（国密 SM3，见 [`crate::crypto`]）并与 `CheckValue` 比对，判断
//!    签名覆盖的内容是否被篡改。
//!
//! [`check_path`] 给出面向命令行的高层入口：对单个 OFD 文件执行上述全部校验并
//! 汇总为 [`CheckReport`]，任何失败都不会以 `Err` 形式中断，而是记入报告，便于
//! 批量校验多个文件并列出不合规清单。
//!
//! 注意：本模块仅做摘要级完整性校验，**不**对签名值（SM2 签名）做密码学验签。
//!
//! # 示例
//!
//! ```no_run
//! use ofd_core::verify::check_path;
//!
//! let report = check_path("sample.ofd");
//! if report.conforms() {
//!     println!("符合规范");
//! } else {
//!     for p in &report.problems {
//!         eprintln!("问题: {p}");
//!     }
//! }
//! ```

use std::io::{Read, Seek};
use std::path::Path;

use crate::crypto::{base64_encode, sm3};
use crate::model::signature::{Signature, Signatures};
use crate::types::{StId, StLoc, parent_dir, resolve_path};
use crate::{OfdReader, Result};

/// 国密 SM3 杂凑算法的 OID（`References@CheckMethod` 取值）。
pub const OID_SM3: &str = "1.2.156.10197.1.401";

/// 摘要清单所用的校验（杂凑）方法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckMethod {
    /// 国密 SM3，可重新计算并比对。
    Sm3,
    /// 本库未实现的摘要算法，无法据此判定完整性（携带原始 OID）。
    Unsupported(String),
}

impl CheckMethod {
    /// 由 `References@CheckMethod` 的 OID 解析校验方法；缺省按 SM3 处理。
    pub fn from_oid(oid: Option<&str>) -> Self {
        match oid.map(str::trim) {
            None | Some("") | Some(OID_SM3) => CheckMethod::Sm3,
            Some(other) => CheckMethod::Unsupported(other.to_string()),
        }
    }
}

/// 单条 [`Reference`](crate::model::signature::Reference) 的校验状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefStatus {
    /// 重新计算的摘要与 `CheckValue` 一致。
    Ok,
    /// 摘要不一致，文件内容已被篡改。
    Mismatch {
        /// 签名中记录的期望摘要（Base64）。
        expected: String,
        /// 重新计算得到的实际摘要（Base64）。
        actual: String,
    },
    /// 被引用的文件在包内不存在。
    Missing,
    /// 摘要算法不受支持，无法判定。
    Unsupported,
}

/// 单个被保护文件的校验记录。
#[derive(Debug, Clone)]
pub struct RefCheck {
    /// 被保护文件在包内的路径（`FileRef` 原文）。
    pub file_ref: String,
    /// 校验状态。
    pub status: RefStatus,
}

/// 单个签名的整体判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigVerdict {
    /// 所有被保护文件摘要均一致。
    Valid,
    /// 存在被篡改或缺失的被保护文件。
    Invalid,
    /// 因摘要算法不受支持等原因无法判定。
    Unverified,
}

/// 单个签名的完整性校验报告。
#[derive(Debug, Clone)]
pub struct SignatureReport {
    /// 签名标识。
    pub id: StId,
    /// 签名类型（`Seal`/`Sign`）。
    pub sig_type: String,
    /// 签名描述文件（`Signature.xml`）在包内的路径。
    pub base_loc: String,
    /// 摘要校验方法。
    pub method: CheckMethod,
    /// 各被保护文件的校验记录。
    pub references: Vec<RefCheck>,
}

impl SignatureReport {
    /// 综合判定本签名的完整性。
    pub fn verdict(&self) -> SigVerdict {
        if matches!(self.method, CheckMethod::Unsupported(_)) {
            return SigVerdict::Unverified;
        }
        if self
            .references
            .iter()
            .any(|r| !matches!(r.status, RefStatus::Ok))
        {
            SigVerdict::Invalid
        } else {
            SigVerdict::Valid
        }
    }

    /// 返回所有校验未通过（篡改或缺失）的被保护文件记录。
    pub fn failures(&self) -> impl Iterator<Item = &RefCheck> {
        self.references
            .iter()
            .filter(|r| matches!(r.status, RefStatus::Mismatch { .. } | RefStatus::Missing))
    }
}

/// 一个 OFD 文件的完整校验报告。
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// 结构符合性问题清单（每项为一条人类可读的描述）。
    pub problems: Vec<String>,
    /// 各签名的完整性校验报告。
    pub signatures: Vec<SignatureReport>,
}

impl CheckReport {
    /// 是否符合规范：既无结构问题，也没有任何签名被判定为 [`SigVerdict::Invalid`]。
    ///
    /// 因算法不受支持而 [`SigVerdict::Unverified`] 的签名不视为不合规。
    pub fn conforms(&self) -> bool {
        self.problems.is_empty()
            && self
                .signatures
                .iter()
                .all(|s| s.verdict() != SigVerdict::Invalid)
    }
}

impl<R: Read + Seek> OfdReader<R> {
    /// 装载文档登记的全部签名（`Signatures.xml` 及其引用的各 `Signature.xml`）。
    ///
    /// 遍历所有 `DocBody`，对声明了 `Signatures` 的文档解析其签名列表与每份签名
    /// 描述。返回的 [`LoadedSignature`] 携带在包内的解析路径，供完整性校验定位。
    pub fn load_signatures(&mut self) -> Result<Vec<LoadedSignature>> {
        let sig_locs: Vec<StLoc> = self
            .ofd()
            .doc_bodies
            .iter()
            .filter_map(|b| b.signatures.clone())
            .collect();

        let mut loaded = Vec::new();
        for loc in &sig_locs {
            let list_path = resolve_path("", loc);
            let base = parent_dir(&list_path).to_string();
            let list: Signatures = self.package_mut().parse(&list_path)?;
            for sig_ref in &list.signatures {
                let path = resolve_path(&base, &sig_ref.base_loc);
                let signature: Signature = self.package_mut().parse(&path)?;
                loaded.push(LoadedSignature {
                    id: sig_ref.id,
                    sig_type: sig_ref.type_or_default().to_string(),
                    base_loc: path,
                    signature,
                });
            }
        }
        Ok(loaded)
    }

    /// 校验单个签名的完整性：逐个重算被保护文件的摘要并与 `CheckValue` 比对。
    pub fn verify_signature(&mut self, loaded: &LoadedSignature) -> SignatureReport {
        let refs = &loaded.signature.signed_info.references;
        let method = CheckMethod::from_oid(refs.check_method.as_deref());

        let mut checks = Vec::with_capacity(refs.references.len());
        for reference in &refs.references {
            let file_ref = reference.file_ref.to_string();
            let status = if method != CheckMethod::Sm3 {
                RefStatus::Unsupported
            } else {
                let path = resolve_path("", &reference.file_ref);
                match self.package_mut().read(&path) {
                    Err(_) => RefStatus::Missing,
                    Ok(bytes) => {
                        let actual = base64_encode(&sm3(&bytes));
                        if actual == reference.check_value.trim() {
                            RefStatus::Ok
                        } else {
                            RefStatus::Mismatch {
                                expected: reference.check_value.trim().to_string(),
                                actual,
                            }
                        }
                    }
                }
            };
            checks.push(RefCheck { file_ref, status });
        }

        SignatureReport {
            id: loaded.id,
            sig_type: loaded.sig_type.clone(),
            base_loc: loaded.base_loc.clone(),
            method,
            references: checks,
        }
    }

    /// 校验文档全部签名的完整性，返回逐个签名的报告。
    pub fn verify_signatures(&mut self) -> Result<Vec<SignatureReport>> {
        let loaded = self.load_signatures()?;
        Ok(loaded.iter().map(|l| self.verify_signature(l)).collect())
    }
}

/// 已装载的单个签名及其在包内的解析路径。
#[derive(Debug, Clone)]
pub struct LoadedSignature {
    /// 签名标识。
    pub id: StId,
    /// 签名类型（`Seal`/`Sign`）。
    pub sig_type: String,
    /// `Signature.xml` 在包内的解析路径。
    pub base_loc: String,
    /// 解析后的签名描述。
    pub signature: Signature,
}

/// 对已打开的 OFD 读取器执行结构符合性 + 签名完整性校验。
///
/// 与 [`check_path`] 的区别在于此函数针对已打开的 [`OfdReader`]（如内存中的包），
/// 因此不包含“容器能否打开/主入口能否解析”这一步。
pub fn check_reader<R: Read + Seek>(reader: &mut OfdReader<R>) -> CheckReport {
    let mut report = CheckReport::default();
    check_structure(reader, &mut report);

    match reader.verify_signatures() {
        Ok(sigs) => report.signatures = sigs,
        Err(e) => report.problems.push(format!("签名装载失败: {e}")),
    }
    report
}

/// 对文件系统中的一个 OFD 文件执行完整校验。
///
/// 任何失败（无法打开、主入口缺失、引用文件解析失败、签名被篡改等）都会记入
/// 返回的 [`CheckReport`]，本函数不会返回 `Err`，便于批量校验。
pub fn check_path<P: AsRef<Path>>(path: P) -> CheckReport {
    let mut report = CheckReport::default();
    let mut reader = match OfdReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            report
                .problems
                .push(format!("无法打开或解析 OFD 主入口: {e}"));
            return report;
        }
    };
    let sub = check_reader(&mut reader);
    report.problems.extend(sub.problems);
    report.signatures = sub.signatures;
    report
}

/// 检查主入口、各版式文档及其页/模板/资源能否按声明定位并解析。
fn check_structure<R: Read + Seek>(reader: &mut OfdReader<R>, report: &mut CheckReport) {
    let ofd = reader.ofd();
    if ofd.version.trim().is_empty() {
        report
            .problems
            .push("OFD.xml 缺少 Version 属性".to_string());
    }
    if ofd.doc_type != "OFD" && ofd.doc_type != "OFD-A" {
        report
            .problems
            .push(format!("OFD.xml 的 DocType 非法: {:?}", ofd.doc_type));
    }
    if ofd.doc_bodies.is_empty() {
        report.problems.push("OFD.xml 不含任何 DocBody".to_string());
    }

    let bodies = ofd.doc_bodies.clone();
    for (i, body) in bodies.iter().enumerate() {
        let Some(root) = &body.doc_root else {
            report
                .problems
                .push(format!("DocBody #{i} 缺少 DocRoot，无法定位文档根节点"));
            continue;
        };

        let doc = match reader.load_document(body) {
            Ok(d) => d,
            Err(e) => {
                report
                    .problems
                    .push(format!("DocBody #{i} 文档根节点 {root} 解析失败: {e}"));
                continue;
            }
        };

        // 页对象逐页定位与解析。
        let pages: Vec<_> = doc.pages().to_vec();
        for page in &pages {
            if let Err(e) = reader.load_page(&doc, page) {
                report.problems.push(format!(
                    "DocBody #{i} 页 {} ({}) 解析失败: {e}",
                    page.id, page.base_loc
                ));
            }
        }

        // 模板页。
        let templates: Vec<_> = doc.template_pages().to_vec();
        for tpl in &templates {
            if let Err(e) = reader.load_template(&doc, tpl) {
                report.problems.push(format!(
                    "DocBody #{i} 模板页 {} 解析失败: {e}",
                    tpl.base_loc
                ));
            }
        }

        // 公共资源与文档资源。
        let resources: Vec<_> = doc
            .public_res()
            .iter()
            .chain(doc.document_res())
            .cloned()
            .collect();
        for loc in &resources {
            if let Err(e) = reader.load_resource(&doc, loc) {
                report
                    .problems
                    .push(format!("DocBody #{i} 资源 {loc} 解析失败: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_check(status: RefStatus) -> RefCheck {
        RefCheck {
            file_ref: "Doc_0/Document.xml".to_string(),
            status,
        }
    }

    fn report_with(method: CheckMethod, statuses: Vec<RefStatus>) -> SignatureReport {
        SignatureReport {
            id: StId(1),
            sig_type: "Seal".to_string(),
            base_loc: "Doc_0/Signs/Sign_0/Signature.xml".to_string(),
            method,
            references: statuses.into_iter().map(ref_check).collect(),
        }
    }

    #[test]
    fn check_method_from_oid_branches() {
        assert_eq!(CheckMethod::from_oid(None), CheckMethod::Sm3);
        assert_eq!(CheckMethod::from_oid(Some("")), CheckMethod::Sm3);
        assert_eq!(CheckMethod::from_oid(Some("  ")), CheckMethod::Sm3);
        assert_eq!(CheckMethod::from_oid(Some(OID_SM3)), CheckMethod::Sm3);
        assert_eq!(
            CheckMethod::from_oid(Some("1.2.840.113549")),
            CheckMethod::Unsupported("1.2.840.113549".to_string())
        );
    }

    #[test]
    fn verdict_unverified_when_method_unsupported() {
        let r = report_with(
            CheckMethod::Unsupported("x".into()),
            vec![RefStatus::Ok],
        );
        assert_eq!(r.verdict(), SigVerdict::Unverified);
    }

    #[test]
    fn verdict_valid_and_invalid() {
        let valid = report_with(CheckMethod::Sm3, vec![RefStatus::Ok, RefStatus::Ok]);
        assert_eq!(valid.verdict(), SigVerdict::Valid);
        assert_eq!(valid.failures().count(), 0);

        let invalid = report_with(
            CheckMethod::Sm3,
            vec![
                RefStatus::Ok,
                RefStatus::Mismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                RefStatus::Missing,
                RefStatus::Unsupported,
            ],
        );
        assert_eq!(invalid.verdict(), SigVerdict::Invalid);
        // failures 仅收 Mismatch 与 Missing。
        assert_eq!(invalid.failures().count(), 2);
    }

    #[test]
    fn check_report_conforms() {
        // 无问题、无 Invalid 签名 → 符合。
        let mut report = CheckReport::default();
        report
            .signatures
            .push(report_with(CheckMethod::Sm3, vec![RefStatus::Ok]));
        // Unverified 不算不合规。
        report
            .signatures
            .push(report_with(CheckMethod::Unsupported("x".into()), vec![RefStatus::Ok]));
        assert!(report.conforms());

        // 有结构问题 → 不符合。
        report.problems.push("boom".into());
        assert!(!report.conforms());

        // 有 Invalid 签名 → 不符合。
        let mut report2 = CheckReport::default();
        report2
            .signatures
            .push(report_with(CheckMethod::Sm3, vec![RefStatus::Missing]));
        assert!(!report2.conforms());
    }
}
