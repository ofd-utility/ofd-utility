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

fn build_ofd() -> Vec<u8> {
    let font = embedded_font_bytes();
    let png = png_bytes();
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let text_files = [
            ("OFD.xml", OFD_XML),
            ("Doc_0/Document.xml", DOCUMENT_XML),
            ("Doc_0/PublicRes.xml", PUBLIC_RES_XML),
            ("Doc_0/Pages/Page_0/Content.xml", PAGE_XML),
            ("Doc_0/Tpls/Tpl_bg/Content.xml", TPL_BG_XML),
            ("Doc_0/Tpls/Tpl_fg/Content.xml", TPL_FG_XML),
            ("Doc_0/Annots/Annotations.xml", ANNOTATIONS_XML),
            ("Doc_0/Annots/Page_0/Annotation.xml", PAGE_ANNOT_XML),
        ];
        for (name, content) in text_files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.start_file("Doc_0/Res/font.ttf", opts).unwrap();
        zip.write_all(&font).unwrap();
        zip.start_file("Doc_0/Res/image.png", opts).unwrap();
        zip.write_all(&png).unwrap();
        zip.finish().unwrap();
    }
    buf
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
