//! 页树与页对象（见 GB/T 33190—2016 7.6、7.7）。
//!
//! - 页树 `Pages` 见图 12、表 11：通过前序遍历叶节点确定页顺序；
//! - 页对象（`Content.xml` 的根节点 `Page`）见图 13、表 12；
//! - 模板页 `CT_TemplatePage` 见图 14、表 13；
//! - 图层 `CT_Layer` 见图 15、表 14。
//!
//! 图层内部的页面图元对象（文字、图形、图像、复合对象，见第 8~13 章）
//! 不属于基础结构，未在此建模，反序列化时会被忽略。

use serde::{Deserialize, Serialize};

use crate::model::common::Actions;
use crate::model::document::CtPageArea;
use crate::model::graphics::PageBlock;
use crate::types::{StId, StLoc, StRefId};

/// 页树（见表 11）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Pages {
    /// 页节点列表，页顺序即此处的出现顺序（必选）。
    #[serde(rename = "Page", default)]
    pub pages: Vec<PageRef>,
}

/// 页树中的页节点（见表 11）。
///
/// 注意：这是页树中对页的*引用*，仅含标识与指向页内容的路径；
/// 真正的页内容是 [`PageObject`]。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageRef {
    /// 声明该页的标识，不能与已有标识重复（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 指向页对象描述文件（必选）。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: StLoc,
}

/// 页对象，即页内容描述文件（`Content.xml`）的根节点（见表 12）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageObject {
    /// 该页所使用的模板页，可使用多个（可选）。
    #[serde(rename = "Template", default)]
    pub templates: Vec<Template>,
    /// 定义该页页面区域的大小和位置，仅对该页有效（可选）。
    #[serde(rename = "Area")]
    pub area: Option<CtPageArea>,
    /// 页资源，指向该页使用的资源文件（可选）。
    #[serde(rename = "PageRes", default)]
    pub page_res: Vec<StLoc>,
    /// 页面内容描述；该节点不存在时表示空白页（可选）。
    #[serde(rename = "Content")]
    pub content: Option<Content>,
    /// 与页面关联的动作序列（可选）。
    #[serde(rename = "Actions")]
    pub actions: Option<Actions>,
}

impl PageObject {
    /// 是否为空白页（无 `Content` 节点）。
    pub fn is_blank(&self) -> bool {
        self.content.is_none()
    }
}

/// 页面对模板页的引用（见表 12）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Template {
    /// 引用文档公共数据中定义的模板页标识（必选）。
    #[serde(rename = "@TemplateID")]
    pub template_id: StRefId,
    /// 控制模板在页面中的呈现顺序，默认 `Background`（可选）。
    #[serde(rename = "@ZOrder")]
    pub z_order: Option<String>,
}

/// 页面内容描述（见表 12）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Content {
    /// 层节点，一页可包含一个或多个层（必选）。
    #[serde(rename = "Layer", default)]
    pub layers: Vec<CtLayer>,
}

/// `CT_Layer`：图层（见表 14、表 15）。
///
/// 图层内部的图元对象（文字/图形/图像/复合对象，见 [`PageBlock`]）属于页面
/// 描述（第 8 章起）；它们按出现顺序保存在 [`objects`](CtLayer::objects) 中，
/// 供 [`crate::render`] 渲染器按叠放次序绘制。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtLayer {
    /// 图层标识。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 层类型，取值 `Body`/`Foreground`/`Background`，默认 `Body`（可选）。
    #[serde(rename = "@Type")]
    pub layer_type: Option<String>,
    /// 图层的绘制参数，引用资源文件中定义的绘制参数标识（可选）。
    #[serde(rename = "@DrawParam")]
    pub draw_param: Option<StRefId>,
    /// 图层内的页面图元对象，按文档顺序排列。
    #[serde(rename = "$value", default)]
    pub objects: Vec<PageBlock>,
}

/// `CT_TemplatePage`：模板页（见表 13）。
///
/// 模板页内容结构和普通页相同，其内容文件同样反序列化为 [`PageObject`]。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtTemplatePage {
    /// 模板页的标识，不能与已有标识重复（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 指向模板页内容描述文件（必选）。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: StLoc,
    /// 模板页名称（可选）。
    #[serde(rename = "@Name")]
    pub name: Option<String>,
    /// 模板页的默认图层类型/呈现顺序，默认 `Background`（可选）。
    #[serde(rename = "@ZOrder")]
    pub z_order: Option<String>,
}
