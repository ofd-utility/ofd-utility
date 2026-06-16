# API 参考

[English](REFERENCE.en.md) · 中文

`ofd-core` 公共 API 速查。规范条目均指 **GB/T 33190—2016**。完整文档可运行 `cargo doc --open` 生成。

---

## 模块总览

| 模块 | 职责 | 规范 |
| --- | --- | --- |
| [`types`](#模块-types) | 基础数据类型与路径解析 | 7.3 |
| [`package`](#模块-package) | ZIP 容器读取 | 第 6 章 |
| [`model`](#模块-model) | XML 数据模型 | 第 7 章 |
| [`render`](#模块-render) | 页面渲染为图片 | 8.5 |
| [`verify`](#模块-verify) | 规范符合性 + 签名完整性校验 | 7、18 章 |
| [`crypto`](#模块-crypto) | SM3 杂凑与 Base64 | GB/T 32905—2016 |
| [`xmlfmt`](#模块-xmlfmt) | XML 重排美化 | — |
| [`error`](#模块-error) | 错误类型 | — |

顶层入口是 [`OfdReader`](#ofdreader)，封装容器与模型，提供高层装载与渲染、校验方法。

---

## OfdReader

OFD 文档的高层读取器。打开时即解析主入口，随后按需装载文档体。

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `open` | `open<P: AsRef<Path>>(path) -> Result<Self>` | 从文件路径打开（`OfdReader<File>`） |
| `new` | `new(package: OfdPackage<R>) -> Result<Self>` | 从已打开的包创建并解析 `OFD.xml` |
| `ofd` | `ofd(&self) -> &Ofd` | 主入口数据模型 |
| `package_mut` | `package_mut(&mut self) -> &mut OfdPackage<R>` | 底层容器可变引用（读取未建模部件） |
| `load_document` | `load_document(&mut self, body: &DocBody) -> Result<LoadedDocument>` | 装载文档根节点 `Document.xml` |
| `load_page` | `load_page(&mut self, doc, page: &PageRef) -> Result<PageObject>` | 装载页对象 `Content.xml` |
| `load_template` | `load_template(&mut self, doc, template: &CtTemplatePage) -> Result<PageObject>` | 装载模板页 |
| `load_resource` | `load_resource(&mut self, doc, loc: &StLoc) -> Result<Res>` | 装载资源文件 |
| `load_signatures` | `load_signatures(&mut self) -> Result<Vec<LoadedSignature>>` | 装载全部签名 |
| `verify_signature` | `verify_signature(&mut self, &LoadedSignature) -> SignatureReport` | 校验单个签名完整性 |
| `verify_signatures` | `verify_signatures(&mut self) -> Result<Vec<SignatureReport>>` | 校验全部签名 |
| `render_page_to_file` | `render_page_to_file(&mut self, doc, page_index, &RenderOptions, path) -> Result<()>` | 渲染某页为图片文件 |

### LoadedDocument

已装载的版式文档及其在包内目录 `base`。

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `pages()` | `&[PageRef]` | 页树中的页（按顺序） |
| `template_pages()` | `&[CtTemplatePage]` | 模板页 |
| `public_res()` | `&[StLoc]` | 公共资源路径 |
| `document_res()` | `&[StLoc]` | 文档资源路径 |

字段：`document: Document`、`base: String`（文档目录，如 `Doc_0`）。

---

## 模块 `types`

基础数据类型（7.3）与路径工具。

| 类型 / 函数 | 说明 |
| --- | --- |
| `StId` | 标识（无符号整数） |
| `StRefId` | 引用标识 |
| `StLoc` | 包内文件路径（相对或绝对） |
| `StBox` | 矩形区域 `x y w h`（毫米） |
| `StPos` | 坐标点 |
| `StArray` | 数值数组 |
| `resolve_path(base, loc) -> String` | 基于目录解析 `StLoc` 为包内绝对路径 |
| `parent_dir(path) -> &str` | 取路径的父目录 |

---

## 模块 `package`

### OfdPackage&lt;R&gt;

封装 ZIP 归档，泛型于 `R: Read + Seek`（常见 `File` 或 `Cursor`）。

| 方法 | 说明 |
| --- | --- |
| `open(path)` | 从文件路径打开 |
| `new(reader)` | 从任意 `Read + Seek` 创建 |
| `read(path) -> Result<Vec<u8>>` | 读取条目原始字节 |
| `read_to_string(path) -> Result<String>` | 读取为 UTF-8 文本 |
| `parse<T: DeserializeOwned>(path) -> Result<T>` | 读取并反序列化 XML 为模型 |
| `contains(path) -> bool` | 是否存在条目 |
| `entries() -> Vec<String>` | 所有条目名 |
| `len()` / `is_empty()` | 条目数量 / 是否为空 |

---

## 模块 `model`

XML 数据模型（第 7 章），按部件分子模块。下表列主要导出类型。

| 子模块 | 主要类型 | 对应部件 |
| --- | --- | --- |
| `ofd` | `Ofd`、`DocBody`、`CtDocInfo`、`Keywords`、`CustomData(s)` | `OFD.xml`（7.4） |
| `document` | `Document`、`CommonData`、`CtPageArea`、`CtPermission`、`Outlines`、`Bookmarks`、`ValidPeriod` | `Document.xml`（7.5/7.8） |
| `page` | `PageObject`、`PageRef`、`Pages`、`Content`、`CtLayer`、`CtTemplatePage`、`Template` | 页树/页对象（7.6/7.7） |
| `resource` | `Res`、`CtFont`、`CtColorSpace`、`CtDrawParam`、`CtMultiMedia`、`Fonts`、`ColorSpaces` | 资源（7.9） |
| `graphics` | `TextObject`、`PathObject`、`ImageObject`、`CompositeObject`、`CtColor`、`PageBlock`、`TextCode` | 页面图元 |
| `annotation` | `Annotations`、`Annot`、`PageAnnot`、`Appearance` | 注释 |
| `signature` | `Signatures`、`Signature`、`SignedInfo`、`References`、`Reference`、`Seal`、`StampAnnot` | 数字签名（第 18 章） |
| `common` | `Version(s)`、`Actions`、`CtAction`、`CtDest` | 公共结构 |

常用字段示例：

- `Ofd { version: String, doc_type: String, doc_bodies: Vec<DocBody> }`，方法 `is_archive()`（`DocType == "OFD-A"`）。
- `DocBody { doc_info: CtDocInfo, doc_root: Option<StLoc>, signatures: Option<StLoc>, versions: Option<Versions> }`。
- `PageRef { id: StId, base_loc: StLoc }`。
- `PageObject` 方法 `is_blank()`（无内容图层）。

---

## 模块 `render`

### RenderOptions

| 字段 / 方法 | 说明 |
| --- | --- |
| `dpi: f64` | 输出分辨率（默认 150） |
| `background: Option<[u8;4]>` | 背景 `[R,G,B,A]`，`None` 为透明（默认白色不透明） |
| `with_dpi(dpi) -> Self` | 以指定 DPI 构造，其余默认 |
| `background(opt) -> Self` | 链式设置背景 |

### 函数 / 重导出

| 项 | 说明 |
| --- | --- |
| `image_format_from_ext(ext) -> Option<ImageFormat>` | 由扩展名推断图片格式 |
| `pub use image::ImageFormat` | 重导出，免去直接依赖 `image` |

支持输出格式：**PNG / JPEG / BMP / TIFF / GIF / WebP**。渲染方法 `OfdReader::render_page_to_file` 见上。

---

## 模块 `verify`

| 类型 | 说明 |
| --- | --- |
| `CheckReport` | 单文件校验报告：`problems: Vec<String>`、`signatures: Vec<SignatureReport>`；`conforms() -> bool` |
| `SignatureReport` | 单签名报告：`id`、`sig_type`、`method`、`references`；`verdict()`、`failures()` |
| `SigVerdict` | `Valid` / `Invalid` / `Unverified` |
| `RefCheck` | 单个被保护文件记录：`file_ref`、`status` |
| `RefStatus` | `Ok` / `Mismatch{expected,actual}` / `Missing` / `Unsupported` |
| `CheckMethod` | `Sm3` / `Unsupported(String)`；`from_oid(opt)` |
| `LoadedSignature` | 已装载签名及其包内路径 |

| 函数 | 说明 |
| --- | --- |
| `check_path<P: AsRef<Path>>(path) -> CheckReport` | 校验文件系统中的 OFD（不返回 `Err`） |
| `check_reader<R>(&mut OfdReader<R>) -> CheckReport` | 校验已打开的读取器 |

常量：`OID_SM3 = "1.2.156.10197.1.401"`。

---

## 模块 `crypto`

自包含国密杂凑与编码，供签名完整性校验使用。

| 函数 | 说明 |
| --- | --- |
| `sm3(data: &[u8]) -> [u8; 32]` | SM3 杂凑（GB/T 32905—2016） |
| `base64_encode(data: &[u8]) -> String` | 标准 Base64 编码 |

> 不实现 SM2 验签，仅做摘要计算。

---

## 模块 `xmlfmt`

XML 美化：将紧凑（无缩进、可能单行）的部件重排为 2 空格缩进的多行形式，保留声明、注释、CDATA 等节点。`ofd-cli cat` 默认即以此打印 XML 条目。

| 函数 | 说明 |
| --- | --- |
| `pretty_xml(input: &str) -> Result<String>` | 重排 XML 文本；输入非法时返回 `OfdError::Structure` |

> 顶层以 `ofd_core::pretty_xml` 重导出。

---

## 模块 `error`

| 类型 | 说明 |
| --- | --- |
| `OfdError` | 错误枚举：`Io` / `Zip` / `Xml` / `BasicType` / `EntryNotFound` / `Structure` / `Image` / `Render` |
| `Result<T>` | `std::result::Result<T, OfdError>` 别名 |

各变体均带可读 `Display`，并对 `std::io::Error`、`zip::result::ZipError`、`quick_xml::DeError`、`image::ImageError` 实现了 `From`。

---

## 常量

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `ENTRY_OFD` | `"OFD.xml"` | 包内主入口文件名（表 1） |
| `verify::OID_SM3` | `"1.2.156.10197.1.401"` | SM3 算法 OID |
