//! OFD → 图片渲染（光栅化）。
//!
//! 在 [`crate::OfdReader`] 之上提供将版式页面渲染为位图的能力：
//!
//! - 以毫米为单位的页面物理区域，按给定 **DPI** 换算为像素画布；
//! - 依模板页（背景/前景）与图层类型（Background/Body/Foreground）的叠放
//!   次序，逐个绘制文字、图形与图像对象；
//! - 通过 [`image`] 编码为 PNG / JPEG / BMP / TIFF / GIF / WebP 等常见格式，
//!   格式由参数指定。
//!
//! 坐标与变换遵循规范 8.5：页面坐标系原点位于物理区域左上角，y 轴向下；
//! 每个图元对象的内部坐标先经其变换矩阵 `CTM`，再平移到边界 `Boundary`
//! 左上角，得到页面坐标，最后按 `DPI` 缩放到设备像素。
//!
//! # 示例
//!
//! ```no_run
//! use ofd_core::{OfdReader, render::RenderOptions};
//!
//! let mut reader = OfdReader::open("sample.ofd")?;
//! let body = reader.ofd().doc_bodies[0].clone();
//! let doc = reader.load_document(&body)?;
//!
//! // 将首页以 200 DPI 渲染并保存为 PNG（格式由扩展名推断）。
//! let opts = RenderOptions::with_dpi(200.0);
//! reader.render_page_to_file(&doc, 0, &opts, "page0.png")?;
//! # Ok::<(), ofd_core::OfdError>(())
//! ```

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

use image::{DynamicImage, RgbaImage};

/// 重新导出 [`image::ImageFormat`]，便于调用方指定输出格式而无需直接依赖 `image`。
pub use image::ImageFormat;
use tiny_skia::{
    FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform as SkTransform,
};

use crate::error::{OfdError, Result};
use crate::model::graphics::{
    CompositeObject, CtCgTransform, CtColor, CtVectorG, ImageObject, PageBlock, PathObject,
    TextObject, parse_deltas,
};
use crate::model::resource::{CtColorSpace, CtDrawParam};
use crate::types::{StBox, StLoc, StRefId, parent_dir, resolve_path};
use crate::{LoadedDocument, OfdReader, PageObject, PageRef};

use std::io::{Read, Seek};

/// 设备像素的安全上限，超过则拒绝渲染以避免超大内存分配。
const MAX_DIMENSION: u32 = 20_000;

/// 渲染参数。
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// 输出分辨率（每英寸点数）。毫米尺寸据此换算为像素。
    pub dpi: f64,
    /// 画布背景色 `[R, G, B, A]`。`None` 表示透明背景。
    pub background: Option<[u8; 4]>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            dpi: 150.0,
            background: Some([255, 255, 255, 255]),
        }
    }
}

impl RenderOptions {
    /// 以指定 DPI 构造渲染参数，其余取默认（白色不透明背景）。
    pub fn with_dpi(dpi: f64) -> Self {
        RenderOptions {
            dpi,
            ..Default::default()
        }
    }

    /// 设置背景色（`None` 为透明）。
    pub fn background(mut self, background: Option<[u8; 4]>) -> Self {
        self.background = background;
        self
    }
}

/// OFD 长度单位（毫米）到英寸的换算系数。
const MM_PER_INCH: f64 = 25.4;

/// 解析出的字型资源。
struct FontRes {
    /// 字型文件字节：优先取包内内嵌字型，缺省时回退到同名系统字体；
    /// 两者皆无时为 `None`，该字型无法绘制轮廓。
    data: Option<Vec<u8>>,
    /// 字型在字体文件中的字面索引（TTC 集合内的序号；单字体为 0）。
    index: u32,
    /// 是否为系统字体替代（而非包内内嵌字型）。替代字型的字宽未必与文档排布
    /// 所依据的原字型一致，绘制时需按字位宽度收紧（见 [`squeeze_to_advance`]）。
    substituted: bool,
}

/// 进程级系统字体库，按需加载一次。
static SYSTEM_FONTS: OnceLock<fontdb::Database> = OnceLock::new();

/// 返回（首次调用时加载）系统字体库。
fn system_fonts() -> &'static fontdb::Database {
    SYSTEM_FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// 名称含 CJK 字符时优先回退的常见中文字型族（按可得性概率排序）。
///
/// OFD 常以“宋体”“楷体”等中文名引用字型却不内嵌，而这些名称在多数 Linux
/// 系统并非已安装字型的族名（如方正楷体注册名为“方正楷体_GBK”）。按名称
/// 匹配失败时若直接落到通用 sans-serif（往往是仅含拉丁字形的 Roboto），中文
/// 会因无字形而整体丢失；故先尝试这一组确实含中文字形的字型族。
const CJK_FALLBACK_FAMILIES: &[&str] = &[
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Noto Sans SC",
    "Microsoft YaHei",
    "微软雅黑",
    "SimSun",
    "宋体",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
    "Noto Serif CJK SC",
];

/// 判断字符是否落在常见 CJK 区段（用于决定是否需要中文字型兜底）。
fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF   // CJK 统一表意文字
        | 0x3400..=0x4DBF // 扩展 A
        | 0x3000..=0x303F // CJK 标点
        | 0xFF00..=0xFFEF // 全角字符
        | 0x2E80..=0x2EFF // 部首补充
    )
}

/// 常见中文字型的拉丁族名前缀（已规范化：小写、去空白与连字符）。
///
/// OFD 里引用中文字型时既可能写中文名（“宋体”“楷体”），也可能写 Windows 上的
/// 拉丁注册名（`SimHei`、`KaiTi`、`DengXian`）。后者不含 CJK 字符，若只按字符区段
/// 判断就不会触发中文兜底，最终落到往往仅含拉丁字形（甚至根本未安装）的通用族，
/// 中文随之整体丢失。故显式列出这些名称。
const CJK_LATIN_FAMILY_PREFIXES: &[&str] = &[
    "simsun",
    "nsimsun",
    "simhei",
    "simkai",
    "simfang",
    "simli",
    "simyou",
    "kaiti",
    "fangsong",
    "dengxian",
    "youyuan",
    "lisu",
    "stsong",
    "stkaiti",
    "stheiti",
    "stfangsong",
    "stxihei",
    "stzhongsong",
    "msyh",
    "microsoftyahei",
    "microsoftjhenghei",
    "pingfang",
    "hiraginosansgb",
    "heitisc",
    "songtisc",
    "mingliu",
    "dfkai",
];

/// 规范化字型名以便与 [`CJK_LATIN_FAMILY_PREFIXES`] 比对：转小写并去掉空白、
/// 连字符与下划线（覆盖 `Microsoft YaHei UI`、`KaiTi_GB2312` 等写法）。
fn normalize_family(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// 判断所引字型是否为中文字型（含 CJK 字符，或命中常见中文字型的拉丁族名）。
fn is_cjk_family(name: &str, family: &str) -> bool {
    if name.chars().chain(family.chars()).any(is_cjk) {
        return true;
    }
    [name, family].iter().any(|s| {
        let norm = normalize_family(s);
        !norm.is_empty()
            && CJK_LATIN_FAMILY_PREFIXES
                .iter()
                .any(|p| norm.starts_with(p))
    })
}

/// 按字型名/族名在系统字体中查找替代字体，返回（字体字节, 字面索引）。
///
/// OFD 常仅以名称引用系统字体（如“宋体”“SimHei”）而不内嵌字型文件；此时按名称
/// 匹配。匹配失败时按以下顺序兜底，任一环节命中即返回：
///
/// 1. 所引字型为中文字型（见 [`is_cjk_family`]）时，回退到一组确实含中文字形的
///    字型族（见 [`CJK_FALLBACK_FAMILIES`]），避免落到仅含拉丁字形的字型；
/// 2. 通用无衬线字体；
/// 3. 仍未命中（系统未安装通用族所指向的实体字型时会如此）则再试中文字型族，
///    并最终任取一款已安装字型 —— 字形不同好过整段文字凭空消失。
fn lookup_system_font(
    name: &str,
    family: &str,
    bold: bool,
    italic: bool,
) -> Option<(Vec<u8>, u32)> {
    let db = system_fonts();
    let style = if italic {
        fontdb::Style::Italic
    } else {
        fontdb::Style::Normal
    };
    let weight = if bold {
        fontdb::Weight::BOLD
    } else {
        fontdb::Weight::NORMAL
    };

    let query_family = |fam: &str| -> Option<(Vec<u8>, u32)> {
        let fam = fam.trim();
        if fam.is_empty() {
            return None;
        }
        let q = fontdb::Query {
            families: &[fontdb::Family::Name(fam)],
            weight,
            style,
            stretch: fontdb::Stretch::Normal,
        };
        db.query(&q)
            .and_then(|id| db.with_face_data(id, |data, index| (data.to_vec(), index)))
    };

    let query_cjk_families = || {
        CJK_FALLBACK_FAMILIES
            .iter()
            .find_map(|fam| query_family(fam))
    };

    // 1. 按字型名 → 族名精确匹配。
    for cand in [name, family] {
        if let Some(data) = query_family(cand) {
            return Some(data);
        }
    }

    // 2. 中文字型未命中已安装字型时，回退到任一可用的中文字型，以免落到仅含
    //    拉丁字形的 sans-serif 导致中文整体缺失。
    if is_cjk_family(name, family)
        && let Some(data) = query_cjk_families()
    {
        return Some(data);
    }

    // 3. 通用无衬线兜底。
    let q = fontdb::Query {
        families: &[fontdb::Family::SansSerif],
        weight,
        style,
        stretch: fontdb::Stretch::Normal,
    };
    if let Some(data) = db
        .query(&q)
        .and_then(|id| db.with_face_data(id, |data, index| (data.to_vec(), index)))
    {
        return Some(data);
    }

    // 4. 通用族在本机无对应实体字型（fontdb 的 sans-serif 默认指向 Arial 等，
    //    未必安装）：再试中文字型族（其同样含拉丁字形），最后任取一款已安装
    //    字型，宁可字形不同也不要整段文字消失。
    query_cjk_families().or_else(|| {
        let id = db.faces().next()?.id;
        db.with_face_data(id, |data, index| (data.to_vec(), index))
    })
}

/// 一篇文档渲染所需的资源集合（颜色空间、绘制参数、字型、多媒体）。
#[derive(Default)]
struct DocResources {
    color_spaces: HashMap<u64, CtColorSpace>,
    draw_params: HashMap<u64, CtDrawParam>,
    fonts: HashMap<u64, FontRes>,
    media: HashMap<u64, Vec<u8>>,
    vector_gs: HashMap<u64, CtVectorG>,
    default_cs: Option<u64>,
}

/// OFD 仿射变换矩阵，采用规范约定 `[a b c d e f]`：
///
/// ```text
/// x' = a*x + c*y + e
/// y' = b*x + d*y + f
/// ```
#[derive(Debug, Clone, Copy)]
struct Mat {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Mat {
    /// 单位矩阵。
    fn identity() -> Self {
        Mat {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// 平移矩阵。
    fn translate(x: f64, y: f64) -> Self {
        Mat {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        }
    }

    /// 缩放矩阵。
    fn scale(sx: f64, sy: f64) -> Self {
        Mat {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    /// 旋转矩阵，`theta` 为弧度。在 y 轴向下的对象坐标系中表现为顺时针旋转，
    /// 与规范表 47 “从 x 轴正向顺时针”的角度约定一致。
    fn rotate(theta: f64) -> Self {
        let (s, c) = theta.sin_cos();
        Mat {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// 由规范 `[a b c d e f]` 数组构造。
    fn from_array(v: &[f64]) -> Option<Self> {
        if v.len() == 6 {
            Some(Mat {
                a: v[0],
                b: v[1],
                c: v[2],
                d: v[3],
                e: v[4],
                f: v[5],
            })
        } else {
            None
        }
    }

    /// 矩阵复合：返回 `self ∘ rhs`，即先应用 `rhs` 再应用 `self`。
    fn mul(self, rhs: Mat) -> Mat {
        Mat {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    /// 转换为 tiny-skia 变换。
    fn to_skia(self) -> SkTransform {
        SkTransform::from_row(
            self.a as f32,
            self.b as f32,
            self.c as f32,
            self.d as f32,
            self.e as f32,
            self.f as f32,
        )
    }
}

/// 解析得到的 RGBA 颜色（分量 0~255）。
type Rgba = [u8; 4];

impl<R: Read + Seek> OfdReader<R> {
    /// 将文档第 `page_index` 页渲染为像素图（[`tiny_skia::Pixmap`]）。
    ///
    /// `page_index` 为页树中的页序号（从 0 起）。
    pub fn render_page(
        &mut self,
        doc: &LoadedDocument,
        page_index: usize,
        options: &RenderOptions,
    ) -> Result<Pixmap> {
        let pages = doc.pages();
        let page_ref = pages
            .get(page_index)
            .ok_or_else(|| OfdError::Render(format!("page index {page_index} out of range")))?
            .clone();

        // 装载本页及其页级资源。
        let page = self.load_page(doc, &page_ref)?;
        let mut resources = self.build_resources(doc)?;
        let page_res: Vec<StLoc> = page.page_res.clone();
        for loc in &page_res {
            self.load_res_into(&doc.base, loc, &mut resources);
        }

        // 预装载模板页内容（背景/前景），避免后续与资源借用冲突。
        let mut bg_templates: Vec<PageObject> = Vec::new();
        let mut fg_templates: Vec<PageObject> = Vec::new();
        for tref in &page.templates {
            let Some(tpl) = doc
                .template_pages()
                .iter()
                .find(|t| t.id == tref.template_id.as_id())
                .cloned()
            else {
                continue;
            };
            let zorder = tref
                .z_order
                .clone()
                .or_else(|| tpl.z_order.clone())
                .unwrap_or_else(|| "Background".to_string());
            // 模板内容文件缺失时跳过，不影响整页渲染。
            let Ok(tpl_page) = self.load_template(doc, &tpl) else {
                continue;
            };
            if zorder.eq_ignore_ascii_case("Foreground") {
                fg_templates.push(tpl_page);
            } else {
                bg_templates.push(tpl_page);
            }
        }

        // 页面物理区域（优先本页 Area，其次文档默认 PageArea，仍缺省用 A4）。
        let area = page
            .area
            .as_ref()
            .or(doc.document.common_data.page_area.as_ref())
            .map(|a| a.physical_box)
            .unwrap_or(StBox::A4_MM);

        let scale = options.dpi / MM_PER_INCH;
        let width_px = ((area.width * scale).round() as i64).max(1);
        let height_px = ((area.height * scale).round() as i64).max(1);
        if width_px > MAX_DIMENSION as i64 || height_px > MAX_DIMENSION as i64 {
            return Err(OfdError::Render(format!(
                "rendered size {width_px}x{height_px} exceeds limit {MAX_DIMENSION}"
            )));
        }

        let mut pixmap = Pixmap::new(width_px as u32, height_px as u32)
            .ok_or_else(|| OfdError::Render("failed to allocate pixmap".to_string()))?;
        if let Some([r, g, b, a]) = options.background {
            pixmap.fill(tiny_skia::Color::from_rgba8(r, g, b, a));
        }

        // 页面坐标（毫米）→ 设备像素：缩放并将物理区域左上角平移到原点。
        let page_to_device =
            Mat::translate(-area.x * scale, -area.y * scale).mul(Mat::scale(scale, scale));

        // 叠放次序：背景模板 → 页面内容 → 前景模板。
        for tpl in &bg_templates {
            render_page_object(&mut pixmap, &resources, page_to_device, tpl);
        }
        render_page_object(&mut pixmap, &resources, page_to_device, &page);
        for tpl in &fg_templates {
            render_page_object(&mut pixmap, &resources, page_to_device, tpl);
        }

        // 注释（如电子印章/签章）叠加在页面内容之上。
        self.render_annotations(doc, &page_ref, &resources, page_to_device, &mut pixmap);

        Ok(pixmap)
    }

    /// 渲染指定页的注释外观（叠加在页面内容之上）。
    ///
    /// 注释入口、页注释文件或其引用的资源缺失时静默跳过，不影响整页渲染。
    fn render_annotations(
        &mut self,
        doc: &LoadedDocument,
        page_ref: &PageRef,
        res: &DocResources,
        page_to_device: Mat,
        pixmap: &mut Pixmap,
    ) {
        let Some(loc) = doc.document.annotations.clone() else {
            return;
        };
        let path = resolve_path(&doc.base, &loc);
        let Ok(annotations) = self.package.parse::<crate::Annotations>(&path) else {
            return;
        };
        // 页注释文件路径相对注释入口文件所在目录解析。
        let ann_dir = parent_dir(&path).to_string();
        let page_id = page_ref.id.value();

        for pg in &annotations.pages {
            if pg.page_id.value() != page_id {
                continue;
            }
            let file_path = resolve_path(&ann_dir, &pg.file_loc);
            let Ok(page_annot) = self.package.parse::<crate::PageAnnot>(&file_path) else {
                continue;
            };
            for annot in &page_annot.annots {
                if annot.visible == Some(false) {
                    continue;
                }
                let Some(app) = &annot.appearance else {
                    continue;
                };
                // 外观内图元坐标相对外观边界左上角。
                let app_to_device =
                    page_to_device.mul(Mat::translate(app.boundary.x, app.boundary.y));
                for obj in &app.objects {
                    render_block(pixmap, res, app_to_device, obj, None);
                }
            }
        }
    }

    /// 将指定页渲染为 [`image::RgbaImage`]。
    pub fn render_page_to_image(
        &mut self,
        doc: &LoadedDocument,
        page_index: usize,
        options: &RenderOptions,
    ) -> Result<RgbaImage> {
        let pixmap = self.render_page(doc, page_index, options)?;
        Ok(pixmap_to_image(&pixmap))
    }

    /// 将指定页渲染并编码为给定格式的图片字节。
    pub fn render_page_to_bytes(
        &mut self,
        doc: &LoadedDocument,
        page_index: usize,
        options: &RenderOptions,
        format: ImageFormat,
    ) -> Result<Vec<u8>> {
        let img = self.render_page_to_image(doc, page_index, options)?;
        encode_image(img, format)
    }

    /// 将指定页渲染并写入文件，图片格式由路径扩展名推断。
    ///
    /// 支持的扩展名：`png`、`jpg`/`jpeg`、`bmp`、`tif`/`tiff`、`gif`、`webp`。
    pub fn render_page_to_file<P: AsRef<Path>>(
        &mut self,
        doc: &LoadedDocument,
        page_index: usize,
        options: &RenderOptions,
        path: P,
    ) -> Result<()> {
        let path = path.as_ref();
        let format = format_from_path(path).ok_or_else(|| {
            OfdError::Render(format!(
                "cannot infer image format from path: {}",
                path.display()
            ))
        })?;
        let bytes = self.render_page_to_bytes(doc, page_index, options, format)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// 收集文档级资源（公共资源 + 文档资源）。
    fn build_resources(&mut self, doc: &LoadedDocument) -> Result<DocResources> {
        let mut res = DocResources {
            default_cs: doc.document.common_data.default_cs.map(|id| id.value()),
            ..Default::default()
        };
        let locs: Vec<StLoc> = doc
            .public_res()
            .iter()
            .chain(doc.document_res().iter())
            .cloned()
            .collect();
        for loc in &locs {
            self.load_res_into(&doc.base, loc, &mut res);
        }
        Ok(res)
    }

    /// 解析一个资源文件并把其中的颜色空间、绘制参数、字型、多媒体并入 `res`。
    ///
    /// 资源文件本身或其引用的数据文件缺失时静默跳过，以尽量完成渲染。
    fn load_res_into(&mut self, base_dir: &str, loc: &StLoc, res: &mut DocResources) {
        let res_path = resolve_path(base_dir, loc);
        let Ok(parsed) = self.package.parse::<crate::Res>(&res_path) else {
            return;
        };
        // 资源数据文件的基准目录：资源文件所在目录叠加其 BaseLoc。
        let res_dir = parent_dir(&res_path).to_string();
        let data_base = resolve_path(&res_dir, &parsed.base_loc);

        for cs in parsed.color_spaces() {
            res.color_spaces.insert(cs.id.value(), cs.clone());
        }
        for dp in parsed.draw_params() {
            res.draw_params.insert(dp.id.value(), dp.clone());
        }
        for font in parsed.fonts() {
            let embedded = font
                .font_file
                .as_ref()
                .map(|ff| resolve_path(&data_base, ff))
                .and_then(|p| self.package.read(&p).ok());
            let res_font = match embedded {
                Some(data) => FontRes {
                    data: Some(data),
                    index: 0,
                    substituted: false,
                },
                // 未内嵌字型：回退到同名系统字体，避免文字整体缺失。
                None => {
                    let (data, index) = lookup_system_font(
                        &font.font_name,
                        font.family_name.as_deref().unwrap_or(""),
                        font.bold.unwrap_or(false),
                        font.italic.unwrap_or(false),
                    )
                    .map(|(d, i)| (Some(d), i))
                    .unwrap_or((None, 0));
                    FontRes {
                        data,
                        index,
                        substituted: true,
                    }
                }
            };
            res.fonts.insert(font.id.value(), res_font);
        }
        for mm in parsed.multi_medias() {
            let p = resolve_path(&data_base, &mm.media_file);
            if let Ok(bytes) = self.package.read(&p) {
                res.media.insert(mm.id.value(), bytes);
            }
        }
        for vg in parsed.composite_graphic_units() {
            res.vector_gs.insert(vg.id.value(), vg.clone());
        }
    }
}

/// 渲染一个页对象（普通页或模板页）的全部图层。
///
/// 图层按类型排序绘制：Background → Body → Foreground；同一图层内的对象按
/// 文档顺序绘制。
fn render_page_object(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    page: &PageObject,
) {
    let Some(content) = &page.content else {
        return;
    };
    let mut layers: Vec<_> = content.layers.iter().collect();
    layers.sort_by_key(|l| match l.layer_type.as_deref() {
        Some("Background") => 0,
        Some("Foreground") => 2,
        _ => 1, // Body（默认）
    });
    for layer in layers {
        // 图层级绘制参数：作为层内所有图元的默认绘制参数（见表 14、表 15）。
        let layer_dp = resolve_draw_param(res, layer.draw_param);
        for obj in &layer.objects {
            render_block(pixmap, res, page_to_device, obj, layer_dp.as_ref());
        }
    }
}

/// 渲染单个页块对象（必要时递归进入分组）。
///
/// `inherited` 为来自图层（或外层分组）的默认绘制参数，供未自带绘制参数的
/// 图元继承颜色、线宽等属性。
fn render_block(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    block: &PageBlock,
    inherited: Option<&CtDrawParam>,
) {
    match block {
        PageBlock::Path(p) => render_path(pixmap, res, page_to_device, p, inherited),
        PageBlock::Text(t) => render_text(pixmap, res, page_to_device, t, inherited),
        PageBlock::Image(i) => render_image(pixmap, res, page_to_device, i),
        PageBlock::Block(g) => {
            for obj in &g.objects {
                render_block(pixmap, res, page_to_device, obj, inherited);
            }
        }
        PageBlock::Composite(c) => render_composite(pixmap, res, page_to_device, c, 0, inherited),
    }
}

/// 复合对象（矢量图像）的最大展开深度，防止矢量图相互引用导致无限递归。
const MAX_COMPOSITE_DEPTH: u32 = 16;

/// 绘制复合对象：展开其按 `ResourceID` 引用的矢量图像（`CT_VectorG`）内容。
///
/// 矢量图内部图元在其自身坐标系中描述，经复合对象的边界与变换矩阵 `CTM`
/// 映射到页面，再按上层变换绘制到设备像素。矢量图内容本身可再嵌套复合对象，
/// 以 `depth` 限制展开层数避免循环引用。
fn render_composite(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    obj: &CompositeObject,
    depth: u32,
    inherited: Option<&CtDrawParam>,
) {
    if depth >= MAX_COMPOSITE_DEPTH {
        return;
    }
    let Some(vg) = res.vector_gs.get(&obj.resource_id.value()) else {
        return;
    };
    let Some(content) = &vg.content else {
        return;
    };
    // 矢量图坐标系 → 页面：先经复合对象 CTM，再平移到其边界左上角。
    let inner_to_device = object_to_device(
        page_to_device,
        &obj.boundary,
        obj.ctm.as_ref().map(|a| a.as_slice()),
    );
    for block in &content.objects {
        render_block_at_depth(pixmap, res, inner_to_device, block, depth + 1, inherited);
    }
}

/// 与 [`render_block`] 相同，但携带复合对象展开深度，供矢量图内容递归绘制。
fn render_block_at_depth(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    block: &PageBlock,
    depth: u32,
    inherited: Option<&CtDrawParam>,
) {
    match block {
        PageBlock::Composite(c) => {
            render_composite(pixmap, res, page_to_device, c, depth, inherited)
        }
        PageBlock::Block(g) => {
            for obj in &g.objects {
                render_block_at_depth(pixmap, res, page_to_device, obj, depth, inherited);
            }
        }
        _ => render_block(pixmap, res, page_to_device, block, inherited),
    }
}

/// 计算图元对象坐标系 → 设备像素的变换：`page_to_device ∘ translate(边界) ∘ CTM`。
fn object_to_device(page_to_device: Mat, boundary: &StBox, ctm: Option<&[f64]>) -> Mat {
    let m = ctm.and_then(Mat::from_array).unwrap_or_else(Mat::identity);
    page_to_device
        .mul(Mat::translate(boundary.x, boundary.y))
        .mul(m)
}

/// 解析图元引用的绘制参数，并沿 `@Relative` 继承链回退合并各属性。
///
/// 子绘制参数显式设置的属性优先，未设置的属性逐级回退到父绘制参数（见 8.2、
/// 表 24）。返回展平后的拷贝，调用方可像访问单个绘制参数一样取用各字段。
/// 通过记录已访问标识避免循环引用导致的无限递归。
fn resolve_draw_param(res: &DocResources, id: Option<StRefId>) -> Option<CtDrawParam> {
    let mut cur = res.draw_params.get(&id?.value())?.clone();
    let mut visited = vec![cur.id.value()];
    while let Some(rid) = cur.relative.map(|r| r.value()) {
        if visited.contains(&rid) {
            break;
        }
        let Some(parent) = res.draw_params.get(&rid) else {
            break;
        };
        visited.push(rid);
        fill_missing_draw_param(&mut cur, parent);
        // 沿链继续向上：父参数的 `Relative` 决定下一级。
        cur.relative = parent.relative;
    }
    Some(cur)
}

/// 将 `dst` 中未显式设置（`None`）的属性回退填充为 `src` 的对应值。
/// 用于 `@Relative` 继承与图层级绘制参数继承（子优先、父兜底）。
fn fill_missing_draw_param(dst: &mut CtDrawParam, src: &CtDrawParam) {
    dst.line_width = dst.line_width.or(src.line_width);
    dst.join = dst.join.take().or_else(|| src.join.clone());
    dst.cap = dst.cap.take().or_else(|| src.cap.clone());
    dst.miter_limit = dst.miter_limit.or(src.miter_limit);
    dst.dash_offset = dst.dash_offset.or(src.dash_offset);
    dst.dash_pattern = dst.dash_pattern.take().or_else(|| src.dash_pattern.clone());
    dst.fill_color = dst.fill_color.take().or_else(|| src.fill_color.clone());
    dst.stroke_color = dst.stroke_color.take().or_else(|| src.stroke_color.clone());
}

/// 计算图元实际生效的绘制参数：先解析图元自身引用的绘制参数（含 `@Relative`
/// 链），再以图层（[`CtLayer`]）继承的绘制参数 `inherited` 作为最低优先级兜底。
///
/// 优先级：图元内联属性 > 图元 `DrawParam` 链 > 图层 `DrawParam`（见 8.2、表 14）。
/// 图元内联属性在各绘制函数中单独优先处理，故此处仅合并后两级。
fn effective_draw_param(
    res: &DocResources,
    id: Option<StRefId>,
    inherited: Option<&CtDrawParam>,
) -> Option<CtDrawParam> {
    match (resolve_draw_param(res, id), inherited) {
        (Some(mut own), Some(parent)) => {
            fill_missing_draw_param(&mut own, parent);
            Some(own)
        }
        (Some(own), None) => Some(own),
        (None, Some(parent)) => Some(parent.clone()),
        (None, None) => None,
    }
}

/// 绘制图形（路径）对象。
fn render_path(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    obj: &PathObject,
    inherited: Option<&CtDrawParam>,
) {
    let Some(data) = &obj.abbreviated_data else {
        return;
    };
    let Some(path) = build_path(data) else {
        return;
    };
    let transform = object_to_device(
        page_to_device,
        &obj.boundary,
        obj.ctm.as_ref().map(|a| a.as_slice()),
    )
    .to_skia();
    let dp = effective_draw_param(res, obj.draw_param, inherited);
    let dp = dp.as_ref();

    let fill = obj.fill.unwrap_or(false);
    let stroke = obj.stroke.unwrap_or(true);
    let obj_alpha = obj.alpha;

    // 规范表 26：路径对象的填充色缺省为透明色（与表 45 文字对象缺省黑色不同）。
    // 未给出填充色时若按黑色填充，会把「Fill=true + Stroke=true」的勾边图形（如
    // 发票上的 ⊗ 标记）涂成实心黑块，掩盖其后的勾边笔迹。
    if fill
        && let Some(color) = obj
            .fill_color
            .as_ref()
            .or_else(|| dp.and_then(|d| d.fill_color.as_ref()))
            .map(|c| resolve_color(res, c, obj_alpha))
    {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        let rule = match obj.rule.as_deref() {
            Some("Even-Odd") | Some("EvenOdd") => FillRule::EvenOdd,
            _ => FillRule::Winding,
        };
        pixmap.fill_path(&path, &paint, rule, transform, None);
    }

    if stroke {
        let color = obj
            .stroke_color
            .as_ref()
            .or_else(|| dp.and_then(|d| d.stroke_color.as_ref()))
            .map(|c| resolve_color(res, c, obj_alpha))
            .unwrap_or([0, 0, 0, alpha_or_opaque(obj_alpha)]);
        let width = obj
            .line_width
            .or_else(|| dp.and_then(|d| d.line_width))
            .unwrap_or(0.353);
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        // 端点/连接/斜接样式：图元属性优先，缺省回退绘制参数，再回退规范默认（表 34）。
        let cap = obj
            .cap
            .as_deref()
            .or_else(|| dp.and_then(|d| d.cap.as_deref()));
        let join = obj
            .join
            .as_deref()
            .or_else(|| dp.and_then(|d| d.join.as_deref()));
        let miter = obj
            .miter_limit
            .or_else(|| dp.and_then(|d| d.miter_limit))
            .unwrap_or(4.234);
        // 虚线样式：实线段/空白段长度交替；偏移为起始相位（均以毫米计）。
        let dash = obj
            .dash_pattern
            .as_ref()
            .or_else(|| dp.and_then(|d| d.dash_pattern.as_ref()))
            .map(|p| p.as_slice().iter().map(|&v| v as f32).collect::<Vec<_>>())
            .filter(|p| p.len() >= 2 && p.iter().any(|&v| v > 0.0))
            .and_then(|p| {
                let off = obj
                    .dash_offset
                    .or_else(|| dp.and_then(|d| d.dash_offset))
                    .unwrap_or(0.0) as f32;
                tiny_skia::StrokeDash::new(p, off)
            });
        let stroke_style = Stroke {
            width: width.max(f64::MIN_POSITIVE) as f32,
            line_cap: match cap {
                Some("Round") => tiny_skia::LineCap::Round,
                Some("Square") => tiny_skia::LineCap::Square,
                _ => tiny_skia::LineCap::Butt,
            },
            line_join: match join {
                Some("Round") => tiny_skia::LineJoin::Round,
                Some("Bevel") => tiny_skia::LineJoin::Bevel,
                _ => tiny_skia::LineJoin::Miter,
            },
            miter_limit: miter as f32,
            dash,
        };
        pixmap.stroke_path(&path, &paint, &stroke_style, transform, None);
    }
}

/// 将紧缩路径数据 `AbbreviatedData` 解析为 tiny-skia 路径。
///
/// 操作符见规范表 36：`S`/`M` 起始/移动、`L` 线段、`Q` 二次贝塞尔、
/// `B` 三次贝塞尔、`A` 椭圆弧（按 9.3.5、表 42 绘制）、`C` 自动闭合。
fn build_path(data: &str) -> Option<tiny_skia::Path> {
    let normalized = data.replace(',', " ");
    let mut tokens = normalized.split_whitespace().peekable();
    let mut pb = PathBuilder::new();
    let mut started = false;
    // 跟踪当前绘制点与子路径起点（弧线、闭合需要）。
    let mut cur = (0.0_f64, 0.0_f64);
    let mut start = (0.0_f64, 0.0_f64);

    // 读取 n 个浮点数；不足则返回 None。
    fn take(
        tokens: &mut std::iter::Peekable<std::str::SplitWhitespace>,
        n: usize,
    ) -> Option<Vec<f64>> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            let t = tokens.next()?;
            v.push(t.parse::<f64>().ok()?);
        }
        Some(v)
    }

    while let Some(tok) = tokens.next() {
        match tok {
            "S" | "M" => {
                if let Some(v) = take(&mut tokens, 2) {
                    pb.move_to(v[0] as f32, v[1] as f32);
                    cur = (v[0], v[1]);
                    start = cur;
                    started = true;
                }
            }
            "L" => {
                if started && let Some(v) = take(&mut tokens, 2) {
                    pb.line_to(v[0] as f32, v[1] as f32);
                    cur = (v[0], v[1]);
                }
            }
            "Q" => {
                if started && let Some(v) = take(&mut tokens, 4) {
                    pb.quad_to(v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32);
                    cur = (v[2], v[3]);
                }
            }
            "B" => {
                if started && let Some(v) = take(&mut tokens, 6) {
                    pb.cubic_to(
                        v[0] as f32,
                        v[1] as f32,
                        v[2] as f32,
                        v[3] as f32,
                        v[4] as f32,
                        v[5] as f32,
                    );
                    cur = (v[4], v[5]);
                }
            }
            "A" => {
                // 椭圆弧：rx ry angle large sweep x y（见表 36、表 42）。
                if started && let Some(v) = take(&mut tokens, 7) {
                    let end = (v[5], v[6]);
                    append_arc(
                        &mut pb,
                        cur,
                        v[0],
                        v[1],
                        v[2],
                        v[3] != 0.0,
                        v[4] != 0.0,
                        end,
                    );
                    cur = end;
                }
            }
            "C" => {
                if started {
                    pb.close();
                    cur = start;
                }
            }
            _ => {}
        }
    }

    pb.finish()
}

/// 将一段椭圆弧追加到路径，按规范 9.3.5、表 42 的端点参数化绘制。
///
/// 入参对应紧缩操作符 `A` 的操作数：`rx`/`ry` 为椭圆长短轴半径，
/// `angle_deg` 为椭圆在当前坐标系下的旋转角（度，正值顺时针），
/// `large_arc` 为是否取大于 180° 的弧，`sweep` 为是否顺时针由起点扫向终点。
/// 采用 SVG 端点参数化算法换算出中心参数，再以不超过 90° 的三次贝塞尔逼近。
// 入参与紧缩操作符 `A` 的操作数一一对应，保持平铺以贴合规范表述。
#[allow(clippy::too_many_arguments)]
fn append_arc(
    pb: &mut PathBuilder,
    start: (f64, f64),
    rx: f64,
    ry: f64,
    angle_deg: f64,
    large_arc: bool,
    sweep: bool,
    end: (f64, f64),
) {
    let (x1, y1) = start;
    let (x2, y2) = end;

    // 退化处理（表 42 异常处理）：半径含 0 或起讫点重合 → 直线段。
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx == 0.0 || ry == 0.0 || (x1 == x2 && y1 == y2) {
        pb.line_to(x2 as f32, y2 as f32);
        return;
    }

    // 角度对 360 取模后转弧度；OFD 正角为顺时针，与 y 轴向下的屏幕系一致。
    let phi = (angle_deg % 360.0).to_radians();
    let (sin_p, cos_p) = phi.sin_cos();

    // 步骤 1：把端点差转换到未旋转的椭圆坐标系。
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_p * dx + sin_p * dy;
    let y1p = -sin_p * dx + cos_p * dy;

    // 步骤 2：必要时按比例放大半径，保证椭圆能容纳两端点。
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    // 步骤 3：求旋转坐标系下的椭圆中心。
    let num = (rx * rx) * (ry * ry) - (rx * rx) * (y1p * y1p) - (ry * ry) * (x1p * x1p);
    let den = (rx * rx) * (y1p * y1p) + (ry * ry) * (x1p * x1p);
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large_arc == sweep {
        coef = -coef;
    }
    let cxp = coef * (rx * y1p) / ry;
    let cyp = -coef * (ry * x1p) / rx;

    // 步骤 4：换回原坐标系的中心。
    let cx = cos_p * cxp - sin_p * cyp + (x1 + x2) / 2.0;
    let cy = sin_p * cxp + cos_p * cyp + (y1 + y2) / 2.0;

    // 步骤 5：求起始角与扫掠角。
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut dtheta = angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * std::f64::consts::PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * std::f64::consts::PI;
    }

    // 步骤 6：按 ≤90° 的分段以三次贝塞尔逼近。
    let segments = (dtheta.abs() / (std::f64::consts::PI / 2.0))
        .ceil()
        .max(1.0) as usize;
    let delta = dtheta / segments as f64;
    let t = (4.0 / 3.0) * (delta / 4.0).tan();
    let mut th = theta1;
    // 椭圆参数点及其切向（含旋转）映射到原坐标系。
    let point = |th: f64| -> (f64, f64) {
        let (s, c) = th.sin_cos();
        let ex = rx * c;
        let ey = ry * s;
        (cx + cos_p * ex - sin_p * ey, cy + sin_p * ex + cos_p * ey)
    };
    let deriv = |th: f64| -> (f64, f64) {
        let (s, c) = th.sin_cos();
        let ex = -rx * s;
        let ey = ry * c;
        (cos_p * ex - sin_p * ey, sin_p * ex + cos_p * ey)
    };
    for _ in 0..segments {
        let th2 = th + delta;
        let (px1, py1) = point(th);
        let (px2, py2) = point(th2);
        let (d1x, d1y) = deriv(th);
        let (d2x, d2y) = deriv(th2);
        let c1 = (px1 + t * d1x, py1 + t * d1y);
        let c2 = (px2 - t * d2x, py2 - t * d2y);
        pb.cubic_to(
            c1.0 as f32,
            c1.1 as f32,
            c2.0 as f32,
            c2.1 as f32,
            px2 as f32,
            py2 as f32,
        );
        th = th2;
    }
}

/// 一个待绘制字形：字形索引与其在对象坐标系中的字形原点。
struct PlacedGlyph {
    gid: ttf_parser::GlyphId,
    /// 字形原点（对象坐标系，毫米）。
    origin: (f64, f64),
    /// 由 `DeltaX`/`DeltaY` 给出的本字形可占宽度（毫米）；字形与字符非一一对应
    /// 或该字形位于一段文字末尾时为 `None`。
    advance_limit: Option<f64>,
}

/// 求字形的横向压缩系数，使其字宽不超过 `DeltaX` 给出的字位宽度。
///
/// 文字对象的字位由 `DeltaX`/`DeltaY` 逐字给定，其数值是按**文档所引字型**的字宽
/// 排布的。该字型未内嵌又未安装时只能以系统字型替代，若替代字型的字宽更大（典型
/// 情形：文档按半角字宽 0.5em 排布 ASCII，替代字型的拉丁字形却是比例字宽），字形
/// 就会溢出字位、与后一个字重叠。此时按字位宽度横向压缩该字形。
///
/// 仅在字型被替代时生效：使用内嵌字型时字宽本就与 `DeltaX` 相符，字宽大于字位则
/// 是文档有意为之（如叠印），不应改动。只压不放，故不影响有意拉开的字间距。
fn squeeze_to_advance(
    face: &ttf_parser::Face,
    glyph: &PlacedGlyph,
    scale_x: f64,
    substituted: bool,
) -> f64 {
    if !substituted {
        return 1.0;
    }
    let Some(limit) = glyph.advance_limit.filter(|l| *l > 0.0) else {
        return 1.0;
    };
    let advance = face.glyph_hor_advance(glyph.gid).unwrap_or(0) as f64 * scale_x;
    if advance > limit {
        limit / advance
    } else {
        1.0
    }
}

/// 绘制文字对象：从字型取出字形轮廓，按 `Fill`/`Stroke` 填充或勾边。
///
/// 字形索引的取得遵循规范 11.4：默认按字型 CMAP 表由字符映射；若文字对象带有
/// `CGTransform`（字形变换），则在其覆盖的字符区间内改用显式给出的字形索引，
/// 从而支持一对一、多对一、一对多、多对多等映射关系。
fn render_text(
    pixmap: &mut Pixmap,
    res: &DocResources,
    page_to_device: Mat,
    obj: &TextObject,
    inherited: Option<&CtDrawParam>,
) {
    let Some(font) = res.fonts.get(&obj.font.value()) else {
        return;
    };
    let Some(data) = &font.data else {
        // 无内嵌字型文件，无法绘制轮廓。
        return;
    };
    let Ok(face) = ttf_parser::Face::parse(data, font.index) else {
        return;
    };
    let units_per_em = face.units_per_em() as f64;
    if units_per_em <= 0.0 {
        return;
    }

    let dp = effective_draw_param(res, obj.draw_param, inherited);
    let dp = dp.as_ref();
    // 规范表 45：Fill 默认 true、Stroke 默认 false。两者皆否则无需绘制。
    let fill = obj.fill.unwrap_or(true);
    let stroke = obj.stroke.unwrap_or(false);
    if !fill && !stroke {
        return;
    }

    let h_scale = obj.h_scale.unwrap_or(1.0);
    let scale_x = obj.size / units_per_em * h_scale;
    let scale_y = obj.size / units_per_em;
    // 字符方向：每个字形绕其原点顺时针旋转该角度（见 11.3、表 47）。
    let char_dir = obj.char_direction.unwrap_or(0) as f64;

    let transform = object_to_device(
        page_to_device,
        &obj.boundary,
        obj.ctm.as_ref().map(|a| a.as_slice()),
    )
    .to_skia();

    let placed = place_glyphs(obj, |ch| face.glyph_index(ch));

    let mut pb = PathBuilder::new();
    for g in &placed {
        // 替代字型的字宽与原字型不一致时，按 DeltaX 给出的字位宽度横向压缩，
        // 避免字形互相重叠（见 [`squeeze_to_advance`]）。
        let gx = scale_x * squeeze_to_advance(&face, g, scale_x, font.substituted);
        // 字体单位（y 向上）→ 对象坐标（y 向下）：缩放并翻转 y，
        // 叠加字符方向旋转，最后平移到字形原点。
        let glyph_mat = glyph_to_object(g.origin.0, g.origin.1, gx, scale_y, char_dir);
        let mut outliner = Outliner {
            pb: &mut pb,
            m: glyph_mat,
        };
        face.outline_glyph(g.gid, &mut outliner);
    }

    let Some(path) = pb.finish() else {
        return;
    };

    if fill {
        let color = obj
            .fill_color
            .as_ref()
            .or_else(|| dp.and_then(|d| d.fill_color.as_ref()))
            .map(|c| resolve_color(res, c, obj.alpha))
            .unwrap_or([0, 0, 0, alpha_or_opaque(obj.alpha)]);
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    if stroke {
        // 勾边色默认透明（表 45）；透明时无可见笔迹，跳过以省去描边。
        let Some(color) = obj
            .stroke_color
            .as_ref()
            .or_else(|| dp.and_then(|d| d.stroke_color.as_ref()))
            .map(|c| resolve_color(res, c, obj.alpha))
            .filter(|c| c[3] > 0)
        else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        // 文字对象自身无线宽属性，沿用绘制参数线宽，缺省回退规范默认（表 34）。
        let width = dp.and_then(|d| d.line_width).unwrap_or(0.353);
        let stroke_style = Stroke {
            width: width.max(f64::MIN_POSITIVE) as f32,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke_style, transform, None);
    }
}

/// 字符流中一个字符的落笔信息。
#[derive(Debug, Clone, Copy, PartialEq)]
struct PenStep {
    /// 绘制点（对象坐标系，毫米）。
    pos: (f64, f64),
    /// 到同一 `TextCode` 内下一字符的笔步长（毫米），即本字符可占的宽度；
    /// 该字符为本段最后一个时为 `None`。
    advance: Option<f64>,
}

/// 按规范表 46 求出文字对象内每个字符的落笔信息（对象坐标系，毫米）。
///
/// 首字符取 `X`/`Y`，其后按 `DeltaX`/`DeltaY` 在对象 X、Y 轴上累加；`X`/`Y`
/// 缺省时沿用上一个 `TextCode` 的起绘坐标。返回与字符流一一对应的序列。
fn text_code_points(obj: &TextObject) -> Vec<PenStep> {
    let mut steps: Vec<PenStep> = Vec::new();
    let mut inherited_x = 0.0_f64;
    let mut inherited_y = 0.0_f64;
    for tc in &obj.text_codes {
        let Some(text) = &tc.text else { continue };
        let start_x = tc.x.unwrap_or(inherited_x);
        let start_y = tc.y.unwrap_or(inherited_y);
        inherited_x = start_x;
        inherited_y = start_y;
        let dx = tc.delta_x.as_deref().map(parse_deltas).unwrap_or_default();
        let dy = tc.delta_y.as_deref().map(parse_deltas).unwrap_or_default();
        // 表 46：增量个数少于字符数时，末位增量重复适用于其后各字符。
        let step = |deltas: &[f64], k: usize| -> f64 {
            deltas
                .get(k)
                .copied()
                .unwrap_or_else(|| deltas.last().copied().unwrap_or(0.0))
        };
        let count = text.chars().count();
        let (mut cx, mut cy) = (start_x, start_y);
        for i in 0..count {
            if i > 0 {
                cx += step(&dx, i - 1);
                cy += step(&dy, i - 1);
            }
            let advance = (i + 1 < count).then(|| step(&dx, i).hypot(step(&dy, i)));
            steps.push(PenStep {
                pos: (cx, cy),
                advance,
            });
        }
    }
    steps
}

/// 计算文字对象内全部字形的绘制点与字形索引（对象坐标系，毫米）。
///
/// 先按规范表 46 求出每个字符的绘制点，再按规范 11.4 应用 `CGTransform`：被
/// 字形变换覆盖的字符区间改用显式字形索引，其余字符经 `cmap` 由字符映射。
/// `cmap` 通常为字型 CMAP 查表，抽象为闭包以便脱离具体字型测试定位逻辑。
fn place_glyphs(
    obj: &TextObject,
    cmap: impl Fn(char) -> Option<ttf_parser::GlyphId>,
) -> Vec<PlacedGlyph> {
    let points = text_code_points(obj);
    let chars: Vec<char> = obj
        .text_codes
        .iter()
        .filter_map(|tc| tc.text.as_deref())
        .flat_map(|t| t.chars())
        .collect();

    // 以 CodePosition 为键索引字形变换，逐字符发射字形。
    let transforms: HashMap<usize, &CtCgTransform> = obj
        .cg_transforms
        .iter()
        .filter(|t| t.code_position >= 0)
        .map(|t| (t.code_position as usize, t))
        .collect();

    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(t) = transforms.get(&i) {
            // 字形变换覆盖区间 [i, i+code_count)，整体对应 glyphs 列表。
            let code_count = t.code_count.unwrap_or(1).max(1) as usize;
            if let Some(glyphs) = &t.glyphs {
                for (j, &g) in glyphs.as_slice().iter().enumerate() {
                    // 字形较字符多时（一对多），多出的字形并置在区间末字符的
                    // 绘制点；字形较字符少时（多对一）则只用前若干个绘制点。
                    let pi = i + j.min(code_count.saturating_sub(1));
                    if let Some(&p) = points.get(pi) {
                        out.push(PlacedGlyph {
                            gid: ttf_parser::GlyphId(g as u16),
                            origin: p.pos,
                            // 字形与字符非一一对应，单个字形可占的宽度无从谈起。
                            advance_limit: None,
                        });
                    }
                }
            }
            i += code_count;
        } else {
            // 无字形变换：按 CMAP 由字符取字形索引（规范 11.4.2 一对一）。
            if let Some(gid) = cmap(chars[i])
                && let Some(&p) = points.get(i)
            {
                out.push(PlacedGlyph {
                    gid,
                    origin: p.pos,
                    advance_limit: p.advance,
                });
            }
            i += 1;
        }
    }
    out
}

/// 计算单个字形的“字体单位 → 对象坐标系”变换矩阵。
///
/// 字体坐标 y 轴向上、对象坐标 y 轴向下，故先按字号缩放并翻转 y；
/// 再按字符方向 `char_dir_deg`（度，顺时针）绕原点旋转；最后平移到字形
/// 原点 `(cx, cy)`。`scale_x` 已含 `HScale` 横向缩放。
fn glyph_to_object(cx: f64, cy: f64, scale_x: f64, scale_y: f64, char_dir_deg: f64) -> Mat {
    Mat::translate(cx, cy)
        .mul(Mat::rotate(char_dir_deg.to_radians()))
        .mul(Mat::scale(scale_x, -scale_y))
}

/// 将字型轮廓（字体单位）按给定矩阵映射到对象坐标系路径并写入 `pb`。
struct Outliner<'a> {
    pb: &'a mut PathBuilder,
    m: Mat,
}

impl Outliner<'_> {
    /// 将一个字体单位坐标点经矩阵映射到对象坐标系。
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let (xf, yf) = (x as f64, y as f64);
        (
            (self.m.a * xf + self.m.c * yf + self.m.e) as f32,
            (self.m.b * xf + self.m.d * yf + self.m.f) as f32,
        )
    }
}

impl ttf_parser::OutlineBuilder for Outliner<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.pb.move_to(px, py);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.pb.line_to(px, py);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (cx1, cy1) = self.map(x1, y1);
        let (px, py) = self.map(x, y);
        self.pb.quad_to(cx1, cy1, px, py);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (cx1, cy1) = self.map(x1, y1);
        let (cx2, cy2) = self.map(x2, y2);
        let (px, py) = self.map(x, y);
        self.pb.cubic_to(cx1, cy1, cx2, cy2, px, py);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// 绘制图像对象。
fn render_image(pixmap: &mut Pixmap, res: &DocResources, page_to_device: Mat, obj: &ImageObject) {
    let Some(bytes) = res.media.get(&obj.resource_id.value()) else {
        return;
    };
    let Ok(decoded) = image::load_from_memory(bytes) else {
        return;
    };
    let rgba = decoded.to_rgba8();
    let (iw, ih) = (rgba.width(), rgba.height());
    if iw == 0 || ih == 0 {
        return;
    }
    let Some(src) = rgba_to_pixmap(&rgba) else {
        return;
    };

    // 图像在自身坐标系中铺满单位正方形 [0,1]×[0,1]。
    // 有 CTM 时由 CTM 映射；无 CTM 时按边界尺寸缩放。
    let unit_obj = obj
        .ctm
        .as_ref()
        .and_then(|a| Mat::from_array(a.as_slice()))
        .unwrap_or_else(|| Mat::scale(obj.boundary.width, obj.boundary.height));
    let unit_to_device = page_to_device
        .mul(Mat::translate(obj.boundary.x, obj.boundary.y))
        .mul(unit_obj);
    // 源图像像素 → 设备像素。
    let pixel_to_device = unit_to_device.mul(Mat::scale(1.0 / iw as f64, 1.0 / ih as f64));

    let opacity = obj.alpha.map(|a| a as f32 / 255.0).unwrap_or(1.0);
    let paint = PixmapPaint {
        opacity,
        ..Default::default()
    };
    pixmap.draw_pixmap(0, 0, src.as_ref(), &paint, pixel_to_device.to_skia(), None);
}

/// 将 `image` 的 RGBA 缓冲转换为 tiny-skia 像素图（预乘 alpha）。
fn rgba_to_pixmap(img: &RgbaImage) -> Option<Pixmap> {
    let mut pm = Pixmap::new(img.width(), img.height())?;
    let dst = pm.data_mut();
    for (i, px) in img.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        let af = a as u16;
        dst[i * 4] = (r as u16 * af / 255) as u8;
        dst[i * 4 + 1] = (g as u16 * af / 255) as u8;
        dst[i * 4 + 2] = (b as u16 * af / 255) as u8;
        dst[i * 4 + 3] = a;
    }
    Some(pm)
}

/// 将渲染结果像素图转换为 `image` 的 RGBA 图像（去预乘）。
fn pixmap_to_image(pixmap: &Pixmap) -> RgbaImage {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = RgbaImage::new(w, h);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let c = px.demultiply();
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        out.put_pixel(x, y, image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]));
    }
    out
}

/// 编码图像为指定格式的字节。JPEG 不支持透明通道，编码前转为 RGB。
fn encode_image(img: RgbaImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            DynamicImage::ImageRgba8(img)
                .to_rgb8()
                .write_to(&mut buf, format)?;
        }
        _ => {
            img.write_to(&mut buf, format)?;
        }
    }
    Ok(buf.into_inner())
}

/// 由文件路径扩展名推断图片格式。
fn format_from_path(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    image_format_from_ext(&ext)
}

/// 由扩展名/格式名（不含点，大小写不敏感）映射到 [`ImageFormat`]。
pub fn image_format_from_ext(ext: &str) -> Option<ImageFormat> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// 透明度：给定值或默认不透明（255）。
fn alpha_or_opaque(alpha: Option<u8>) -> u8 {
    alpha.unwrap_or(255)
}

/// 依颜色空间将 [`CtColor`] 解析为 RGBA。
///
/// `obj_alpha` 为图元对象级透明度，将与颜色自身透明度相乘。
fn resolve_color(res: &DocResources, color: &CtColor, obj_alpha: Option<u8>) -> Rgba {
    let values: Vec<f64> = color
        .value
        .as_ref()
        .map(|v| v.as_slice().to_vec())
        .unwrap_or_default();

    let cs_id = color.color_space.map(|c| c.value()).or(res.default_cs);
    let cs = cs_id.and_then(|id| res.color_spaces.get(&id));
    let bits = cs.and_then(|c| c.bits_per_component).unwrap_or(8);
    let max = ((1u64 << bits.min(16)) - 1) as f64;
    let cs_type = cs.map(|c| c.cs_type.as_str()).unwrap_or("RGB");

    let norm = |i: usize| -> f64 { values.get(i).copied().unwrap_or(0.0) / max };

    let (r, g, b) = if values.is_empty() {
        (0.0, 0.0, 0.0)
    } else {
        match cs_type {
            "Gray" => {
                let v = norm(0);
                (v, v, v)
            }
            "CMYK" => {
                let (c, m, y, k) = (norm(0), norm(1), norm(2), norm(3));
                (
                    (1.0 - c) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
            // RGB 及未知类型按 RGB 处理。
            _ => (norm(0), norm(1), norm(2)),
        }
    };

    let color_alpha = color.alpha.unwrap_or(255) as f64;
    let obj_a = obj_alpha.unwrap_or(255) as f64;
    let a = (color_alpha * obj_a / 255.0).round() as u8;

    [
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        a,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::graphics::CtColor;
    use crate::types::StId;

    fn dp(id: u64, relative: Option<u64>) -> CtDrawParam {
        CtDrawParam {
            id: StId(id),
            relative: relative.map(StId),
            ..Default::default()
        }
    }

    fn color(component: f64) -> CtColor {
        CtColor {
            value: Some(StArray(vec![component])),
            ..Default::default()
        }
    }

    fn resources_with(params: Vec<CtDrawParam>) -> DocResources {
        let mut res = DocResources::default();
        for p in params {
            res.draw_params.insert(p.id.value(), p);
        }
        res
    }

    /// 测试辅助：按标识解析展平后的绘制参数。
    fn resolve_for(res: &DocResources, id: u64) -> CtDrawParam {
        resolve_draw_param(res, Some(StRefId(id))).expect("draw param exists")
    }

    /// 8.2 表 24：子绘制参数未显式设置的属性应沿 `@Relative` 链回退到父参数，
    /// 而子参数已设置的属性优先保留。
    #[test]
    fn draw_param_inherits_missing_attrs_via_relative() {
        // 父：定义填充色与线宽；子：仅覆盖勾边色，引用父。
        let mut parent = dp(1, None);
        parent.fill_color = Some(color(0.5));
        parent.line_width = Some(2.0);
        let mut child = dp(2, Some(1));
        child.stroke_color = Some(color(0.9));

        let res = resources_with(vec![parent, child]);
        let flat = resolve_for(&res, 2);
        // 子未设置填充色/线宽：回退父值；勾边色保留子值。
        assert!(flat.fill_color.is_some());
        assert_eq!(flat.line_width, Some(2.0));
        assert!(flat.stroke_color.is_some());
    }

    /// 多级链 3→2→1：属性应跨越中间节点回退到最远祖先。
    #[test]
    fn draw_param_inherits_across_multiple_levels() {
        let mut grandparent = dp(1, None);
        grandparent.fill_color = Some(color(0.5));
        let parent = dp(2, Some(1));
        let parent_id = parent.id.value();
        assert_eq!(parent_id, 2);
        let child = dp(3, Some(2));

        let res = resources_with(vec![grandparent, parent, child]);
        let flat = resolve_for(&res, 3);
        assert!(flat.fill_color.is_some());
    }

    /// 循环引用（2→1→2）不应导致无限递归。
    #[test]
    fn draw_param_relative_cycle_terminates() {
        let res = resources_with(vec![dp(1, Some(2)), dp(2, Some(1))]);
        let flat = resolve_for(&res, 2);
        assert_eq!(flat.id.value(), 2);
    }

    /// 表 14：图元未引用绘制参数时，应继承图层（`inherited`）的填充色。
    /// 复现票样中“文字落在 DrawParam=4 的图层、自身不带颜色”导致渲染成黑色的缺陷。
    #[test]
    fn effective_draw_param_falls_back_to_layer() {
        let mut layer = dp(4, None);
        layer.fill_color = Some(color(0.6));
        let res = resources_with(vec![layer.clone()]);

        // 图元无自身绘制参数：应取图层填充色。
        let eff = effective_draw_param(&res, None, Some(&layer)).expect("inherited param");
        assert!(eff.fill_color.is_some());
    }

    /// 图元自身绘制参数优先于图层绘制参数（子覆盖父）。
    #[test]
    fn effective_draw_param_object_overrides_layer() {
        let mut layer = dp(4, None);
        layer.fill_color = Some(color(0.6));
        let mut own = dp(2, None);
        own.fill_color = Some(color(0.1));
        own.line_width = None; // 线宽未设置：应回退图层。
        let mut layer_with_lw = layer.clone();
        layer_with_lw.line_width = Some(3.0);

        let res = resources_with(vec![own.clone(), layer_with_lw.clone()]);
        let eff =
            effective_draw_param(&res, Some(StRefId(2)), Some(&layer_with_lw)).expect("param");
        // 填充色取图元自身（0.1）而非图层（0.6）。
        assert_eq!(
            eff.fill_color
                .as_ref()
                .and_then(|c| c.value.as_ref())
                .map(|v| v.as_slice()[0]),
            Some(0.1)
        );
        // 线宽图元未设置：回退图层值。
        assert_eq!(eff.line_width, Some(3.0));
    }

    /// 紧缩数据中的 `A` 应绘制真正的椭圆弧而非直线弦：半圆弧的包围盒
    /// 必须向弦的一侧鼓出约一个半径，且 `SweepDirection` 决定鼓出方向
    /// （验证规范 9.3.5、表 42 的弧线绘制）。
    #[test]
    fn arc_bulges_like_a_semicircle() {
        // 由 (0,0) 经半径 5 的半圆到 (10,0)。横向跨度≈直径 10、鼓出≈半径 5。
        // sweep=1 为顺时针：y 轴向下时 9→12→3 点方向，鼓向 -y（上方）。
        let cw = build_path("S 0 0 A 5 5 0 1 1 10 0").expect("path");
        let b = cw.bounds();
        assert!(
            b.top() < -4.5,
            "clockwise semicircle should bulge up (-y), got {}",
            b.top()
        );
        assert!(b.bottom() < 0.5, "stays on one side of the chord");
        assert!(
            (b.left()).abs() < 0.5 && (b.right() - 10.0).abs() < 0.5,
            "span ≈ diameter"
        );
        assert!((b.height() - 5.0).abs() < 0.5, "bulge ≈ radius");

        // sweep=0 为逆时针：应鼓向相反一侧（+y，下方）。
        let ccw = build_path("S 0 0 A 5 5 0 1 0 10 0").expect("path");
        let b2 = ccw.bounds();
        assert!(
            b2.bottom() > 4.5,
            "counter-clockwise should bulge down (+y), got {}",
            b2.bottom()
        );
    }

    /// 半径为 0 时按表 42 退化为直线段，不产生鼓出。
    #[test]
    fn arc_with_zero_radius_degenerates_to_line() {
        let path = build_path("S 0 0 A 0 0 0 0 0 10 0").expect("path");
        let b = path.bounds();
        assert!(b.height() < 0.001, "degenerate arc must be flat");
    }

    /// 经字形变换后的对象坐标点（用于断言旋转方向）。
    fn glyph_pt(cx: f64, cy: f64, scale: f64, char_dir: f64, fx: f64, fy: f64) -> (f64, f64) {
        let m = glyph_to_object(cx, cy, scale, scale, char_dir);
        (m.a * fx + m.c * fy + m.e, m.b * fx + m.d * fy + m.f)
    }

    /// `CharDirection` 应按规范表 47 顺时针旋转字形：字体坐标中“向上”的
    /// 点（+y）在 0/90/180/270 下分别落到对象坐标的 上/右/下/左。
    #[test]
    fn char_direction_rotates_glyph_clockwise() {
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // 字体单位 (0, 1)：上方一个单位。缩放 1，原点 (0,0)。
        // 对象坐标 y 向下：CharDir=0 → 该点在上方 (0,-1)。
        let (x, y) = glyph_pt(0.0, 0.0, 1.0, 0.0, 0.0, 1.0);
        assert!(approx(x, 0.0) && approx(y, -1.0), "0° up→up: {x},{y}");
        // 顺时针 90° → 指向右方 (+1, 0)。
        let (x, y) = glyph_pt(0.0, 0.0, 1.0, 90.0, 0.0, 1.0);
        assert!(approx(x, 1.0) && approx(y, 0.0), "90° up→right: {x},{y}");
        // 180° → 指向下方 (0, +1)。
        let (x, y) = glyph_pt(0.0, 0.0, 1.0, 180.0, 0.0, 1.0);
        assert!(approx(x, 0.0) && approx(y, 1.0), "180° up→down: {x},{y}");
        // 270° → 指向左方 (-1, 0)。
        let (x, y) = glyph_pt(0.0, 0.0, 1.0, 270.0, 0.0, 1.0);
        assert!(approx(x, -1.0) && approx(y, 0.0), "270° up→left: {x},{y}");
    }

    /// 字形原点平移正确：变换后的字体原点 (0,0) 落在 (cx, cy)。
    #[test]
    fn glyph_origin_translates_to_pen_position() {
        let (x, y) = glyph_pt(12.0, 34.0, 2.0, 90.0, 0.0, 0.0);
        assert!((x - 12.0).abs() < 1e-9 && (y - 34.0).abs() < 1e-9);
    }

    use crate::model::graphics::{CtCgTransform, TextCode, TextObject};
    use crate::types::StArray;

    fn text_code(x: Option<f64>, y: Option<f64>, dx: &str, text: &str) -> TextCode {
        TextCode {
            x,
            y,
            delta_x: (!dx.is_empty()).then(|| dx.to_string()),
            delta_y: None,
            text: Some(text.to_string()),
        }
    }

    /// 表 46：首字符取 X/Y，其后沿对象 X 轴按 DeltaX 累加绘制点。
    #[test]
    fn text_points_advance_by_delta_x() {
        let obj = TextObject {
            text_codes: vec![text_code(Some(0.0), Some(25.0), "10 10", "ABC")],
            ..Default::default()
        };
        let pts = text_code_points(&obj);
        let pos: Vec<_> = pts.iter().map(|p| p.pos).collect();
        assert_eq!(pos, vec![(0.0, 25.0), (10.0, 25.0), (20.0, 25.0)]);
        // 末字符之后无笔步长；其余字符可占宽度即到下一字符的步长。
        let adv: Vec<_> = pts.iter().map(|p| p.advance).collect();
        assert_eq!(adv, vec![Some(10.0), Some(10.0), None]);
    }

    /// 表 46：后续 TextCode 省略 X/Y 时沿用上一个 TextCode 的起绘坐标。
    #[test]
    fn text_points_inherit_xy_from_previous_text_code() {
        let obj = TextObject {
            text_codes: vec![
                text_code(Some(5.0), Some(7.0), "", "A"),
                // 省略 X/Y：应沿用上一段的起绘 (5,7) 而非回退到 0。
                text_code(None, None, "", "B"),
            ],
            ..Default::default()
        };
        let pts = text_code_points(&obj);
        let pos: Vec<_> = pts.iter().map(|p| p.pos).collect();
        assert_eq!(pos, vec![(5.0, 7.0), (5.0, 7.0)]);
    }

    /// 11.4 一对一：无字形变换时按 CMAP 逐字符取字形，定位于各自绘制点。
    #[test]
    fn place_glyphs_uses_cmap_without_transform() {
        let obj = TextObject {
            text_codes: vec![text_code(Some(0.0), Some(0.0), "10", "AB")],
            ..Default::default()
        };
        // 桩 CMAP：字符的码位即字形索引。
        let placed = place_glyphs(&obj, |ch| Some(ttf_parser::GlyphId(ch as u16)));
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].gid.0, b'A' as u16);
        assert_eq!(placed[0].origin, (0.0, 0.0));
        assert_eq!(placed[1].gid.0, b'B' as u16);
        assert_eq!(placed[1].origin, (10.0, 0.0));
    }

    /// 11.4 多对一：两个字符经字形变换映射为单个连字字形，落在首字符绘制点；
    /// CMAP 不应被调用（验证显式字形索引优先于按字符查表）。
    #[test]
    fn place_glyphs_applies_ligature_transform() {
        let obj = TextObject {
            text_codes: vec![text_code(Some(0.0), Some(0.0), "10", "fi")],
            cg_transforms: vec![CtCgTransform {
                code_position: 0,
                code_count: Some(2),
                glyph_count: Some(1),
                glyphs: Some(StArray(vec![192])),
            }],
            ..Default::default()
        };
        let placed = place_glyphs(&obj, |_| {
            panic!("CMAP must not be used inside transform range")
        });
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].gid.0, 192);
        assert_eq!(placed[0].origin, (0.0, 0.0));
    }

    /// 11.4：字形变换只覆盖部分字符，区间外的字符仍按 CMAP 处理。
    #[test]
    fn place_glyphs_mixes_transform_and_cmap() {
        let obj = TextObject {
            // 三个字符，绘制点 0/10/20；前两个被连字变换覆盖，第三个走 CMAP。
            text_codes: vec![text_code(Some(0.0), Some(0.0), "10 10", "fix")],
            cg_transforms: vec![CtCgTransform {
                code_position: 0,
                code_count: Some(2),
                glyph_count: Some(1),
                glyphs: Some(StArray(vec![192])),
            }],
            ..Default::default()
        };
        let placed = place_glyphs(&obj, |ch| Some(ttf_parser::GlyphId(ch as u16)));
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].gid.0, 192);
        assert_eq!(placed[0].origin, (0.0, 0.0));
        // 第三个字符 'x' 落在其自身绘制点 (20,0)。
        assert_eq!(placed[1].gid.0, b'x' as u16);
        assert_eq!(placed[1].origin, (20.0, 0.0));
    }

    // —— 紧缩路径解析 build_path ——

    #[test]
    fn build_path_handles_all_commands() {
        // S/M 起点、L 线段、Q 二次、B 三次、A 弧、C 闭合；逗号亦作分隔。
        let p = build_path("M 0,0 L 10 0 Q 10 5 5 10 B 4 4 2 2 0 10 A 3 3 0 0 1 0 0 C");
        assert!(p.is_some());
    }

    #[test]
    fn build_path_edge_cases() {
        // 无任何移动起点 → 空路径。
        assert!(build_path("L 1 1").is_none());
        // 操作数不足 → 该段被忽略，最终仍只有 move，finish 返回 None（无线段）。
        assert!(build_path("M 0 0 L 1").is_none());
        // 未知操作符被跳过。
        assert!(build_path("M 0 0 Z 9 L 5 5").is_some());
        // 空串。
        assert!(build_path("").is_none());
    }

    // —— 颜色解析 resolve_color ——

    fn cs(id: u64, ty: &str, bits: Option<u32>) -> CtColorSpace {
        CtColorSpace {
            id: StId(id),
            cs_type: ty.to_string(),
            bits_per_component: bits,
            profile: None,
        }
    }

    fn res_with_cs(spaces: Vec<CtColorSpace>, default_cs: Option<u64>) -> DocResources {
        let mut res = DocResources {
            default_cs,
            ..Default::default()
        };
        for c in spaces {
            res.color_spaces.insert(c.id.value(), c);
        }
        res
    }

    fn colored(values: Vec<f64>, cs_id: Option<u64>, alpha: Option<u8>) -> CtColor {
        CtColor {
            value: Some(StArray(values)),
            color_space: cs_id.map(StRefId),
            alpha,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_color_rgb_default_when_no_space() {
        // 无颜色空间 → 按 RGB、8 位处理（分量按 max=255 归一化）。
        let res = DocResources::default();
        let c = resolve_color(&res, &colored(vec![255.0, 0.0, 0.0], None, None), None);
        assert_eq!(c, [255, 0, 0, 255]);
    }

    #[test]
    fn resolve_color_gray_and_bits() {
        // Gray，4 位 → max=15；值 15 → 白。
        let res = res_with_cs(vec![cs(1, "Gray", Some(4))], None);
        let c = resolve_color(&res, &colored(vec![15.0], Some(1), None), None);
        assert_eq!(c, [255, 255, 255, 255]);
    }

    #[test]
    fn resolve_color_cmyk() {
        // 纯黑 K=1 → 黑。
        let res = res_with_cs(vec![cs(2, "CMYK", None)], None);
        let c = resolve_color(
            &res,
            &colored(vec![0.0, 0.0, 0.0, 255.0], Some(2), None),
            None,
        );
        assert_eq!(&c[..3], &[0, 0, 0]);
    }

    #[test]
    fn resolve_color_uses_default_cs_and_alpha_multiplies() {
        // 颜色未指定空间 → 用文档默认颜色空间（Gray）。
        let res = res_with_cs(vec![cs(7, "Gray", None)], Some(7));
        // 对象透明度 128 与颜色透明度 255 相乘 → 128。
        let c = resolve_color(&res, &colored(vec![255.0], None, Some(255)), Some(128));
        assert_eq!(c, [255, 255, 255, 128]);
    }

    #[test]
    fn resolve_color_empty_values_is_black() {
        let res = DocResources::default();
        let c = resolve_color(
            &res,
            &CtColor {
                value: None,
                ..Default::default()
            },
            None,
        );
        assert_eq!(c, [0, 0, 0, 255]);
    }

    // —— 矩阵 Mat ——

    #[test]
    fn mat_from_array_and_ops() {
        assert!(Mat::from_array(&[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).is_some());
        assert!(Mat::from_array(&[1.0, 2.0]).is_none());
        // 平移 ∘ 缩放：点 (1,1) → 缩放 2 → (2,2) → 平移 (10,20) → (12,22)。
        let m = Mat::translate(10.0, 20.0).mul(Mat::scale(2.0, 2.0));
        assert!((m.a - 2.0).abs() < 1e-9 && (m.e - 10.0).abs() < 1e-9);
        let sk = Mat::identity().to_skia();
        assert_eq!(sk.sx, 1.0);
        // 旋转 90°：a≈0。
        let r = Mat::rotate(std::f64::consts::FRAC_PI_2);
        assert!(r.a.abs() < 1e-9);
    }

    // —— 杂项纯函数 ——

    #[test]
    fn is_cjk_and_alpha_helpers() {
        assert!(is_cjk('字'));
        assert!(is_cjk('，')); // 全角标点
        assert!(!is_cjk('A'));
        assert_eq!(alpha_or_opaque(None), 255);
        assert_eq!(alpha_or_opaque(Some(10)), 10);
    }

    #[test]
    fn image_format_mapping() {
        for (ext, _) in [
            ("png", ()),
            ("JPG", ()),
            ("jpeg", ()),
            ("bmp", ()),
            ("tiff", ()),
            ("tif", ()),
            ("gif", ()),
            ("webp", ()),
        ] {
            assert!(image_format_from_ext(ext).is_some(), "{ext}");
        }
        assert!(image_format_from_ext("xyz").is_none());
        assert!(format_from_path(Path::new("a/b.PNG")).is_some());
        assert!(format_from_path(Path::new("noext")).is_none());
    }

    #[test]
    fn lookup_system_font_finds_something() {
        // 取系统字体库中真实存在的某个族名，覆盖“按名称精确匹配成功”分支。
        let db = system_fonts();
        let fam = db
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()));
        if let Some(fam) = fam {
            assert!(lookup_system_font(&fam, "", false, false).is_some());
        }
        // 含中文名走 CJK 兜底分支；空名/未知名走通用无衬线兜底分支（仅求执行到，
        // 是否命中取决于运行环境已装字型，故不强断言）。
        let _ = lookup_system_font("宋体", "宋体", true, false);
        let _ = lookup_system_font("ZZ-No-Such-Font", "", false, true);
        let _ = lookup_system_font("", "", false, false);
    }

    #[test]
    fn pixmap_image_roundtrip_and_encode() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 1, image::Rgba([0, 255, 0, 128]));
        let pm = rgba_to_pixmap(&img).unwrap();
        let back = pixmap_to_image(&pm);
        assert_eq!(back.get_pixel(0, 0).0, [255, 0, 0, 255]);
        // PNG / JPEG 编码各走一条分支。
        assert!(
            !encode_image(img.clone(), ImageFormat::Png)
                .unwrap()
                .is_empty()
        );
        assert!(!encode_image(img, ImageFormat::Jpeg).unwrap().is_empty());
    }

    #[test]
    fn render_options_builders() {
        let o = RenderOptions::with_dpi(300.0).background(Some([1, 2, 3, 4]));
        assert_eq!(o.dpi, 300.0);
        assert_eq!(o.background, Some([1, 2, 3, 4]));
    }
}
