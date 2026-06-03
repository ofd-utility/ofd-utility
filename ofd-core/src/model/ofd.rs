//! OFD 主入口 `OFD.xml`（见 GB/T 33190—2016 7.4）。
//!
//! `OFD.xml` 是包的主入口文件，一个包内存在且只存在一个。其结构见图 3，
//! 属性说明见表 3；文档元数据 `CT_DocInfo` 见图 4、表 4。

use serde::{Deserialize, Serialize};

use crate::model::common::Versions;
use crate::types::StLoc;

/// `OFD.xml` 的根节点（见表 3）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Ofd {
    /// 文件格式的版本号，取值为 `1.0`（必选）。
    #[serde(rename = "@Version")]
    pub version: String,
    /// 文件格式子集类型。`OFD` 表示符合本标准，`OFD-A` 表示符合 OFD 存档规范（必选）。
    #[serde(rename = "@DocType")]
    pub doc_type: String,
    /// 文件对象入口，可以存在多个，以便在一个文档中包含多个版式文档（必选）。
    #[serde(rename = "DocBody")]
    pub doc_bodies: Vec<DocBody>,
}

impl Ofd {
    /// 是否为 OFD 存档规范（`DocType == "OFD-A"`）。
    pub fn is_archive(&self) -> bool {
        self.doc_type == "OFD-A"
    }
}

/// 文件对象入口（见表 3）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocBody {
    /// 文档元数据信息描述（必选）。
    #[serde(rename = "DocInfo")]
    pub doc_info: CtDocInfo,
    /// 指向文档根节点文档（`Document.xml`），见 7.5（可选）。
    #[serde(rename = "DocRoot")]
    pub doc_root: Option<StLoc>,
    /// 版本信息，见第 19 章（可选）。
    #[serde(rename = "Versions")]
    pub versions: Option<Versions>,
    /// 指向该文档中签名和签章结构，见第 18 章（可选）。
    #[serde(rename = "Signatures")]
    pub signatures: Option<StLoc>,
}

/// `CT_DocInfo`：文档元数据（见表 4）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtDocInfo {
    /// 采用 UUID 算法生成的由 32 个字符组成的文件标识（可选）。
    #[serde(rename = "DocID")]
    pub doc_id: Option<String>,
    /// 文档标题，可与文件名不同（可选）。
    #[serde(rename = "Title")]
    pub title: Option<String>,
    /// 文档作者（可选）。
    #[serde(rename = "Author")]
    pub author: Option<String>,
    /// 文档主题（可选）。
    #[serde(rename = "Subject")]
    pub subject: Option<String>,
    /// 文档摘要与注释（可选）。
    #[serde(rename = "Abstract")]
    pub abstract_: Option<String>,
    /// 文档创建日期（可选）。
    #[serde(rename = "CreationDate")]
    pub creation_date: Option<String>,
    /// 文档最近修改日期（可选）。
    #[serde(rename = "ModDate")]
    pub mod_date: Option<String>,
    /// 文档分类：`Normal`/`EBook`/`ENewsPaper`/`EMagzine`，默认 `Normal`（可选）。
    #[serde(rename = "DocUsage")]
    pub doc_usage: Option<String>,
    /// 文档封面，指向一个图片文件（可选）。
    #[serde(rename = "Cover")]
    pub cover: Option<StLoc>,
    /// 关键词集合（可选）。
    #[serde(rename = "Keywords")]
    pub keywords: Option<Keywords>,
    /// 创建文档的应用程序（可选）。
    #[serde(rename = "Creator")]
    pub creator: Option<String>,
    /// 创建文档的应用程序的版本信息（可选）。
    #[serde(rename = "CreatorVersion")]
    pub creator_version: Option<String>,
    /// 用户自定义元数据集合（可选）。
    #[serde(rename = "CustomDatas")]
    pub custom_datas: Option<CustomDatas>,
}

/// 关键词集合，每个关键词用一个 `Keyword` 子节点表达（见表 4）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Keywords {
    /// 关键词列表。
    #[serde(rename = "Keyword", default)]
    pub keywords: Vec<String>,
}

/// 用户自定义元数据集合（见表 4）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CustomDatas {
    /// 自定义元数据列表。
    #[serde(rename = "CustomData", default)]
    pub custom_datas: Vec<CustomData>,
}

/// 用户自定义元数据，指定一个名称及其对应的值（见表 4）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CustomData {
    /// 用户自定义元数据名称（必选）。
    #[serde(rename = "@Name")]
    pub name: String,
    /// 用户自定义元数据值。
    #[serde(rename = "$text", default)]
    pub value: String,
}
