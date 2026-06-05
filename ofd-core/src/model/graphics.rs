//! 页面图元对象（见 GB/T 33190—2016 第 8~13 章）。
//!
//! 基础结构（[`crate::model::page`]）只关心页/层的骨架，而图层内部真正承载
//! 可视内容的是一组*页块*（`CT_PageBlock` 的子节点，见表 15）：文字对象、
//! 图形对象、图像对象、复合对象，以及可递归嵌套的页块分组。
//!
//! 这些对象是 [`crate::render`] 渲染器的输入。它们均派生自 `CT_GraphicUnit`
//! （表 23），共享边界、变换矩阵、绘制参数等图元属性；为与 `quick-xml` 的
//! 属性反序列化良好配合，这里将公共属性直接内联到各对象中，而非使用
//! `#[serde(flatten)]`。

use serde::{Deserialize, Serialize};

use crate::types::{StArray, StBox, StId, StRefId};

/// 图层 / 页块中的一个可绘制对象（`CT_PageBlock` 的子节点，见表 15）。
///
/// 以 XML 元素名区分类型，渲染时按文档顺序（即出现顺序）依次绘制以保证
/// 正确的叠放次序（z-order）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum PageBlock {
    /// 文字对象（见 11 章、表 45）。
    #[serde(rename = "TextObject")]
    Text(TextObject),
    /// 图形（路径）对象（见 9 章、表 34）。
    #[serde(rename = "PathObject")]
    Path(PathObject),
    /// 图像对象（见 12 章、表 43）。
    #[serde(rename = "ImageObject")]
    Image(ImageObject),
    /// 复合对象（见 13 章、表 49）；当前渲染器暂不展开其矢量内容。
    #[serde(rename = "CompositeObject")]
    Composite(CompositeObject),
    /// 页块分组，可递归嵌套（见表 15）。
    #[serde(rename = "PageBlock")]
    Block(PageBlockGroup),
}

/// 可递归嵌套的页块分组（`CT_PageBlock`，见表 15）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageBlockGroup {
    /// 分组标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 组内的子对象，按出现顺序绘制。
    #[serde(rename = "$value", default)]
    pub objects: Vec<PageBlock>,
}

/// `CT_Color`：颜色（见 8.3、表 27）。
///
/// 仅建模渲染所需的核心字段：分量值、颜色空间引用与透明度。渐变、底纹与
/// 调色板索引等高级着色暂未建模。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtColor {
    /// 颜色分量值，含义由所引用颜色空间决定（可选；缺省时按黑色处理）。
    #[serde(rename = "@Value")]
    pub value: Option<StArray<f64>>,
    /// 调色板索引（可选）。
    #[serde(rename = "@Index")]
    pub index: Option<i64>,
    /// 引用资源中定义的颜色空间标识（可选；缺省采用文档默认/ RGB）。
    #[serde(rename = "@ColorSpace")]
    pub color_space: Option<StRefId>,
    /// 颜色透明度，取值 0~255（可选，缺省 255 不透明）。
    #[serde(rename = "@Alpha")]
    pub alpha: Option<u8>,
}

/// `CT_Text`：文字对象（见 11 章、表 45）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TextObject {
    /// 对象标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 对象边界，相对页面坐标系（必选）。
    #[serde(rename = "@Boundary", default)]
    pub boundary: StBox,
    /// 对象变换矩阵，6 元素 `[a b c d e f]`（可选）。
    #[serde(rename = "@CTM")]
    pub ctm: Option<StArray<f64>>,
    /// 引用的绘制参数标识（可选）。
    #[serde(rename = "@DrawParam")]
    pub draw_param: Option<StRefId>,
    /// 对象整体透明度 0~255（可选）。
    #[serde(rename = "@Alpha")]
    pub alpha: Option<u8>,
    /// 引用的字型资源标识（必选；缺失时为 `0`，渲染时跳过）。
    #[serde(rename = "@Font", default)]
    pub font: StRefId,
    /// 字号，单位为毫米（必选；缺失时为 `0`，渲染时跳过）。
    #[serde(rename = "@Size", default)]
    pub size: f64,
    /// 是否勾边，默认 `false`（可选）。
    #[serde(rename = "@Stroke")]
    pub stroke: Option<bool>,
    /// 是否填充，默认 `true`（可选）。
    #[serde(rename = "@Fill")]
    pub fill: Option<bool>,
    /// 横向缩放比例，默认 `1.0`（可选）。
    #[serde(rename = "@HScale")]
    pub h_scale: Option<f64>,
    /// 阅读方向：文字排列方向相对 x 轴正向的顺时针角度，取 0/90/180/270，
    /// 默认 `0`（可选，见 11.3、表 47）。排列由各字 `DeltaX`/`DeltaY` 显式给出，
    /// 此属性为阅读次序的描述。
    #[serde(rename = "@ReadDirection")]
    pub read_direction: Option<i32>,
    /// 字符方向：单个字形基线相对 x 轴正向的顺时针旋转角，取 0/90/180/270，
    /// 默认 `0`（可选，见 11.3、表 47）。渲染时每个字形绕其原点旋转该角度。
    #[serde(rename = "@CharDirection")]
    pub char_direction: Option<i32>,
    /// 文字粗细值，取 0~1000 的百级档（100,200,…,900），默认 `400`（可选，见表 45）。
    #[serde(rename = "@Weight")]
    pub weight: Option<i32>,
    /// 是否为斜体样式，默认 `false`（可选，见表 45）。
    #[serde(rename = "@Italic")]
    pub italic: Option<bool>,
    /// 填充颜色（可选）。
    #[serde(rename = "FillColor")]
    pub fill_color: Option<CtColor>,
    /// 勾边颜色（可选）。
    #[serde(rename = "StrokeColor")]
    pub stroke_color: Option<CtColor>,
    /// 字形变换序列：将文字内容中的字符编码映射到具体字形索引（可选，见 11.4、表 48）。
    ///
    /// 规范中 `CGTransform` 与 `TextCode` 在 `CT_Text` 内交替出现，每个 `CGTransform`
    /// 的 `CodePosition` 相对其后 `TextCode` 的字符串起始位置。此处将其汇集为一个序列，
    /// 渲染时按 `CodePosition` 在所有 `TextCode` 拼接后的字符流中定位；对最常见的
    /// 单 `TextCode` 文字对象两种解释一致。
    #[serde(rename = "CGTransform", default)]
    pub cg_transforms: Vec<CtCgTransform>,
    /// 文字定位与内容序列（必选，至少一个）。
    #[serde(rename = "TextCode", default)]
    pub text_codes: Vec<TextCode>,
}

/// `CT_CGTransform`：字形变换，描述字符编码到字形索引的对应关系（见 11.4、表 48）。
///
/// 涵盖一对一、多对一、一对多、多对多四种关系：自 `code_position` 起的
/// `code_count` 个字符，整体对应 `glyphs` 给出的 `glyph_count` 个字形索引。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtCgTransform {
    /// 字符编码在文字内容中的起始位置，从 0 开始（必选）。
    #[serde(rename = "@CodePosition", default)]
    pub code_position: i32,
    /// 参与变换的字符数量，应 ≥1，默认 `1`（可选）。
    #[serde(rename = "@CodeCount")]
    pub code_count: Option<i32>,
    /// 变换得到的字形索引数量，应 ≥1，默认 `1`（可选）。
    #[serde(rename = "@GlyphCount")]
    pub glyph_count: Option<i32>,
    /// 变换后的字形索引列表（必选）。
    #[serde(rename = "Glyphs")]
    pub glyphs: Option<StArray<u32>>,
}

/// `TextCode`：一段文字的定位与字符内容（见表 46）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TextCode {
    /// 首字符的字形原点横坐标（对象坐标系，毫米）。
    #[serde(rename = "@X")]
    pub x: Option<f64>,
    /// 首字符的字形原点纵坐标（对象坐标系，毫米）。
    #[serde(rename = "@Y")]
    pub y: Option<f64>,
    /// 相邻字符在 X 方向的偏移序列，支持 `g` 游程编码（可选）。
    #[serde(rename = "@DeltaX")]
    pub delta_x: Option<String>,
    /// 相邻字符在 Y 方向的偏移序列，支持 `g` 游程编码（可选）。
    #[serde(rename = "@DeltaY")]
    pub delta_y: Option<String>,
    /// 字符内容。
    #[serde(rename = "$text")]
    pub text: Option<String>,
}

/// `CT_Path`：图形（路径）对象（见 9 章、表 34）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PathObject {
    /// 对象标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 对象边界，相对页面坐标系（必选）。
    #[serde(rename = "@Boundary", default)]
    pub boundary: StBox,
    /// 对象变换矩阵，6 元素 `[a b c d e f]`（可选）。
    #[serde(rename = "@CTM")]
    pub ctm: Option<StArray<f64>>,
    /// 引用的绘制参数标识（可选）。
    #[serde(rename = "@DrawParam")]
    pub draw_param: Option<StRefId>,
    /// 线宽，单位为毫米（可选）。
    #[serde(rename = "@LineWidth")]
    pub line_width: Option<f64>,
    /// 线条端点样式 `Butt`/`Round`/`Square`，默认 `Butt`（可选，见表 34）。
    #[serde(rename = "@Cap")]
    pub cap: Option<String>,
    /// 线条连接样式 `Miter`/`Round`/`Bevel`，默认 `Miter`（可选，见表 34）。
    #[serde(rename = "@Join")]
    pub join: Option<String>,
    /// `Join` 为 `Miter` 时的斜接限制，默认 `4.234`（可选，见表 34）。
    #[serde(rename = "@MiterLimit")]
    pub miter_limit: Option<f64>,
    /// 虚线起始相位，默认 `0`（可选，见表 34）。
    #[serde(rename = "@DashOffset")]
    pub dash_offset: Option<f64>,
    /// 虚线重复样式（实线段/空白段长度交替序列，可选，见表 34）。
    #[serde(rename = "@DashPattern")]
    pub dash_pattern: Option<StArray<f64>>,
    /// 对象整体透明度 0~255（可选）。
    #[serde(rename = "@Alpha")]
    pub alpha: Option<u8>,
    /// 是否勾边，默认 `true`（可选）。
    #[serde(rename = "@Stroke")]
    pub stroke: Option<bool>,
    /// 是否填充，默认 `false`（可选）。
    #[serde(rename = "@Fill")]
    pub fill: Option<bool>,
    /// 填充时的判定规则，`NonZero` 或 `Even-Odd`，默认 `NonZero`（可选）。
    #[serde(rename = "@Rule")]
    pub rule: Option<String>,
    /// 填充颜色（可选）。
    #[serde(rename = "FillColor")]
    pub fill_color: Option<CtColor>,
    /// 勾边颜色（可选）。
    #[serde(rename = "StrokeColor")]
    pub stroke_color: Option<CtColor>,
    /// 路径绘制指令的紧缩表示（必选）。
    #[serde(rename = "AbbreviatedData")]
    pub abbreviated_data: Option<String>,
}

/// `CT_Image`：图像对象（见 12 章、表 43）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ImageObject {
    /// 对象标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 对象边界，相对页面坐标系（必选）。
    #[serde(rename = "@Boundary", default)]
    pub boundary: StBox,
    /// 对象变换矩阵，6 元素 `[a b c d e f]`（可选）。
    #[serde(rename = "@CTM")]
    pub ctm: Option<StArray<f64>>,
    /// 对象整体透明度 0~255（可选）。
    #[serde(rename = "@Alpha")]
    pub alpha: Option<u8>,
    /// 引用的多媒体资源标识（必选）。
    #[serde(rename = "@ResourceID", default)]
    pub resource_id: StRefId,
    /// 图像蒙版资源标识（可选）。
    #[serde(rename = "@ImageMask")]
    pub image_mask: Option<StRefId>,
}

/// `CT_Composite`：复合对象（见 13 章、表 49）。
///
/// 复合对象引用资源文件中定义的矢量图像组（`CT_VectorG`）。当前渲染器仅
/// 保留其边界信息，暂不展开内部矢量内容。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompositeObject {
    /// 对象标识（可选）。
    #[serde(rename = "@ID")]
    pub id: Option<StId>,
    /// 对象边界，相对页面坐标系（必选）。
    #[serde(rename = "@Boundary", default)]
    pub boundary: StBox,
    /// 对象变换矩阵（可选）。
    #[serde(rename = "@CTM")]
    pub ctm: Option<StArray<f64>>,
    /// 引用的矢量图像组资源标识（必选）。
    #[serde(rename = "@ResourceID", default)]
    pub resource_id: StRefId,
}

/// 解析 `DeltaX` / `DeltaY` 偏移序列。
///
/// 序列以空格分隔；其中 `g` 引导一段游程编码：`g n v` 表示数值 `v` 连续
/// 出现 `n` 次。例如 `"g 3 4.5 10"` 解析为 `[4.5, 4.5, 4.5, 10.0]`。
pub fn parse_deltas(raw: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut tokens = raw.split_whitespace();
    while let Some(tok) = tokens.next() {
        if tok == "g" || tok == "G" {
            let count = tokens.next().and_then(|s| s.parse::<usize>().ok());
            let value = tokens.next().and_then(|s| s.parse::<f64>().ok());
            if let (Some(count), Some(value)) = (count, value) {
                out.extend(std::iter::repeat_n(value, count));
            }
        } else if let Ok(v) = tok.parse::<f64>() {
            out.push(v);
        }
    }
    out
}

/// `CT_VectorG`：矢量图像（复合对象引用的图形内容，见 13 章、表 49、表 50）。
///
/// 资源文件中以 `CompositeGraphicUnit` 的形式出现，被页面中的复合对象
/// （[`CompositeObject`]）通过 `ResourceID` 引用。其 `Content` 为一个
/// `CT_PageBlock`，承载文字、图形、图像乃至嵌套复合对象等绘制内容；这些内容
/// 在矢量图自身坐标系（原点位于左上角，范围 `Width`×`Height`）中描述，绘制时
/// 经复合对象的边界与变换矩阵映射到页面。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CtVectorG {
    /// 资源标识（必选）。
    #[serde(rename = "@ID")]
    pub id: StId,
    /// 矢量图自身坐标系的宽度（必选）。
    #[serde(rename = "@Width", default)]
    pub width: f64,
    /// 矢量图自身坐标系的高度（必选）。
    #[serde(rename = "@Height", default)]
    pub height: f64,
    /// 缩略图资源标识（可选）。
    #[serde(rename = "@Thumbnail")]
    pub thumbnail: Option<StRefId>,
    /// 替换图像资源标识（可选）。
    #[serde(rename = "@Substitution")]
    pub substitution: Option<StRefId>,
    /// 矢量内容（`CT_PageBlock`，必选）；缺失时无可绘制内容。
    #[serde(rename = "Content")]
    pub content: Option<PageBlockGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delta_runlength() {
        assert_eq!(parse_deltas("g 3 4.5 10"), vec![4.5, 4.5, 4.5, 10.0]);
        assert_eq!(parse_deltas("1 2 3"), vec![1.0, 2.0, 3.0]);
        assert!(parse_deltas("").is_empty());
    }
}
