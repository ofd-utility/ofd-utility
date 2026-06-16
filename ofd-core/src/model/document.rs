//! 文档根节点 `Document.xml`（见 GB/T 33190—2016 7.5）。
//!
//! 文档根节点结构见图 5、属性见表 5；文档公共数据 `CT_CommonData` 见图 6、
//! 表 6；页面区域 `CT_PageArea` 见图 7、表 7；权限声明、视图首选项、书签、
//! 大纲等也在本章定义。

use serde::{Deserialize, Serialize};

use crate::model::common::Actions;
use crate::model::common::CtDest;
use crate::model::page::{CtTemplatePage, Pages};
use crate::types::{StBox, StId, StLoc, StRefId};

/// 文档根节点（见表 5）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Document {
    /// 文档公共数据，定义页面区域、公共资源等（必选）。
    #[serde(rename = "CommonData")]
    pub common_data: CommonData,
    /// 页树，见 7.6（必选）。
    #[serde(rename = "Pages")]
    pub pages: Pages,
    /// 大纲，见 7.8（可选）。
    #[serde(rename = "Outlines")]
    pub outlines: Option<Outlines>,
    /// 文档权限声明（可选）。
    #[serde(rename = "Permissions")]
    pub permissions: Option<CtPermission>,
    /// 文档关联的动作序列（可选）。
    #[serde(rename = "Actions")]
    pub actions: Option<Actions>,
    /// 文档视图首选项（可选）。
    #[serde(rename = "VPreferences")]
    pub v_preferences: Option<VPreferences>,
    /// 文档书签集（可选）。
    #[serde(rename = "Bookmarks")]
    pub bookmarks: Option<Bookmarks>,
    /// 指向附件列表文件，见第 20 章（可选）。
    #[serde(rename = "Attachments")]
    pub attachments: Option<StLoc>,
    /// 指向注释列表文件，见第 15 章（可选）。
    #[serde(rename = "Annotations")]
    pub annotations: Option<StLoc>,
    /// 指向自定义标引列表文件，见第 16 章（可选）。
    #[serde(rename = "CustomTags")]
    pub custom_tags: Option<StLoc>,
    /// 指向扩展列表文件，见第 17 章（可选）。
    #[serde(rename = "Extensions")]
    pub extensions: Option<StLoc>,
}

/// `CT_CommonData`：文档公共数据（见表 6）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommonData {
    /// 当前文档中所有对象使用标识的最大值，初始值为 0（必选）。
    #[serde(rename = "MaxUnitID")]
    pub max_unit_id: StId,
    /// 指定该文档页面区域的默认大小和位置。
    ///
    /// GB/T 33190—2016 表 6 将其列为必选，但部分开票系统生成的发票 OFD 省略了
    /// 文档级 `PageArea`，转而仅在每页的 `Page/Area`（[`crate::PageObject::area`]）
    /// 声明页面区域。为兼容此类文件，这里放宽为可选；缺省时应回退到本页 `Area`，
    /// 仍缺省则用 A4（见 [`crate::types::StBox::A4_MM`]）。
    #[serde(rename = "PageArea")]
    pub page_area: Option<CtPageArea>,
    /// 公共资源序列，每个节点指向包内的一个资源描述文档（可选）。
    #[serde(rename = "PublicRes", default)]
    pub public_res: Vec<StLoc>,
    /// 文档资源序列，每个节点指向包内的一个资源描述文档（可选）。
    #[serde(rename = "DocumentRes", default)]
    pub document_res: Vec<StLoc>,
    /// 模板页序列（可选）。
    #[serde(rename = "TemplatePage", default)]
    pub template_pages: Vec<CtTemplatePage>,
    /// 引用资源文件中定义的默认颜色空间标识，缺省采用 RGB（可选）。
    #[serde(rename = "DefaultCS")]
    pub default_cs: Option<StRefId>,
}

/// `CT_PageArea`：页面区域（见表 7、图 8）。
///
/// 四个边界框逐层嵌套：出血框 ⊃ 物理框 ⊃ 显示框 ⊃ 版心框。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CtPageArea {
    /// 页面物理区域，左上角为页面空间坐标系原点（必选）。
    #[serde(rename = "PhysicalBox")]
    pub physical_box: StBox,
    /// 显示区域，页面内容实际显示或打印输出的区域（可选）。
    #[serde(rename = "ApplicationBox")]
    pub application_box: Option<StBox>,
    /// 版心区域，即文件的正文区域（可选）。
    #[serde(rename = "ContentBox")]
    pub content_box: Option<StBox>,
    /// 出血区域，超出设备性能限制的额外出血区域；缺省为物理区域（可选）。
    #[serde(rename = "BleedBox")]
    pub bleed_box: Option<StBox>,
}

/// `CT_Permission`：文档权限声明（见表 8）。
///
/// 各布尔项缺省值均为 `true`；此处以 [`Option`] 表达“是否显式声明”，
/// 取值请配合 [`CtPermission::get`] 风格的默认逻辑使用。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtPermission {
    /// 是否允许编辑，默认 `true`。
    #[serde(rename = "Edit")]
    pub edit: Option<bool>,
    /// 是否允许添加或修改标注，默认 `true`。
    #[serde(rename = "Annot")]
    pub annot: Option<bool>,
    /// 是否允许导出，默认 `true`。
    #[serde(rename = "Export")]
    pub export: Option<bool>,
    /// 是否允许进行数字签名，默认 `true`。
    #[serde(rename = "Signature")]
    pub signature: Option<bool>,
    /// 是否允许添加水印，默认 `true`。
    #[serde(rename = "Watermark")]
    pub watermark: Option<bool>,
    /// 是否允许截屏，默认 `true`。
    #[serde(rename = "PrintScreen")]
    pub print_screen: Option<bool>,
    /// 打印权限设置（可选）。
    #[serde(rename = "Print")]
    pub print: Option<Print>,
    /// 有效期（可选）。
    #[serde(rename = "ValidPeriod")]
    pub valid_period: Option<ValidPeriod>,
}

/// 打印权限（见表 8）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Print {
    /// 文档是否允许被打印，默认 `true`。
    #[serde(rename = "@Printable")]
    pub printable: Option<bool>,
    /// 打印份数；`<0` 不受限、`0` 不允许、`>0` 为实际份数。
    #[serde(rename = "@Copies")]
    pub copies: Option<i64>,
}

/// 文档有效期（见表 8）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ValidPeriod {
    /// 有效期开始日期。
    #[serde(rename = "@StartDate")]
    pub start_date: Option<String>,
    /// 有效期结束日期。
    #[serde(rename = "@EndDate")]
    pub end_date: Option<String>,
}

/// `CT_VPreferences`：文档视图首选项（见表 9）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VPreferences {
    /// 窗口模式，默认 `None`。
    #[serde(rename = "PageMode")]
    pub page_mode: Option<String>,
    /// 页面布局模式，默认 `OneColumn`。
    #[serde(rename = "PageLayout")]
    pub page_layout: Option<String>,
    /// 标题栏显示模式，默认 `FileName`。
    #[serde(rename = "TabDisplay")]
    pub tab_display: Option<String>,
    /// 是否隐藏工具栏，默认 `false`。
    #[serde(rename = "HideToolbar")]
    pub hide_toolbar: Option<bool>,
    /// 是否隐藏菜单栏，默认 `false`。
    #[serde(rename = "HideMenubar")]
    pub hide_menubar: Option<bool>,
    /// 是否隐藏主窗口之外的其他窗体组件，默认 `false`。
    #[serde(rename = "HideWindowUI")]
    pub hide_window_ui: Option<bool>,
    /// 自动缩放模式，默认 `Default`。
    #[serde(rename = "ZoomMode")]
    pub zoom_mode: Option<String>,
    /// 文档的缩放率。
    #[serde(rename = "Zoom")]
    pub zoom: Option<f64>,
}

/// 书签集，包含一组书签（见表 5、表 10）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Bookmarks {
    /// 书签列表。
    #[serde(rename = "Bookmark", default)]
    pub bookmarks: Vec<CtBookmark>,
}

/// `CT_Bookmark`：文档书签（见表 10）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtBookmark {
    /// 书签名称（必选）。
    #[serde(rename = "@Name")]
    pub name: String,
    /// 书签对应的文档位置（必选）。
    #[serde(rename = "Dest")]
    pub dest: Option<CtDest>,
}

/// 大纲根节点，按树状结构组织（见 7.8、图 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Outlines {
    /// 顶层大纲节点。
    #[serde(rename = "OutlineElem", default)]
    pub outline_elems: Vec<CtOutlineElem>,
}

/// `CT_OutlineElem`：大纲节点（见表 17）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtOutlineElem {
    /// 大纲节点标题（必选）。
    #[serde(rename = "@Title")]
    pub title: String,
    /// 该节点下所有叶节点的数目参考值，默认 0（可选）。
    #[serde(rename = "@Count")]
    pub count: Option<i64>,
    /// 初始状态下是否展开子节点，默认 `true`（可选）。
    #[serde(rename = "@Expanded")]
    pub expanded: Option<bool>,
    /// 节点被激活时执行的动作序列（可选）。
    #[serde(rename = "Actions")]
    pub actions: Option<Actions>,
    /// 子大纲节点，层层嵌套形成树状结构（可选）。
    #[serde(rename = "OutlineElem", default)]
    pub children: Vec<CtOutlineElem>,
}

#[cfg(test)]
mod tests {
    use super::CommonData;

    /// `PublicRes` / `DocumentRes` are repeatable sequences (表 6). Some 开票系统
    /// emit them interleaved (`PublicRes … DocumentRes … PublicRes`). Without
    /// quick-xml's `overlapped-lists` feature this fails with
    /// `duplicate field PublicRes`; with it the non-consecutive repeats fold
    /// into the `Vec` as intended.
    #[test]
    fn interleaved_public_res_collects_into_vec() {
        let xml = r#"<ofd:CommonData xmlns:ofd="http://www.ofdspec.org/2016">
            <ofd:MaxUnitID>10</ofd:MaxUnitID>
            <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>
            <ofd:DocumentRes>DocumentRes.xml</ofd:DocumentRes>
            <ofd:PublicRes>PublicRes_2.xml</ofd:PublicRes>
        </ofd:CommonData>"#;

        let cd: CommonData = quick_xml::de::from_str(xml).expect("parse CommonData");
        assert_eq!(
            cd.public_res
                .iter()
                .map(|l| l.0.as_str())
                .collect::<Vec<_>>(),
            ["PublicRes.xml", "PublicRes_2.xml"],
        );
        assert_eq!(cd.document_res.len(), 1);
    }
}
