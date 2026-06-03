//! 多处引用的公共结构。
//!
//! 动作（`CT_Action`，见第 14 章）、目标位置（`CT_Dest`，见表 54）、
//! 版本（见第 19 章）等在基础结构中被多个节点引用，这里给出便于解析的
//! 简化模型。详细语义可在后续章节实现中逐步完善（未建模的子节点在反序列
//! 化时会被忽略）。

use serde::{Deserialize, Serialize};

use crate::types::{StId, StLoc, StRefId};

/// 动作序列。当存在多个 `Action` 对象时，所有动作依次执行。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Actions {
    /// 动作列表。
    #[serde(rename = "Action", default)]
    pub actions: Vec<CtAction>,
}

/// `CT_Action`：文档/页面/大纲等关联的动作（见第 14 章，此处为基础建模）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtAction {
    /// 事件类型，如 `DO`（文档打开）、`PO`（页面打开），见表 52。
    #[serde(rename = "@Event")]
    pub event: Option<String>,
    /// 动作类型。
    #[serde(rename = "@Type")]
    pub action_type: Option<String>,
    /// 目标位置。
    #[serde(rename = "Dest")]
    pub dest: Option<CtDest>,
}

/// `CT_Dest`：文档内的目标位置（见表 54，此处为基础建模）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtDest {
    /// 目标类型，如 `XYZ`、`Fit`、`FitH`、`FitV`、`FitR` 等。
    #[serde(rename = "@Type")]
    pub dest_type: Option<String>,
    /// 引用的页面标识。
    #[serde(rename = "@PageID")]
    pub page_id: Option<StRefId>,
    /// 左边界坐标。
    #[serde(rename = "@Left")]
    pub left: Option<f64>,
    /// 上边界坐标。
    #[serde(rename = "@Top")]
    pub top: Option<f64>,
    /// 右边界坐标。
    #[serde(rename = "@Right")]
    pub right: Option<f64>,
    /// 下边界坐标。
    #[serde(rename = "@Bottom")]
    pub bottom: Option<f64>,
    /// 缩放比例。
    #[serde(rename = "@Zoom")]
    pub zoom: Option<f64>,
}

/// 版本集合，定义文件因注释和其他改动产生的版本信息（见第 19 章）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Versions {
    /// 版本列表。
    #[serde(rename = "Version", default)]
    pub versions: Vec<Version>,
}

/// 单个版本描述节点（见第 19 章，此处为基础建模）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Version {
    /// 版本标识。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 版本序号。
    #[serde(rename = "@Index")]
    pub index: Option<i64>,
    /// 是否为当前版本。
    #[serde(rename = "@Current")]
    pub current: Option<bool>,
    /// 指向版本描述文件。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: Option<StLoc>,
}
