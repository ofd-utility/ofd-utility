# ofd-core

[![crates.io](https://img.shields.io/crates/v/ofd-core.svg)](https://crates.io/crates/ofd-core)
[![docs.rs](https://docs.rs/ofd-core/badge.svg)](https://docs.rs/ofd-core)

OFD（开放版式文档，GB/T 33190—2016）解析、校验与渲染库。

A Rust library for parsing, validating and rendering OFD (Open Fixed-layout
Document, GB/T 33190-2016) files.

## 功能 / Features

- 第 6 章容器方案 + 第 7 章基本结构的解析（`OFD.xml`、`Document.xml`、页树、资源等）；
- OFD → 图片渲染（基于 `tiny-skia` 光栅化、`ttf-parser` 字型轮廓）；
- 文档结构校验。

## 示例 / Example

```rust,no_run
use ofd_core::OfdReader;

let mut reader = OfdReader::open("sample.ofd")?;
println!("版本: {}, 类型: {}", reader.ofd().version, reader.ofd().doc_type);

let bodies = reader.ofd().doc_bodies.clone();
let doc = reader.load_document(&bodies[0])?;
for page_ref in doc.pages() {
    let page = reader.load_page(&doc, page_ref)?;
    println!("页 {} 是否空白: {}", page_ref.id, page.is_blank());
}
# Ok::<(), ofd_core::OfdError>(())
```

## 安装 / Install

```toml
[dependencies]
ofd-core = "0.1"
```

## License

MIT
