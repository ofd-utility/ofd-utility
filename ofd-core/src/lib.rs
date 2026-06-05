//! `ofd-core`：OFD（开放版式文档，GB/T 33190—2016）基础结构解析库。
//!
//! 本 crate 实现规范第 6 章（容器方案）与第 7 章（基本结构）的解析，
//! 提供从 ZIP 容器到数据模型的标准 Rust API：
//!
//! - [`types`]：基础数据类型（7.3），如 [`StBox`]、[`StLoc`] 等；
//! - [`OfdPackage`]：ZIP 容器读取（第 6 章）；
//! - [`model`]：`OFD.xml`、`Document.xml`、页树/页对象、资源等数据模型；
//! - [`OfdReader`]：在容器与数据模型之上的高层接口，自动处理路径解析与装载。
//!
//! # 示例
//!
//! ```no_run
//! use ofd_core::OfdReader;
//!
//! let mut reader = OfdReader::open("sample.ofd")?;
//! println!("版本: {}, 类型: {}", reader.ofd().version, reader.ofd().doc_type);
//!
//! // 装载第一个版式文档及其首页
//! let bodies = reader.ofd().doc_bodies.clone();
//! let doc = reader.load_document(&bodies[0])?;
//! for page_ref in doc.pages() {
//!     let page = reader.load_page(&doc, page_ref)?;
//!     println!("页 {} 是否空白: {}", page_ref.id, page.is_blank());
//! }
//! # Ok::<(), ofd_core::OfdError>(())
//! ```

pub mod crypto;
pub mod error;
pub mod model;
pub mod package;
pub mod render;
pub mod types;
pub mod verify;

use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

pub use error::{OfdError, Result};
pub use model::*;
pub use package::OfdPackage;
pub use render::RenderOptions;
pub use types::{StArray, StBox, StId, StLoc, StPos, StRefId, parent_dir, resolve_path};
pub use verify::{
    CheckMethod, CheckReport, LoadedSignature, RefStatus, SigVerdict, SignatureReport, check_path,
    check_reader,
};

/// 包内主入口文件名（见表 1）。
pub const ENTRY_OFD: &str = "OFD.xml";

/// OFD 文档的高层读取器。
///
/// 打开时即解析主入口 [`Ofd`]（`OFD.xml`），随后可按需装载各版式文档的根节点、
/// 页对象、模板页与资源；装载过程自动完成 [`StLoc`] 的相对/绝对路径解析。
pub struct OfdReader<R> {
    package: OfdPackage<R>,
    ofd: Ofd,
}

impl OfdReader<File> {
    /// 从文件系统路径打开一个 OFD 文档。
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::new(OfdPackage::open(path)?)
    }
}

impl<R: Read + Seek> OfdReader<R> {
    /// 基于已打开的 [`OfdPackage`] 创建读取器，并解析主入口 `OFD.xml`。
    pub fn new(mut package: OfdPackage<R>) -> Result<Self> {
        let ofd: Ofd = package.parse(ENTRY_OFD)?;
        Ok(OfdReader { package, ofd })
    }

    /// 主入口数据模型（`OFD.xml`）。
    pub fn ofd(&self) -> &Ofd {
        &self.ofd
    }

    /// 底层容器的可变引用，便于读取本库未直接建模的部件。
    pub fn package_mut(&mut self) -> &mut OfdPackage<R> {
        &mut self.package
    }

    /// 装载某个版式文档的根节点（`Document.xml`）。
    ///
    /// 返回的 [`LoadedDocument`] 携带文档所在目录，供后续解析页、资源等
    /// 相对路径使用。
    pub fn load_document(&mut self, body: &DocBody) -> Result<LoadedDocument> {
        let doc_root = body
            .doc_root
            .as_ref()
            .ok_or_else(|| OfdError::Structure("DocBody is missing DocRoot".to_string()))?;
        let path = resolve_path("", doc_root);
        let base = parent_dir(&path).to_string();
        let document: Document = self.package.parse(&path)?;
        Ok(LoadedDocument { document, base })
    }

    /// 装载页树中某一页的页对象（`Content.xml`）。
    pub fn load_page(&mut self, doc: &LoadedDocument, page: &PageRef) -> Result<PageObject> {
        let path = resolve_path(&doc.base, &page.base_loc);
        self.package.parse(&path)
    }

    /// 装载某个模板页的内容（结构与普通页相同）。
    pub fn load_template(
        &mut self,
        doc: &LoadedDocument,
        template: &CtTemplatePage,
    ) -> Result<PageObject> {
        let path = resolve_path(&doc.base, &template.base_loc);
        self.package.parse(&path)
    }

    /// 装载文档的一个资源文件（公共资源或文档资源）。
    pub fn load_resource(&mut self, doc: &LoadedDocument, loc: &StLoc) -> Result<Res> {
        let path = resolve_path(&doc.base, loc);
        self.package.parse(&path)
    }
}

/// 已装载的版式文档及其在包内的所在目录。
///
/// `base` 为 `Document.xml` 所在目录（如 `Doc_0`），用于解析页对象、资源等
/// 节点中的相对 [`StLoc`]。
pub struct LoadedDocument {
    /// 文档根节点数据模型。
    pub document: Document,
    /// 文档所在目录（不含末尾 `/`）。
    pub base: String,
}

impl LoadedDocument {
    /// 页树中的页节点（按页顺序）。
    pub fn pages(&self) -> &[PageRef] {
        &self.document.pages.pages
    }

    /// 文档公共数据中定义的模板页。
    pub fn template_pages(&self) -> &[CtTemplatePage] {
        &self.document.common_data.template_pages
    }

    /// 公共资源文件路径列表。
    pub fn public_res(&self) -> &[StLoc] {
        &self.document.common_data.public_res
    }

    /// 文档资源文件路径列表。
    pub fn document_res(&self) -> &[StLoc] {
        &self.document.common_data.document_res
    }
}
