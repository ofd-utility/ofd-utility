//! 渲染管线集成覆盖：在一页中综合路径（填充/勾边/虚线/端点/奇偶规则/CTM）、
//! 文字（内嵌字型 + 系统字型回退、CJK）、图像（内嵌 PNG）、模板（背景/前景）、
//! 页级资源、颜色空间（Gray/CMYK/RGB）与注释外观，并覆盖各错误分支。

use std::io::{Cursor, Write};

use image::{ImageFormat as ImgFmt, RgbaImage};
use ofd_core::render::RenderOptions;
use ofd_core::{OfdPackage, OfdReader};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const OFD_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.0" DocType="OFD">
  <ofd:DocBody>
    <ofd:DocInfo><ofd:Title>render</ofd:Title></ofd:DocInfo>
    <ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>
  </ofd:DocBody>
</ofd:OFD>"#;

const DOCUMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData>
    <ofd:MaxUnitID>1000</ofd:MaxUnitID>
    <ofd:DefaultCS>5</ofd:DefaultCS>
    <ofd:PageArea><ofd:PhysicalBox>0 0 100 100</ofd:PhysicalBox></ofd:PageArea>
    <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>
    <ofd:TemplatePage ID="10" BaseLoc="Tpls/Tpl_bg/Content.xml"/>
    <ofd:TemplatePage ID="11" BaseLoc="Tpls/Tpl_fg/Content.xml"/>
  </ofd:CommonData>
  <ofd:Pages>
    <ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/>
  </ofd:Pages>
  <ofd:Annotations>Annots/Annotations.xml</ofd:Annotations>
</ofd:Document>"#;

const PUBLIC_RES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res">
  <ofd:ColorSpaces>
    <ofd:ColorSpace ID="5" Type="RGB" BitsPerComponent="8"/>
    <ofd:ColorSpace ID="6" Type="CMYK"/>
    <ofd:ColorSpace ID="7" Type="Gray"/>
  </ofd:ColorSpaces>
  <ofd:DrawParams>
    <ofd:DrawParam ID="40" LineWidth="1.0" Cap="Round" Join="Round" DashOffset="0" DashPattern="3 2">
      <ofd:StrokeColor Value="0 128 255" ColorSpace="5"/>
    </ofd:DrawParam>
  </ofd:DrawParams>
  <ofd:Fonts>
    <ofd:Font ID="20" FontName="Embedded" FontFile="font.ttf"/>
    <ofd:Font ID="21" FontName="宋体" FamilyName="SimSun"/>
  </ofd:Fonts>
  <ofd:MultiMedias>
    <ofd:MultiMedia ID="30" Type="Image" Format="PNG"><ofd:MediaFile>image.png</ofd:MediaFile></ofd:MultiMedia>
  </ofd:MultiMedias>
</ofd:Res>"#;

// 页内容：路径（含 DrawParam 虚线/勾边、奇偶填充、CTM）、文字（内嵌/系统字型）、
// 图像（带 CTM 与不带 CTM）、分组 Block。
const PAGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Template TemplateID="10" ZOrder="Background"/>
  <ofd:Template TemplateID="11" ZOrder="Foreground"/>
  <ofd:Content>
    <ofd:Layer ID="100" Type="Foreground">
      <ofd:TextObject ID="201" Boundary="5 5 90 20" Font="20" Size="8" Fill="true">
        <ofd:FillColor Value="255 0 0" ColorSpace="5"/>
        <ofd:TextCode X="0" Y="10" DeltaX="6 6 6">Hi!</ofd:TextCode>
      </ofd:TextObject>
    </ofd:Layer>
    <ofd:Layer ID="101" Type="Body">
      <ofd:PathObject ID="202" Boundary="10 10 50 50" Fill="true" Stroke="true" Rule="Even-Odd" DrawParam="40" CTM="1 0 0 1 0 0">
        <ofd:FillColor Value="0 0 0 255" ColorSpace="6"/>
        <ofd:AbbreviatedData>M 0 0 L 50 0 L 50 50 L 0 50 C</ofd:AbbreviatedData>
      </ofd:PathObject>
      <ofd:PathObject ID="203" Boundary="10 10 30 30" Stroke="true" LineWidth="0.5" Cap="Square" Join="Bevel" DashPattern="2 2" DashOffset="1">
        <ofd:StrokeColor Value="0 200 0" ColorSpace="5"/>
        <ofd:AbbreviatedData>M 0 0 L 30 30</ofd:AbbreviatedData>
      </ofd:PathObject>
      <ofd:TextObject ID="204" Boundary="5 40 90 20" Font="21" Size="6" Stroke="true" CharDirection="90">
        <ofd:StrokeColor Value="50" ColorSpace="7"/>
        <ofd:TextCode X="0" Y="10">中文A</ofd:TextCode>
      </ofd:TextObject>
      <ofd:ImageObject ID="205" Boundary="60 60 30 30" ResourceID="30" Alpha="200"/>
      <ofd:ImageObject ID="206" Boundary="60 10 20 20" ResourceID="30" CTM="20 0 0 20 0 0"/>
      <ofd:PageBlock>
        <ofd:PathObject ID="207" Boundary="70 70 20 20" Fill="true">
          <ofd:AbbreviatedData>M 0 0 L 20 0 L 20 20 C</ofd:AbbreviatedData>
        </ofd:PathObject>
      </ofd:PageBlock>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#;

const TPL_BG_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content><ofd:Layer ID="900" Type="Background">
    <ofd:PathObject ID="901" Boundary="0 0 100 100" Fill="true">
      <ofd:FillColor Value="240 240 240" ColorSpace="5"/>
      <ofd:AbbreviatedData>M 0 0 L 100 0 L 100 100 L 0 100 C</ofd:AbbreviatedData>
    </ofd:PathObject>
  </ofd:Layer></ofd:Content>
</ofd:Page>"#;

const TPL_FG_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content><ofd:Layer ID="910">
    <ofd:PathObject ID="911" Boundary="0 0 10 10" Stroke="true">
      <ofd:AbbreviatedData>M 0 0 L 10 10</ofd:AbbreviatedData>
    </ofd:PathObject>
  </ofd:Layer></ofd:Content>
</ofd:Page>"#;

const ANNOTATIONS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Annotations xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Page PageID="1"><ofd:FileLoc>Page_0/Annotation.xml</ofd:FileLoc></ofd:Page>
  <ofd:Page PageID="999"><ofd:FileLoc>Page_x/none.xml</ofd:FileLoc></ofd:Page>
</ofd:Annotations>"#;

const PAGE_ANNOT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:PageAnnot xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Annot ID="1" Type="Stamp" Visible="true">
    <ofd:Appearance Boundary="20 20 40 40">
      <ofd:PathObject ID="301" Boundary="0 0 40 40" Fill="true">
        <ofd:FillColor Value="0 255 0" ColorSpace="5"/>
        <ofd:AbbreviatedData>M 0 0 L 40 0 L 40 40 C</ofd:AbbreviatedData>
      </ofd:PathObject>
    </ofd:Appearance>
  </ofd:Annot>
  <ofd:Annot ID="2" Visible="false"><ofd:Appearance Boundary="0 0 1 1"/></ofd:Annot>
  <ofd:Annot ID="3"/>
</ofd:PageAnnot>"#;

/// 找一个真实 TTF 文件作为内嵌字型，保证 ttf-parser 解析与字形描边路径被执行。
fn embedded_font_bytes() -> Vec<u8> {
    let candidates = [
        "/usr/share/fonts/liberation-fonts/LiberationMono-Regular.ttf",
        "/usr/share/fonts/liberation-fonts/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation-fonts/LiberationMono-Bold.ttf",
    ];
    for p in candidates {
        if let Ok(b) = std::fs::read(p) {
            return b;
        }
    }
    // 退路：任取一个系统 TTF。
    if let Ok(rd) = std::fs::read_dir("/usr/share/fonts/liberation-fonts") {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "ttf") {
                if let Ok(b) = std::fs::read(e.path()) {
                    return b;
                }
            }
        }
    }
    // 仍找不到：返回非字型字节，覆盖 Face::parse 失败分支。
    b"not a font".to_vec()
}

/// 生成一张 2x2 的 PNG 作为内嵌图像。
fn png_bytes() -> Vec<u8> {
    let mut img = RgbaImage::new(2, 2);
    img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    img.put_pixel(0, 1, image::Rgba([0, 0, 255, 128]));
    img.put_pixel(1, 1, image::Rgba([255, 255, 0, 255]));
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImgFmt::Png).unwrap();
    buf.into_inner()
}

/// 打包一份 OFD：页内容、公共资源文件内容及其数据文件（字型、图像）的包内路径
/// 均可变，以便覆盖「有/无 BaseLoc」的资源文件布局与各种页面图元组合。
fn build_ofd_with(public_res: &str, page: &str, font_path: &str, image_path: &str) -> Vec<u8> {
    let font = embedded_font_bytes();
    let png = png_bytes();
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let text_files = [
            ("OFD.xml", OFD_XML),
            ("Doc_0/Document.xml", DOCUMENT_XML),
            ("Doc_0/PublicRes.xml", public_res),
            ("Doc_0/Pages/Page_0/Content.xml", page),
            ("Doc_0/Tpls/Tpl_bg/Content.xml", TPL_BG_XML),
            ("Doc_0/Tpls/Tpl_fg/Content.xml", TPL_FG_XML),
            ("Doc_0/Annots/Annotations.xml", ANNOTATIONS_XML),
            ("Doc_0/Annots/Page_0/Annotation.xml", PAGE_ANNOT_XML),
        ];
        for (name, content) in text_files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.start_file(font_path, opts).unwrap();
        zip.write_all(&font).unwrap();
        zip.start_file(image_path, opts).unwrap();
        zip.write_all(&png).unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn build_ofd() -> Vec<u8> {
    build_ofd_with(
        PUBLIC_RES_XML,
        PAGE_XML,
        "Doc_0/Res/font.ttf",
        "Doc_0/Res/image.png",
    )
}

fn reader() -> OfdReader<Cursor<Vec<u8>>> {
    OfdReader::new(OfdPackage::new(Cursor::new(build_ofd())).unwrap()).unwrap()
}

fn first_doc(r: &mut OfdReader<Cursor<Vec<u8>>>) -> ofd_core::LoadedDocument {
    let body = r.ofd().doc_bodies[0].clone();
    r.load_document(&body).unwrap()
}

#[test]
fn renders_rich_page_with_all_object_kinds() {
    let mut r = reader();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::with_dpi(96.0);
    let pixmap = r.render_page(&doc, 0, &opts).unwrap();
    assert!(pixmap.width() > 0 && pixmap.height() > 0);
    // 背景模板填充浅灰 + 各类图元 → 应存在非透明像素。
    assert!(
        pixmap.pixels().iter().any(|p| p.alpha() > 0),
        "渲染结果应有可见像素"
    );
}

#[test]
fn renders_with_background_color() {
    let mut r = reader();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::with_dpi(72.0).background(Some([255, 255, 255, 255]));
    let pixmap = r.render_page(&doc, 0, &opts).unwrap();
    // 背景不透明 → 所有像素 alpha 满。
    assert!(pixmap.pixels().iter().all(|p| p.alpha() == 255));
}

#[test]
fn render_to_image_and_bytes_and_file() {
    let mut r = reader();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::with_dpi(72.0);

    let img = r.render_page_to_image(&doc, 0, &opts).unwrap();
    assert!(img.width() > 0);

    let png = r
        .render_page_to_bytes(&doc, 0, &opts, image::ImageFormat::Png)
        .unwrap();
    assert!(!png.is_empty());

    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    for ext in ["png", "jpg", "bmp", "tiff", "gif", "webp"] {
        let out = dir.join(format!("render_out.{ext}"));
        r.render_page_to_file(&doc, 0, &opts, &out).unwrap();
        assert!(out.exists(), "{ext} 应已写出");
    }

    // 无法识别的扩展名 → 错误分支。
    let bad = dir.join("render_out.unknownext");
    assert!(r.render_page_to_file(&doc, 0, &opts, &bad).is_err());
}

#[test]
fn render_page_index_out_of_range_errors() {
    let mut r = reader();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::default();
    assert!(r.render_page(&doc, 99, &opts).is_err());
}

#[test]
fn render_exceeding_max_dimension_errors() {
    let mut r = reader();
    let doc = first_doc(&mut r);
    // 100mm 页在极高 DPI 下超出最大像素限制 → 错误分支。
    let opts = RenderOptions::with_dpi(1_000_000.0);
    assert!(r.render_page(&doc, 0, &opts).is_err());
}

/// 省略 `BaseLoc` 的公共资源文件（`Res` 无 BaseLoc 属性），数据文件与资源文件同目录。
const PUBLIC_RES_NO_BASELOC_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:ColorSpaces>
    <ofd:ColorSpace ID="5" Type="RGB" BitsPerComponent="8"/>
    <ofd:ColorSpace ID="6" Type="CMYK"/>
    <ofd:ColorSpace ID="7" Type="Gray"/>
  </ofd:ColorSpaces>
  <ofd:DrawParams>
    <ofd:DrawParam ID="40" LineWidth="1.0" Cap="Round" Join="Round" DashOffset="0" DashPattern="3 2">
      <ofd:StrokeColor Value="0 128 255" ColorSpace="5"/>
    </ofd:DrawParam>
  </ofd:DrawParams>
  <ofd:Fonts>
    <ofd:Font ID="20" FontName="Embedded"><ofd:FontFile>font.ttf</ofd:FontFile></ofd:Font>
    <ofd:Font ID="21" FontName="宋体" FamilyName="SimSun"/>
  </ofd:Fonts>
  <ofd:MultiMedias>
    <ofd:MultiMedia ID="30" Type="Image" Format="PNG"><ofd:MediaFile>image.png</ofd:MediaFile></ofd:MultiMedia>
  </ofd:MultiMedias>
</ofd:Res>"#;

/// 回归: 苏豪 swformsdk 生成的数电发票 `PublicRes.xml` 省略了 `Res/@BaseLoc`
/// （GB/T 33190 表 18 列为必选）。此前该属性缺失会让整个资源文件反序列化失败并被
/// 静默跳过，字型随之全部缺失，渲染结果只剩线框与图片、文字全无。缺省时应按空
/// 路径处理，即以资源文件自身所在目录为基准。
#[test]
fn renders_text_when_public_res_omits_base_loc() {
    let ofd = build_ofd_with(
        PUBLIC_RES_NO_BASELOC_XML,
        PAGE_XML,
        "Doc_0/font.ttf",
        "Doc_0/image.png",
    );
    let mut r = OfdReader::new(OfdPackage::new(Cursor::new(ofd)).unwrap()).unwrap();
    let doc = first_doc(&mut r);
    let pixmap = r
        .render_page(&doc, 0, &RenderOptions::with_dpi(96.0))
        .unwrap();

    // 页内唯一的纯红图元是使用内嵌字型的 TextObject 201，出现红色像素即证明
    // 字型已按资源文件所在目录解析到并完成字形绘制。
    let red = pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 200 && p.green() < 80 && p.blue() < 80)
        .count();
    assert!(red > 0, "缺少 BaseLoc 时仍应加载内嵌字型并绘出文字");
}

/// 仅含一个「Fill=true + Stroke=true 且未给填充色」的圆环路径（形如发票上的 ⊗
/// 标记），铺满整页便于逐像素判读。
const PAGE_UNCOLORED_FILL_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content><ofd:Layer ID="100">
    <ofd:PathObject ID="210" Boundary="0 0 100 100" Fill="true" Stroke="true" LineWidth="2">
      <ofd:AbbreviatedData>M 10 10 L 90 10 L 90 90 L 10 90 C</ofd:AbbreviatedData>
    </ofd:PathObject>
  </ofd:Layer></ofd:Content>
</ofd:Page>"#;

/// 回归: GB/T 33190 表 26 中路径对象的填充色缺省为**透明色**（文字对象表 45 才是
/// 缺省黑色）。此前未给填充色时按黑色填充，会把「Fill=true + Stroke=true」的勾边
/// 图形整体涂黑 —— 数电发票「价税合计」前的 ⊗ 标记因此渲染成实心黑点。
#[test]
fn path_without_fill_color_is_not_filled_black() {
    let ofd = build_ofd_with(
        PUBLIC_RES_XML,
        PAGE_UNCOLORED_FILL_XML,
        "Doc_0/Res/font.ttf",
        "Doc_0/Res/image.png",
    );
    let mut r = OfdReader::new(OfdPackage::new(Cursor::new(ofd)).unwrap()).unwrap();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::with_dpi(96.0).background(Some([255, 255, 255, 255]));
    let pixmap = r.render_page(&doc, 0, &opts).unwrap();

    // 取方框内 80mm 处：既在框内（10~90mm），又避开 2mm 宽的边线与注释图元
    // （绿色三角形位于 20~60mm），应保持背景白色而非被填成黑色。
    let inside = pixmap
        .pixel(pixmap.width() * 4 / 5, pixmap.height() * 4 / 5)
        .unwrap();
    assert_eq!(
        (inside.red(), inside.green(), inside.blue()),
        (255, 255, 255),
        "未给填充色的路径内部应保持透明"
    );
    // 勾边仍按缺省黑色绘出，证明并非整个图元被跳过。
    assert!(
        pixmap
            .pixels()
            .iter()
            .any(|p| p.red() < 40 && p.alpha() > 0),
        "缺省黑色勾边应可见"
    );
}

/// 仅引用系统字型（不内嵌）的资源文件：`SimHei` 为中文字型的拉丁注册名，
/// `NoSuchFontXYZ` 则是任意不存在的拉丁字型名，两者在测试机上均未安装。
const PUBLIC_RES_MISSING_FONTS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res">
  <ofd:ColorSpaces><ofd:ColorSpace ID="5" Type="RGB" BitsPerComponent="8"/></ofd:ColorSpaces>
  <ofd:Fonts>
    <ofd:Font ID="50" FontName="SimHei" FamilyName="SimHei"/>
    <ofd:Font ID="51" FontName="NoSuchFontXYZ" FamilyName="NoSuchFontXYZ"/>
  </ofd:Fonts>
  <ofd:MultiMedias>
    <ofd:MultiMedia ID="30" Type="Image" Format="PNG"><ofd:MediaFile>image.png</ofd:MediaFile></ofd:MultiMedia>
  </ofd:MultiMedias>
</ofd:Res>"#;

/// 两个文字对象分别引用上述两款缺失字型，各用一种可辨识的纯色。
const PAGE_MISSING_FONTS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content><ofd:Layer ID="100">
    <ofd:TextObject ID="220" Boundary="5 5 90 20" Font="50" Size="10">
      <ofd:FillColor Value="255 0 0" ColorSpace="5"/>
      <ofd:TextCode X="0" Y="12" DeltaX="7 7 7 7 7">Ningbo</ofd:TextCode>
    </ofd:TextObject>
    <ofd:TextObject ID="221" Boundary="5 40 90 20" Font="51" Size="10">
      <ofd:FillColor Value="0 0 255" ColorSpace="5"/>
      <ofd:TextCode X="0" Y="12" DeltaX="7 7 7 7">G756</ofd:TextCode>
    </ofd:TextObject>
  </ofd:Layer></ofd:Content>
</ofd:Page>"#;

/// 回归: 铁路电子客票（数电票）以 `SimHei`、`DengXian`、`KaiTi` 等**拉丁注册名**
/// 引用中文字型且不内嵌字型文件。此前中文兜底仅在字型名含 CJK 字符时才触发，这类
/// 名称便直接落到通用 sans-serif；而 fontdb 的通用族默认指向 Arial 等未必安装的
/// 字型，查询返回 None 后 `render_text` 提前 return，相关文字（票面标题、站名、
/// 车次乃至发票专用章内的文字）整体不可见。缺失字型现在必须始终兜底到某款可用
/// 字型。
#[test]
fn missing_system_fonts_fall_back_instead_of_dropping_text() {
    let ofd = build_ofd_with(
        PUBLIC_RES_MISSING_FONTS_XML,
        PAGE_MISSING_FONTS_XML,
        "Doc_0/Res/font.ttf",
        "Doc_0/Res/image.png",
    );
    let mut r = OfdReader::new(OfdPackage::new(Cursor::new(ofd)).unwrap()).unwrap();
    let doc = first_doc(&mut r);
    let opts = RenderOptions::with_dpi(96.0).background(Some([255, 255, 255, 255]));
    let pixmap = r.render_page(&doc, 0, &opts).unwrap();

    // 中文字型的拉丁名（走中文字型兜底）与任意未知拉丁名（走通用/任意字型兜底）
    // 都应画出字形。
    let (mut red, mut blue) = (0usize, 0usize);
    for p in pixmap.pixels() {
        if p.red() > 150 && p.green() < 100 && p.blue() < 100 {
            red += 1;
        }
        if p.blue() > 150 && p.red() < 100 && p.green() < 100 {
            blue += 1;
        }
    }
    assert!(red > 0, "中文字型拉丁名未安装时应兜底绘出文字");
    assert!(blue > 0, "未知字型名未安装时应兜底绘出文字");
}

/// 字位宽度（DeltaX=1mm）远小于字号（10mm）：替代字型的字宽必然溢出字位。
/// 末尾补一个不占墨的不换行空格（`&#xA0;`，普通空格会被 XML 解析器裁掉），
/// 使四个 `M` 都有字位宽度可依 —— 段末字符没有字位宽度，不参与收紧。
const PAGE_NARROW_ADVANCE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content><ofd:Layer ID="100">
    <ofd:TextObject ID="230" Boundary="0 0 100 20" Font="50" Size="10">
      <ofd:FillColor Value="255 0 0" ColorSpace="5"/>
      <ofd:TextCode X="0" Y="15" DeltaX="1 1 1 1">MMMM&#xA0;</ofd:TextCode>
    </ofd:TextObject>
  </ofd:Layer></ofd:Content>
</ofd:Page>"#;

/// 回归: 铁路电子客票按半角字宽（0.5em）排布 ASCII，而 `SimHei` 未安装时替代到的
/// 系统字型拉丁字形是比例字宽（如 Microsoft YaHei 的 `W` 宽达 1.02em），字形溢出
/// 字位后与后一个字重叠（“Wenzhoubei”糊成一团）。字型被替代时应按 `DeltaX` 给出的
/// 字位宽度横向收紧字形。
#[test]
fn substituted_font_glyphs_are_squeezed_into_delta_x() {
    let ofd = build_ofd_with(
        PUBLIC_RES_MISSING_FONTS_XML,
        PAGE_NARROW_ADVANCE_XML,
        "Doc_0/Res/font.ttf",
        "Doc_0/Res/image.png",
    );
    let mut r = OfdReader::new(OfdPackage::new(Cursor::new(ofd)).unwrap()).unwrap();
    let doc = first_doc(&mut r);
    let dpi = 96.0;
    let opts = RenderOptions::with_dpi(dpi).background(Some([255, 255, 255, 255]));
    let pixmap = r.render_page(&doc, 0, &opts).unwrap();

    let mut max_x = None;
    for (i, p) in pixmap.pixels().iter().enumerate() {
        if p.red() > 150 && p.green() < 100 && p.blue() < 100 {
            let x = i as u32 % pixmap.width();
            max_x = Some(max_x.map_or(x, |m: u32| m.max(x)));
        }
    }
    let max_x = max_x.expect("应绘出文字");
    // 四个字形各收紧到 1mm 字位内，最右墨迹不应超出 4mm（留 2mm 余量给反走样）。
    let limit_px = (6.0 * dpi / 25.4) as u32;
    assert!(
        max_x <= limit_px,
        "字形应收紧到字位内：最右墨迹 {max_x}px 超过 {limit_px}px"
    );
}
