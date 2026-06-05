<div align="center">

# ofd-tools

**OFD（开放版式文档，GB/T 33190—2016）解析、校验与渲染工具集 · 纯 Rust 实现**

[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build](https://img.shields.io/badge/build-cargo-green.svg)](#从源码构建)

中文 · [English](README.en.md)

</div>

---

`ofd-tools` 是一个用纯 Rust 编写、**无 C 依赖**的 OFD 工具集，对标国家标准 **GB/T 33190—2016《电子文件存储与交换格式 版式文档》**。它把 OFD 文件从 ZIP 容器一路解析到结构化数据模型，并在此之上提供 **规范校验**、**数字签名完整性校验** 与 **页面渲染成图片** 的能力。

项目由两部分组成：

| Crate | 类型 | 说明 |
| --- | --- | --- |
| [`ofd-core`](ofd-core) | 库 | OFD 解析 / 校验 / 渲染的核心 API，可嵌入你自己的程序 |
| [`ofd-cli`](ofd-cli) | 可执行程序 | 基于 `ofd-core` 的命令行工具，开箱即用 |

## ✨ 特性

- 📦 **容器解析**：直接读取 OFD（ZIP）容器，按路径定位并反序列化包内各 XML 部件。
- 🌲 **完整数据模型**：覆盖主入口 `OFD.xml`、文档根节点 `Document.xml`、页树 / 页对象、模板页、资源（字型 / 颜色空间 / 多媒体 / 绘制参数）、注释与数字签名。
- 🖼️ **页面渲染**：基于 [`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) 光栅化，将版式页渲染为 **PNG / JPEG / BMP / TIFF / GIF / WebP**，DPI 可调，内嵌字型与系统字体回退（含中文 CJK 兜底）。
- ✅ **规范校验**：检查容器、主入口、各文档的根节点 / 页 / 模板 / 资源能否按声明定位并解析。
- 🔐 **签名完整性校验**：内置自包含的国密 **SM3**（GB/T 32905—2016）实现，重算被保护文件摘要并与 `CheckValue` 比对，发现被篡改 / 缺失的文件。
- 🦀 **纯 Rust、零 C 依赖**：跨平台、易于交叉编译、便于嵌入。

> **范围说明**：签名校验只做**摘要级完整性**校验，**不**对签名值（SM2 椭圆曲线签名、证书链）做密码学验签——后者需要证书解析与曲线运算，超出基础库范围。

## 🚀 快速开始

### 安装 / 构建

需要 **Rust 1.93+**（edition 2024）。

```bash
git clone https://github.com/<your-org>/ofd-tools.git
cd ofd-tools
cargo build --release
# 产物：target/release/ofd-cli
```

也可直接通过 cargo 安装命令行工具：

```bash
cargo install --path ofd-cli
```

### 命令行用法

```bash
# 1) 查看 OFD 文件的基础结构信息
ofd-cli info sample.ofd

# 2) 校验一个或多个 OFD 文件是否符合规范（含签名完整性），列出不合规文件
ofd-cli check a.ofd b.ofd c.ofd

# 3) 将各页渲染为图片输出到目录（默认 150 DPI、PNG）
ofd-cli render sample.ofd ./out
ofd-cli render sample.ofd ./out --dpi 300 --format jpg --prefix invoice
```

`render` 输出文件名形如 `<前缀>_doc<文档序号>_page<页序号>.<格式>`，默认前缀为 `ofd2img-<渲染时刻 yyyyMMddHHmmss>`。

日志级别可用环境变量覆盖：

```bash
RUST_LOG=debug ofd-cli info sample.ofd
```

### 作为库使用

在 `Cargo.toml` 中加入依赖：

```toml
[dependencies]
ofd-core = { git = "https://github.com/<your-org>/ofd-tools" }
```

解析并遍历页面：

```rust
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

渲染首页为 PNG：

```rust
use ofd_core::{OfdReader, render::RenderOptions};

let mut reader = OfdReader::open("sample.ofd")?;
let body = reader.ofd().doc_bodies[0].clone();
let doc = reader.load_document(&body)?;
let opts = RenderOptions::with_dpi(200.0);
reader.render_page_to_file(&doc, 0, &opts, "page0.png")?;
# Ok::<(), ofd_core::OfdError>(())
```

更多类型与方法见 **[API 参考文档](docs/REFERENCE.md)**。

## 📚 文档

| 文档 | 说明 |
| --- | --- |
| [最佳实践](docs/BEST_PRACTICES.md) | 性能、错误处理、字体、批量校验等使用建议 |
| [API 参考](docs/REFERENCE.md) | `ofd-core` 模块、类型与方法速查 |
| [English README](README.en.md) | 英文说明 |

## 🗂️ 项目结构

```
ofd-tools/
├── ofd-core/            # 核心库
│   ├── src/
│   │   ├── package.rs   # ZIP 容器读取（规范第 6 章）
│   │   ├── model/       # 数据模型（规范第 7 章）
│   │   ├── render.rs    # OFD → 图片渲染
│   │   ├── verify.rs    # 规范符合性 + 签名完整性校验
│   │   ├── crypto.rs    # 自包含 SM3 与 Base64
│   │   ├── types.rs     # 基础数据类型（7.3）
│   │   └── error.rs     # 错误类型
│   └── tests/           # 端到端解析测试
├── ofd-cli/             # 命令行工具（info / check / render）
└── misc/specification/  # GB/T 33190—2016 规范原文
```

## 🛠️ 开发

```bash
cargo build           # 构建
cargo test            # 运行测试
cargo clippy          # 静态检查
cargo fmt             # 代码格式化
```

欢迎贡献！提交 PR 前请确保 `cargo test`、`cargo clippy`、`cargo fmt --check` 均通过。

## ❤️ 赞助 / 捐赠

`ofd-tools` 是免费开源软件。如果它为你节省了时间，欢迎请作者喝杯咖啡，你的支持是项目持续维护的动力 🙏

<div align="center">

| 微信 | 支付宝 |
| :---: | :---: |
| <img src="assets/wechat.jpg" width="220" alt="微信赞赏码"> | <img src="assets/alipay.jpg" width="220" alt="支付宝收款码"> |

</div>

如果暂时不方便捐赠，**点一个 Star ⭐ 或分享给需要的人** 同样是莫大的鼓励！

## 📄 许可证

本项目采用 **[MIT 许可证](LICENSE)**，可自由用于商业与非商业用途。

GB/T 33190—2016 规范原文版权归相应标准化组织所有，仅作开发参考。

## 🙏 致谢

- [tiny-skia](https://github.com/RazrFalcon/tiny-skia) — 2D 光栅化
- [quick-xml](https://github.com/tafia/quick-xml) — XML 解析
- [image](https://github.com/image-rs/image) — 图片编解码
- [fontdb](https://github.com/RazrFalcon/fontdb) / [ttf-parser](https://github.com/RazrFalcon/ttf-parser) — 字体处理
