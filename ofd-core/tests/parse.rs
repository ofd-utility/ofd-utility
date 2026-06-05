//! 端到端解析测试：在内存中构造一个最小 OFD 包并完整解析其基础结构。

use std::io::{Cursor, Write};

use ofd_core::render::RenderOptions;
use ofd_core::{OfdPackage, OfdReader};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const OFD_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.0" DocType="OFD">
  <ofd:DocBody>
    <ofd:DocInfo>
      <ofd:DocID>9f3c0e1a2b3c4d5e6f7a8b9c0d1e2f30</ofd:DocID>
      <ofd:Title>测试文档</ofd:Title>
      <ofd:Author>张三</ofd:Author>
      <ofd:Creator>ofd-core</ofd:Creator>
      <ofd:CreationDate>2026-06-01</ofd:CreationDate>
      <ofd:Keywords>
        <ofd:Keyword>OFD</ofd:Keyword>
        <ofd:Keyword>版式</ofd:Keyword>
      </ofd:Keywords>
      <ofd:CustomDatas>
        <ofd:CustomData Name="区域">华东</ofd:CustomData>
      </ofd:CustomDatas>
    </ofd:DocInfo>
    <ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>
  </ofd:DocBody>
</ofd:OFD>"#;

const DOCUMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData>
    <ofd:MaxUnitID>1000</ofd:MaxUnitID>
    <ofd:PageArea>
      <ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox>
      <ofd:ApplicationBox>0 0 210 297</ofd:ApplicationBox>
    </ofd:PageArea>
    <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>
    <ofd:TemplatePage ID="10" Name="bg" BaseLoc="Tpls/Tpl_0/Content.xml"/>
  </ofd:CommonData>
  <ofd:Pages>
    <ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/>
    <ofd:Page ID="2" BaseLoc="Pages/Page_1/Content.xml"/>
  </ofd:Pages>
  <ofd:Permissions>
    <ofd:Edit>false</ofd:Edit>
    <ofd:Print Printable="true" Copies="5"/>
  </ofd:Permissions>
  <ofd:Outlines>
    <ofd:OutlineElem Title="第一章">
      <ofd:OutlineElem Title="第一节"/>
    </ofd:OutlineElem>
  </ofd:Outlines>
</ofd:Document>"#;

const PAGE0_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Template TemplateID="10" ZOrder="Background"/>
  <ofd:Content>
    <ofd:Layer ID="100" Type="Body">
      <ofd:TextObject ID="101" Boundary="10 10 50 20"/>
      <ofd:PathObject ID="102" Boundary="50 50 100 100" Fill="true">
        <ofd:FillColor Value="255 0 0"/>
        <ofd:AbbreviatedData>M 0 0 L 100 0 L 100 100 L 0 100 C</ofd:AbbreviatedData>
      </ofd:PathObject>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#;

const PAGE1_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
</ofd:Page>"#;

const PUBLIC_RES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res">
  <ofd:Fonts>
    <ofd:Font ID="20" FontName="宋体" FamilyName="SimSun"/>
  </ofd:Fonts>
</ofd:Res>"#;

fn build_ofd() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let files = [
            ("OFD.xml", OFD_XML),
            ("Doc_0/Document.xml", DOCUMENT_XML),
            ("Doc_0/Pages/Page_0/Content.xml", PAGE0_XML),
            ("Doc_0/Pages/Page_1/Content.xml", PAGE1_XML),
            ("Doc_0/PublicRes.xml", PUBLIC_RES_XML),
        ];
        for (name, content) in files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

#[test]
fn parse_main_entry() {
    let data = build_ofd();
    let reader = OfdReader::new(OfdPackage::new(Cursor::new(data)).unwrap()).unwrap();
    let ofd = reader.ofd();

    assert_eq!(ofd.version, "1.0");
    assert_eq!(ofd.doc_type, "OFD");
    assert!(!ofd.is_archive());
    assert_eq!(ofd.doc_bodies.len(), 1);

    let info = &ofd.doc_bodies[0].doc_info;
    assert_eq!(info.title.as_deref(), Some("测试文档"));
    assert_eq!(info.author.as_deref(), Some("张三"));
    assert_eq!(info.creation_date.as_deref(), Some("2026-06-01"));

    let keywords = info.keywords.as_ref().unwrap();
    assert_eq!(keywords.keywords, vec!["OFD", "版式"]);

    let custom = &info.custom_datas.as_ref().unwrap().custom_datas;
    assert_eq!(custom.len(), 1);
    assert_eq!(custom[0].name, "区域");
    assert_eq!(custom[0].value, "华东");
}

#[test]
fn parse_document_and_pages() {
    let data = build_ofd();
    let mut reader = OfdReader::new(OfdPackage::new(Cursor::new(data)).unwrap()).unwrap();

    let body = reader.ofd().doc_bodies[0].clone();
    let doc = reader.load_document(&body).unwrap();

    assert_eq!(doc.base, "Doc_0");
    assert_eq!(doc.document.common_data.max_unit_id.value(), 1000);

    let pb = doc.document.common_data.page_area.physical_box;
    assert_eq!(pb.width, 210.0);
    assert_eq!(pb.height, 297.0);

    assert_eq!(doc.template_pages().len(), 1);
    assert_eq!(doc.public_res().len(), 1);
    assert_eq!(doc.pages().len(), 2);

    // 权限
    let perm = doc.document.permissions.as_ref().unwrap();
    assert_eq!(perm.edit, Some(false));
    let print = perm.print.as_ref().unwrap();
    assert_eq!(print.printable, Some(true));
    assert_eq!(print.copies, Some(5));

    // 大纲（树状嵌套）
    let outlines = doc.document.outlines.as_ref().unwrap();
    assert_eq!(outlines.outline_elems.len(), 1);
    assert_eq!(outlines.outline_elems[0].title, "第一章");
    assert_eq!(outlines.outline_elems[0].children[0].title, "第一节");

    // 首页：含一个图层
    let page0 = reader.load_page(&doc, &doc.pages()[0].clone()).unwrap();
    assert!(!page0.is_blank());
    let content = page0.content.as_ref().unwrap();
    assert_eq!(content.layers.len(), 1);
    assert_eq!(content.layers[0].layer_type.as_deref(), Some("Body"));
    assert_eq!(page0.templates.len(), 1);
    assert_eq!(page0.templates[0].template_id.value(), 10);

    // 次页：空白页
    let page1 = reader.load_page(&doc, &doc.pages()[1].clone()).unwrap();
    assert!(page1.is_blank());
}

#[test]
fn parse_resource() {
    let data = build_ofd();
    let mut reader = OfdReader::new(OfdPackage::new(Cursor::new(data)).unwrap()).unwrap();
    let body = reader.ofd().doc_bodies[0].clone();
    let doc = reader.load_document(&body).unwrap();

    let loc = doc.public_res()[0].clone();
    let res = reader.load_resource(&doc, &loc).unwrap();
    assert_eq!(res.base_loc.as_str(), "Res");
    let fonts = res.fonts.as_ref().unwrap();
    assert_eq!(fonts.fonts.len(), 1);
    assert_eq!(fonts.fonts[0].font_name, "宋体");
    assert_eq!(fonts.fonts[0].id.value(), 20);
}

/// 注释（如电子印章）应叠加渲染在页面内容之上。
///
/// 用一个以 `PathObject` 填充的绿色方块充当注释外观（避免依赖字型/图片资源），
/// 验证 `Annotations` → `PageAnnot` → `Appearance` 的装载与定位均正确。
#[test]
fn render_annotation_overlay() {
    const ANNOTATIONS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Annotations xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Page PageID="1"><ofd:FileLoc>Page_0/Annot_0.xml</ofd:FileLoc></ofd:Page>
</ofd:Annotations>"#;

    // 外观边界左上角位于页面 (100,100)mm；内部方块相对该点 [0,20]×[0,20]mm。
    const PAGE_ANNOT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:PageAnnot xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Annot Type="Stamp" ID="900">
    <ofd:Appearance Boundary="100 100 20 20">
      <ofd:PathObject ID="901" Boundary="0 0 20 20" Fill="true">
        <ofd:FillColor Value="0 200 0"/>
        <ofd:AbbreviatedData>M 0 0 L 20 0 L 20 20 L 0 20 C</ofd:AbbreviatedData>
      </ofd:PathObject>
    </ofd:Appearance>
  </ofd:Annot>
</ofd:PageAnnot>"#;

    // 在基础包之上追加注释相关文件，并让 Document.xml 声明 Annotations。
    let document_xml = DOCUMENT_XML.replace(
        "  </ofd:Pages>",
        "  </ofd:Pages>\n  <ofd:Annotations>Annots/Annotations.xml</ofd:Annotations>",
    );
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let files = [
            ("OFD.xml", OFD_XML),
            ("Doc_0/Document.xml", document_xml.as_str()),
            ("Doc_0/Pages/Page_0/Content.xml", PAGE0_XML),
            ("Doc_0/Pages/Page_1/Content.xml", PAGE1_XML),
            ("Doc_0/PublicRes.xml", PUBLIC_RES_XML),
            ("Doc_0/Annots/Annotations.xml", ANNOTATIONS_XML),
            ("Doc_0/Annots/Page_0/Annot_0.xml", PAGE_ANNOT_XML),
        ];
        for (name, content) in files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    let mut reader = OfdReader::new(OfdPackage::new(Cursor::new(buf)).unwrap()).unwrap();
    let body = reader.ofd().doc_bodies[0].clone();
    let doc = reader.load_document(&body).unwrap();

    let opts = RenderOptions::with_dpi(72.0);
    let pixmap = reader.render_page(&doc, 0, &opts).unwrap();

    // 注释方块中心 (110,110)mm 处应为绿色。
    let scale = 72.0 / 25.4;
    let x = (110.0 * scale) as u32;
    let y = (110.0 * scale) as u32;
    let idx = (x + y * pixmap.width()) as usize;
    let p = pixmap.pixels()[idx].demultiply();
    assert!(
        p.green() > 150 && p.red() < 100 && p.blue() < 100,
        "expected green annotation, got rgba({},{},{},{})",
        p.red(), p.green(), p.blue(), p.alpha()
    );
}

/// 复合对象应展开其在资源中引用的矢量图（`CT_VectorG`）内容并渲染。
///
/// 资源中定义一个 `CompositeGraphicUnit`（内含蓝色填充方块），页面用一个
/// `CompositeObject` 经 `ResourceID` 引用它并平移到指定边界，验证矢量图内容
/// 被展开、按边界定位并正确着色。
#[test]
fn render_composite_object_from_resource() {
    // 资源：一个 80×80 矢量图，内部为整面蓝色填充方块。
    const DOC_RES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Res xmlns:ofd="http://www.ofdspec.org/2016" BaseLoc="Res">
  <ofd:CompositeGraphicUnits>
    <ofd:CompositeGraphicUnit ID="50" Width="80" Height="80">
      <ofd:Content>
        <ofd:PathObject ID="51" Boundary="0 0 80 80" Fill="true">
          <ofd:FillColor Value="0 0 255"/>
          <ofd:AbbreviatedData>M 0 0 L 80 0 L 80 80 L 0 80 C</ofd:AbbreviatedData>
        </ofd:PathObject>
      </ofd:Content>
    </ofd:CompositeGraphicUnit>
  </ofd:CompositeGraphicUnits>
</ofd:Res>"#;

    // 页面：复合对象边界左上角位于 (40,40)mm，引用上面的矢量图。
    const PAGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content>
    <ofd:Layer ID="100" Type="Body">
      <ofd:CompositeObject ID="60" Boundary="40 40 80 80" ResourceID="50"/>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#;

    // 让 Document.xml 声明文档资源 DocumentRes.xml。
    let document_xml = DOCUMENT_XML.replace(
        "    <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>",
        "    <ofd:PublicRes>PublicRes.xml</ofd:PublicRes>\n    <ofd:DocumentRes>DocumentRes.xml</ofd:DocumentRes>",
    );
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let files = [
            ("OFD.xml", OFD_XML),
            ("Doc_0/Document.xml", document_xml.as_str()),
            ("Doc_0/Pages/Page_0/Content.xml", PAGE_XML),
            ("Doc_0/Pages/Page_1/Content.xml", PAGE1_XML),
            ("Doc_0/PublicRes.xml", PUBLIC_RES_XML),
            ("Doc_0/DocumentRes.xml", DOC_RES_XML),
        ];
        for (name, content) in files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    let mut reader = OfdReader::new(OfdPackage::new(Cursor::new(buf)).unwrap()).unwrap();
    let body = reader.ofd().doc_bodies[0].clone();
    let doc = reader.load_document(&body).unwrap();

    let opts = RenderOptions::with_dpi(72.0);
    let pixmap = reader.render_page(&doc, 0, &opts).unwrap();

    // 矢量图中心映射到页面 (80,80)mm 处，应为蓝色。
    let scale = 72.0 / 25.4;
    let x = (80.0 * scale) as u32;
    let y = (80.0 * scale) as u32;
    let idx = (x + y * pixmap.width()) as usize;
    let p = pixmap.pixels()[idx].demultiply();
    assert!(
        p.blue() > 200 && p.red() < 60 && p.green() < 60,
        "expected blue composite fill, got rgba({},{},{},{})",
        p.red(), p.green(), p.blue(), p.alpha()
    );
}

#[test]
fn render_page_to_pixmap() {
    let data = build_ofd();
    let mut reader = OfdReader::new(OfdPackage::new(Cursor::new(data)).unwrap()).unwrap();
    let body = reader.ofd().doc_bodies[0].clone();
    let doc = reader.load_document(&body).unwrap();

    // A4（210×297mm）以 72 DPI 渲染 → 约 595×842 像素。
    let opts = RenderOptions::with_dpi(72.0);
    let pixmap = reader.render_page(&doc, 0, &opts).unwrap();
    assert_eq!(pixmap.width(), 595);
    assert_eq!(pixmap.height(), 842);

    // 页内有一个红色填充矩形（页面坐标 50..150mm）。取其内部一点应为红色。
    let scale = 72.0 / 25.4;
    let px = (100.0 * scale) as u32; // 100mm 处
    let idx = (px + px * pixmap.width()) as usize;
    let pixel = pixmap.pixels()[idx].demultiply();
    assert!(pixel.red() > 200 && pixel.green() < 60 && pixel.blue() < 60,
        "expected red fill, got rgba({},{},{},{})",
        pixel.red(), pixel.green(), pixel.blue(), pixel.alpha());

    // 编码为 PNG 应成功且非空。
    let png = reader
        .render_page_to_bytes(&doc, 0, &opts, ofd_core::render::ImageFormat::Png)
        .unwrap();
    assert!(png.len() > 8 && &png[1..4] == b"PNG");
}
