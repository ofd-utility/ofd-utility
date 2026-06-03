//! OFD 基础结构的 XML 数据模型（见 GB/T 33190—2016 第 7 章）。
//!
//! 各子模块对应规范中的不同部件：
//! - [`ofd`]：主入口 `OFD.xml`（7.4）；
//! - [`document`]：文档根节点 `Document.xml`（7.5、7.8）；
//! - [`page`]：页树与页对象（7.6、7.7）；
//! - [`resource`]：资源文件（7.9）；
//! - [`common`]：被多处引用的公共结构。

pub mod common;
pub mod document;
pub mod ofd;
pub mod page;
pub mod resource;

pub use common::{Actions, CtAction, CtDest, Version, Versions};
pub use document::{
    Bookmarks, CommonData, CtBookmark, CtOutlineElem, CtPageArea, CtPermission, Document, Outlines,
    Print, ValidPeriod, VPreferences,
};
pub use ofd::{CtDocInfo, CustomData, CustomDatas, DocBody, Keywords, Ofd};
pub use page::{Content, CtLayer, CtTemplatePage, PageObject, PageRef, Pages, Template};
pub use resource::{CtFont, CtMultiMedia, Fonts, MultiMedias, Res};
