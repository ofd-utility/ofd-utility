//! 数字签名与签章结构（见 GB/T 33190—2016 第 18 章）。
//!
//! 签名相关部件分两层：
//! - 签名列表 `Signatures.xml`（[`Signatures`]，见表 80）：由 `DocBody/Signatures`
//!   指向，登记包内所有签名，每个 [`SignatureRef`] 通过 `BaseLoc` 指向一份签名描述
//!   文件；
//! - 单个签名描述 `Signature.xml`（[`Signature`] / `CT_Signature`，见表 81）：包含
//!   被保护文件摘要清单 [`SignedInfo`] 与签名值文件 `SignedValue` 的位置。
//!
//! 完整性校验依据 [`SignedInfo`] 的 [`References`]：逐个重新计算 `FileRef` 所指
//! 文件的摘要并与 [`Reference::check_value`] 比对，验证签名覆盖的内容是否被篡改。
//! 具体校验逻辑见 [`crate::verify`]。

use serde::{Deserialize, Serialize};

use crate::types::{StBox, StId, StLoc};

/// 签名列表 `Signatures.xml` 的根节点（见表 80）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Signatures {
    /// 当前文档中签名的最大标识，新增签名应在此基础上递增（可选）。
    #[serde(rename = "MaxSignId")]
    pub max_sign_id: Option<String>,
    /// 签名描述入口列表，每个指向一份 `Signature.xml`（必选，可多个）。
    #[serde(rename = "Signature", default)]
    pub signatures: Vec<SignatureRef>,
}

/// 签名列表中的一条签名入口（见表 80）。
///
/// 这是签名列表对单个签名的*引用*；真正的签名描述是 [`Signature`]。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SignatureRef {
    /// 签名标识，文档内唯一（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 签名类型：`Seal`（电子印章，默认）或 `Sign`（纯签名）（可选）。
    #[serde(rename = "@Type")]
    pub sig_type: Option<String>,
    /// 指向签名描述文件 `Signature.xml`（必选）。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: StLoc,
}

impl SignatureRef {
    /// 签名类型，缺省按规范取 `Seal`。
    pub fn type_or_default(&self) -> &str {
        self.sig_type.as_deref().unwrap_or("Seal")
    }
}

/// 单份签名描述 `Signature.xml` 的根节点（`CT_Signature`，见表 81）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Signature {
    /// 签名要保护的原文及摘要信息（必选）。
    #[serde(rename = "SignedInfo")]
    pub signed_info: SignedInfo,
    /// 指向签名值文件（`SignedValue.dat`，承载 SM2 签名/SES 签章数据）（必选）。
    #[serde(rename = "SignedValue")]
    pub signed_value: StLoc,
}

/// `SignedInfo`：签名原文信息（见表 82）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignedInfo {
    /// 创建签名时所用的签名引擎提供者信息（必选）。
    #[serde(rename = "Provider")]
    pub provider: Provider,
    /// 签名方法，使用算法的 OID（如 SM2 为 `1.2.156.10197.1.501`）（可选）。
    #[serde(rename = "SignatureMethod")]
    pub signature_method: Option<String>,
    /// 签名时间（可选）。
    #[serde(rename = "SignatureDateTime")]
    pub signature_date_time: Option<String>,
    /// 包内文件摘要清单，签名所保护的内容（必选）。
    #[serde(rename = "References")]
    pub references: References,
    /// 签章外观（电子印章在页面上的呈现位置），可多个（可选）。
    #[serde(rename = "StampAnnot", default)]
    pub stamp_annots: Vec<StampAnnot>,
    /// 电子印章信息（可选）。
    #[serde(rename = "Seal")]
    pub seal: Option<Seal>,
}

/// `Provider`：签名引擎提供者（见表 83）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Provider {
    /// 创建签名的引擎提供者名称（必选）。
    #[serde(rename = "@ProviderName")]
    pub provider_name: String,
    /// 提供者版本（可选）。
    #[serde(rename = "@Version")]
    pub version: Option<String>,
    /// 提供者厂商（可选）。
    #[serde(rename = "@Company")]
    pub company: Option<String>,
}

/// `References`：被保护文件的摘要清单（见表 84）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct References {
    /// 摘要（杂凑）算法的 OID，缺省由签名实现约定；国密 SM3 为
    /// `1.2.156.10197.1.401`（可选）。
    #[serde(rename = "@CheckMethod")]
    pub check_method: Option<String>,
    /// 摘要记录列表。
    #[serde(rename = "Reference", default)]
    pub references: Vec<Reference>,
}

/// `Reference`：单个被保护文件的摘要记录（见表 84）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reference {
    /// 指向包内被保护文件的路径（必选）。
    #[serde(rename = "@FileRef")]
    pub file_ref: StLoc,
    /// 对应文件的摘要值，按 `CheckMethod` 计算后做 Base64 编码（必选）。
    #[serde(rename = "CheckValue", default)]
    pub check_value: String,
}

/// `StampAnnot`：签章在页面上的外观标注（见表 85）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StampAnnot {
    /// 签章标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 引用的页面标识（可选）。
    #[serde(rename = "@PageRef")]
    pub page_ref: Option<String>,
    /// 签章在页面中的外观边界（可选）。
    #[serde(rename = "@Boundary")]
    pub boundary: Option<StBox>,
    /// 签章的裁剪区域（可选）。
    #[serde(rename = "@Clip")]
    pub clip: Option<StBox>,
}

/// `Seal`：电子印章，指向独立的印章数据文件（见表 81）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Seal {
    /// 指向电子印章文件（可选）。
    #[serde(rename = "BaseLoc")]
    pub base_loc: Option<StLoc>,
}
