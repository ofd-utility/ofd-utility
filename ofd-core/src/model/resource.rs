//! 资源文件（见 GB/T 33190—2016 7.9）。
//!
//! 资源是绘制图元时所需数据的集合，其索引信息保存在资源文件中（图 20、表 18）。
//! 资源分公共资源与页资源：公共资源文件在文档根节点中指定，页资源文件在页对象
//! 中指定。
//!
//! 字型 `CT_Font`（见第 11 章）与多媒体 `CT_MultiMedia`（见表 19）在基础结构中
//! 较常用，故一并建模；颜色空间、绘制参数、矢量图像等图元细节属于页面描述章节，
//! 此处暂以分组占位，未建模的内容在反序列化时会被忽略。

use serde::{Deserialize, Serialize};

use crate::types::{StId, StLoc};

/// `Res`：资源文件根节点（见表 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Res {
    /// 定义此资源文件的通用数据存储路径（必选）。
    ///
    /// 资源文件中各数据文件的默认存储位置以此为基准。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: StLoc,
    /// 字型资源组（可选）。
    #[serde(rename = "Fonts")]
    pub fonts: Option<Fonts>,
    /// 多媒体资源组（可选）。
    #[serde(rename = "MultiMedias")]
    pub multi_medias: Option<MultiMedias>,
}

/// 一组字型资源的描述（见表 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Fonts {
    /// 字型列表。
    #[serde(rename = "Font", default)]
    pub fonts: Vec<CtFont>,
}

/// `CT_Font`：字型资源描述（见第 11 章；在基础类型上扩展 `ID`）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtFont {
    /// 资源标识（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 字型名（必选）。
    #[serde(rename = "@FontName")]
    pub font_name: String,
    /// 字型族名（可选）。
    #[serde(rename = "@FamilyName")]
    pub family_name: Option<String>,
    /// 字型适用的字符分类（可选）。
    #[serde(rename = "@Charset")]
    pub charset: Option<String>,
    /// 是否为斜体（可选）。
    #[serde(rename = "@Italic")]
    pub italic: Option<bool>,
    /// 是否为粗体（可选）。
    #[serde(rename = "@Bold")]
    pub bold: Option<bool>,
    /// 是否为衬线字体（可选）。
    #[serde(rename = "@Serif")]
    pub serif: Option<bool>,
    /// 是否为等宽字体（可选）。
    #[serde(rename = "@FixedWidth")]
    pub fixed_width: Option<bool>,
    /// 指向内嵌字型文件（可选）。
    #[serde(rename = "FontFile")]
    pub font_file: Option<StLoc>,
}

/// 一组多媒体资源的描述（见表 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MultiMedias {
    /// 多媒体列表。
    #[serde(rename = "MultiMedia", default)]
    pub multi_medias: Vec<CtMultiMedia>,
}

/// `CT_MultiMedia`：多媒体资源描述（见表 19；在基础类型上扩展 `ID`）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtMultiMedia {
    /// 资源标识（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 多媒体类型：位图图像、视频、音频（必选）。
    #[serde(rename = "@Type")]
    pub media_type: String,
    /// 资源格式，如 `BMP`/`JPEG`/`PNG`/`TIFF`/`AVS` 等（可选）。
    #[serde(rename = "@Format")]
    pub format: Option<String>,
    /// 指向包内的多媒体文件位置（必选）。
    #[serde(rename = "MediaFile")]
    pub media_file: StLoc,
}
