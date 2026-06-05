//! 注释（见 GB/T 33190—2016 第 15 章）。
//!
//! 注释是独立于页面内容之外、叠加绘制在页面之上的图元，最典型者为电子印章
//! （`Type="Stamp"`）的签章外观。其组织分两级：
//!
//! - 注释入口文件（`Document.xml` 中 `Annotations` 指向，见表 49）：按页列出
//!   每页注释所在的文件位置；
//! - 页注释文件 [`PageAnnot`]（见表 50）：含若干 [`Annot`]，每个注释通过
//!   [`Appearance`] 给出其外观——一组与页面图元相同的可绘制对象。
//!
//! 渲染时，注释外观叠加在页面内容与前景模板之上；`Appearance` 的 `Boundary`
//! 给出外观在页面坐标系中的位置，其内部图元对象坐标相对该边界左上角。

use serde::{Deserialize, Serialize};

use crate::model::graphics::PageBlock;
use crate::types::{StBox, StId, StLoc, StRefId};

/// 注释入口文件根节点（`Document.xml` 中 `Annotations` 指向；见表 49）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Annotations {
    /// 按页组织的注释引用，每页一个节点。
    #[serde(rename = "Page", default)]
    pub pages: Vec<AnnotPageRef>,
}

/// 某一页的注释文件引用（见表 49）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AnnotPageRef {
    /// 关联的页对象标识（必选）。
    #[serde(rename = "@PageID")]
    pub page_id: StRefId,
    /// 指向该页的页注释文件（必选）。
    #[serde(rename = "FileLoc")]
    pub file_loc: StLoc,
}

/// 页注释文件根节点（见表 50）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageAnnot {
    /// 该页上的注释列表。
    #[serde(rename = "Annot", default)]
    pub annots: Vec<Annot>,
}

/// `CT_Annot`：单个注释（见表 51）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Annot {
    /// 注释标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 注释类型，如 `Stamp`/`Link`/`Highlight` 等（可选）。
    #[serde(rename = "@Type")]
    pub annot_type: Option<String>,
    /// 是否可见，默认 `true`（可选）。
    #[serde(rename = "@Visible")]
    pub visible: Option<bool>,
    /// 注释的外观描述（可选）。
    #[serde(rename = "Appearance")]
    pub appearance: Option<Appearance>,
}

/// `CT_Appearance`：注释外观（见表 51）。
///
/// 本质是一组与页面图元相同的可绘制对象（`CT_PageBlock`），其坐标相对
/// [`boundary`](Appearance::boundary) 左上角。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Appearance {
    /// 注释外观在页面坐标系中的边界（必选）。
    #[serde(rename = "@Boundary", default)]
    pub boundary: StBox,
    /// 外观内的可绘制对象，按出现顺序绘制。
    #[serde(rename = "$value", default)]
    pub objects: Vec<PageBlock>,
}
