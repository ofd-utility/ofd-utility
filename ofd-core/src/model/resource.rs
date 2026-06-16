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

use crate::model::graphics::{CtColor, CtVectorG};
use crate::types::{StArray, StId, StLoc};

/// `Res`：资源文件根节点（见表 18）。
///
/// 规范中 `Res` 的内容为一个可重复、无序的选择（`xs:choice maxOccurs="unbounded"`），
/// 即各资源组（如 `MultiMedias`）允许出现多次且可与其他组交错。故这里以保留文档
/// 顺序的子节点序列 [`children`](Res::children) 建模，并提供 [`Res::fonts`] 等便捷
/// 方法跨组扁平化遍历其中的资源项。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Res {
    /// 定义此资源文件的通用数据存储路径（必选）。
    ///
    /// 资源文件中各数据文件的默认存储位置以此为基准。
    #[serde(rename = "@BaseLoc")]
    pub base_loc: StLoc,
    /// 资源组子节点，按在文档中出现的顺序保留（可重复、可交错）。
    #[serde(rename = "$value", default)]
    pub children: Vec<ResChild>,
}

/// `Res` 的一个资源组子节点（见表 18 的无序选择内容）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ResChild {
    /// 颜色空间资源组。
    ColorSpaces(ColorSpaces),
    /// 绘制参数资源组。
    DrawParams(DrawParams),
    /// 字型资源组。
    Fonts(Fonts),
    /// 多媒体资源组。
    MultiMedias(MultiMedias),
    /// 矢量图像（复合图形单元）资源组。
    CompositeGraphicUnits(CompositeGraphicUnits),
}

impl Res {
    /// 跨所有 `ColorSpaces` 组遍历颜色空间资源。
    pub fn color_spaces(&self) -> impl Iterator<Item = &CtColorSpace> {
        self.children
            .iter()
            .filter_map(|c| match c {
                ResChild::ColorSpaces(g) => Some(g),
                _ => None,
            })
            .flat_map(|g| g.color_spaces.iter())
    }

    /// 跨所有 `DrawParams` 组遍历绘制参数资源。
    pub fn draw_params(&self) -> impl Iterator<Item = &CtDrawParam> {
        self.children
            .iter()
            .filter_map(|c| match c {
                ResChild::DrawParams(g) => Some(g),
                _ => None,
            })
            .flat_map(|g| g.draw_params.iter())
    }

    /// 跨所有 `Fonts` 组遍历字型资源。
    pub fn fonts(&self) -> impl Iterator<Item = &CtFont> {
        self.children
            .iter()
            .filter_map(|c| match c {
                ResChild::Fonts(g) => Some(g),
                _ => None,
            })
            .flat_map(|g| g.fonts.iter())
    }

    /// 跨所有 `MultiMedias` 组遍历多媒体资源。
    pub fn multi_medias(&self) -> impl Iterator<Item = &CtMultiMedia> {
        self.children
            .iter()
            .filter_map(|c| match c {
                ResChild::MultiMedias(g) => Some(g),
                _ => None,
            })
            .flat_map(|g| g.multi_medias.iter())
    }

    /// 跨所有 `CompositeGraphicUnits` 组遍历矢量图像资源。
    pub fn composite_graphic_units(&self) -> impl Iterator<Item = &CtVectorG> {
        self.children
            .iter()
            .filter_map(|c| match c {
                ResChild::CompositeGraphicUnits(g) => Some(g),
                _ => None,
            })
            .flat_map(|g| g.units.iter())
    }
}

/// 一组矢量图像（复合图形单元）资源的描述（见表 18、表 49）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompositeGraphicUnits {
    /// 矢量图像列表，被页面中的复合对象（`CompositeObject`）按 `ResourceID` 引用。
    #[serde(rename = "CompositeGraphicUnit", default)]
    pub units: Vec<CtVectorG>,
}

/// 一组颜色空间资源的描述（见表 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ColorSpaces {
    /// 颜色空间列表。
    #[serde(rename = "ColorSpace", default)]
    pub color_spaces: Vec<CtColorSpace>,
}

/// `CT_ColorSpace`：颜色空间（见 8.3.1、表 28）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtColorSpace {
    /// 资源标识（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 颜色空间类型：`Gray`/`RGB`/`CMYK`（必选）。
    #[serde(rename = "@Type")]
    pub cs_type: String,
    /// 每个颜色分量的位数，取值 1/2/4/8/16，默认 8（可选）。
    #[serde(rename = "@BitsPerComponent")]
    pub bits_per_component: Option<u32>,
    /// 调色板所在文件（可选）。
    #[serde(rename = "@Profile")]
    pub profile: Option<StLoc>,
}

/// 一组绘制参数资源的描述（见表 18）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DrawParams {
    /// 绘制参数列表。
    #[serde(rename = "DrawParam", default)]
    pub draw_params: Vec<CtDrawParam>,
}

/// `CT_DrawParam`：绘制参数（见 8.2、表 24）。
///
/// 绘制参数可通过 `Relative` 继承另一绘制参数；图元未显式声明的颜色、线宽
/// 等属性将回退到其引用的绘制参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtDrawParam {
    /// 资源标识（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 基础绘制参数标识，本参数在其基础上覆盖（可选）。
    #[serde(rename = "@Relative")]
    pub relative: Option<StId>,
    /// 线宽，单位为毫米，默认 0.353（可选）。
    #[serde(rename = "@LineWidth")]
    pub line_width: Option<f64>,
    /// 线条连接样式（可选）。
    #[serde(rename = "@Join")]
    pub join: Option<String>,
    /// 线条端点样式（可选）。
    #[serde(rename = "@Cap")]
    pub cap: Option<String>,
    /// 斜接限制（可选）。
    #[serde(rename = "@MiterLimit")]
    pub miter_limit: Option<f64>,
    /// 虚线起始相位（可选）。
    #[serde(rename = "@DashOffset")]
    pub dash_offset: Option<f64>,
    /// 虚线重复样式（可选）。
    #[serde(rename = "@DashPattern")]
    pub dash_pattern: Option<StArray<f64>>,
    /// 填充颜色（可选）。
    #[serde(rename = "FillColor")]
    pub fill_color: Option<CtColor>,
    /// 勾边颜色（可选）。
    #[serde(rename = "StrokeColor")]
    pub stroke_color: Option<CtColor>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个含全部资源组类型的 `Res`，使每个迭代器都同时遇到命中分支与
    /// `_ => None` 跳过分支。
    fn full_res() -> Res {
        Res {
            base_loc: StLoc::from("Res"),
            children: vec![
                ResChild::ColorSpaces(ColorSpaces {
                    color_spaces: vec![CtColorSpace {
                        id: StId(1),
                        cs_type: "RGB".into(),
                        ..Default::default()
                    }],
                }),
                ResChild::DrawParams(DrawParams {
                    draw_params: vec![CtDrawParam {
                        id: StId(2),
                        ..Default::default()
                    }],
                }),
                ResChild::Fonts(Fonts {
                    fonts: vec![CtFont {
                        id: StId(3),
                        font_name: "宋体".into(),
                        ..Default::default()
                    }],
                }),
                ResChild::MultiMedias(MultiMedias {
                    multi_medias: vec![CtMultiMedia {
                        id: StId(4),
                        media_type: "Image".into(),
                        media_file: StLoc::from("a.png"),
                        ..Default::default()
                    }],
                }),
                ResChild::CompositeGraphicUnits(CompositeGraphicUnits {
                    units: vec![CtVectorG::default()],
                }),
            ],
        }
    }

    #[test]
    fn iterators_flatten_each_group() {
        let res = full_res();
        assert_eq!(res.color_spaces().count(), 1);
        assert_eq!(res.draw_params().count(), 1);
        assert_eq!(res.fonts().count(), 1);
        assert_eq!(res.multi_medias().count(), 1);
        assert_eq!(res.composite_graphic_units().count(), 1);
        // 命中具体项，确认扁平化取到内层元素。
        assert_eq!(res.fonts().next().unwrap().font_name, "宋体");
        assert_eq!(res.color_spaces().next().unwrap().cs_type, "RGB");
    }

    #[test]
    fn iterators_skip_when_group_absent() {
        // 仅含字型组：其余迭代器走 `_ => None` 分支返回空。
        let res = Res {
            base_loc: StLoc::default(),
            children: vec![ResChild::Fonts(Fonts::default())],
        };
        assert_eq!(res.color_spaces().count(), 0);
        assert_eq!(res.draw_params().count(), 0);
        assert_eq!(res.multi_medias().count(), 0);
        assert_eq!(res.composite_graphic_units().count(), 0);
    }
}
