<div align="center">

# ofd-tools

**A pure-Rust toolkit to parse, validate and render OFD (Open Fixed-layout Document, GB/T 33190—2016)**

[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build](https://img.shields.io/badge/build-cargo-green.svg)](#build-from-source)

[中文](README.md) · English

</div>

---

`ofd-tools` is a pure-Rust, **C-dependency-free** toolkit for OFD, the Chinese national standard **GB/T 33190—2016 "Electronic files storage and exchange formats — Fixed layout documents"**. It parses an OFD file all the way from its ZIP container down to a structured data model, and builds on top of it to provide **conformance checking**, **digital-signature integrity verification** and **page-to-image rendering**.

The project consists of two crates:

| Crate | Kind | Description |
| --- | --- | --- |
| [`ofd-core`](ofd-core) | library | The core parsing / validation / rendering API, embeddable in your own program |
| [`ofd-cli`](ofd-cli) | binary | A ready-to-use command-line tool built on `ofd-core` |

## ✨ Features

- 📦 **Container parsing** — read the OFD (ZIP) container directly, locate parts by path and deserialize the XML payloads.
- 🌲 **Full data model** — covers the entry `OFD.xml`, document root `Document.xml`, page tree / page objects, template pages, resources (fonts / color spaces / multimedia / draw params), annotations and digital signatures.
- 🖼️ **Page rendering** — rasterize layout pages via [`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) into **PNG / JPEG / BMP / TIFF / GIF / WebP**, with adjustable DPI, embedded-font support and system-font fallback (including a CJK fallback chain).
- ✅ **Conformance checking** — verify that the container, the entry point and every document's root / pages / templates / resources can be located and parsed as declared.
- 🔐 **Signature integrity** — a self-contained **SM3** (GB/T 32905—2016) implementation recomputes the digest of each protected file and compares it against `CheckValue` to detect tampered / missing files.
- 🦀 **Pure Rust, zero C deps** — cross-platform, easy to cross-compile and embed.

> **Scope** — signature checking is **digest-level integrity** only. It does **not** cryptographically verify the signature value (SM2 elliptic-curve signature, certificate chain), which would require certificate parsing and curve arithmetic beyond the scope of a base library.

## 🚀 Quick start

### Install / build

Requires **Rust 1.93+** (edition 2024).

```bash
git clone https://github.com/<your-org>/ofd-tools.git
cd ofd-tools
cargo build --release
# artifact: target/release/ofd-cli
```

Or install the CLI directly via cargo:

```bash
cargo install --path ofd-cli
```

### CLI usage

```bash
# 1) Print the basic structure of an OFD file
ofd-cli info sample.ofd

# 2) List every file in the package, Linux `tree` style (--size appends uncompressed byte sizes)
ofd-cli tree sample.ofd
ofd-cli tree sample.ofd --size

# 3) Print the content of entries whose path matches a regex
#    XML is pretty-printed to stdout by default (--raw prints the original source);
#    other types are exported to the current directory
ofd-cli cat sample.ofd 'OFD\.xml'
ofd-cli cat sample.ofd '\.xml$' --raw

# 4) Validate one or more OFD files (incl. signature integrity); failing files are listed
ofd-cli check a.ofd b.ofd c.ofd

# 5) Render every page to images in a directory (default 150 DPI, PNG)
ofd-cli render sample.ofd ./out
ofd-cli render sample.ofd ./out --dpi 300 --format jpg --prefix invoice
```

`tree` derives the directory hierarchy from the flat ZIP entries and prints a directory / file count summary at the end.

`cat`'s `pattern` is an unanchored regex matched as a substring against every in-package entry path; it exits non-zero when nothing matches.

`render` produces filenames like `<prefix>_doc<doc-index>_page<page-index>.<format>`; the default prefix is `ofd2img-<render-time yyyyMMddHHmmss>`.

Override the log level with an env var:

```bash
RUST_LOG=debug ofd-cli info sample.ofd
```

### Use as a library

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
ofd-core = { git = "https://github.com/<your-org>/ofd-tools" }
```

Parse and walk pages:

```rust
use ofd_core::OfdReader;

let mut reader = OfdReader::open("sample.ofd")?;
println!("version: {}, type: {}", reader.ofd().version, reader.ofd().doc_type);

let bodies = reader.ofd().doc_bodies.clone();
let doc = reader.load_document(&bodies[0])?;
for page_ref in doc.pages() {
    let page = reader.load_page(&doc, page_ref)?;
    println!("page {} blank? {}", page_ref.id, page.is_blank());
}
# Ok::<(), ofd_core::OfdError>(())
```

Render the first page to PNG:

```rust
use ofd_core::{OfdReader, render::RenderOptions};

let mut reader = OfdReader::open("sample.ofd")?;
let body = reader.ofd().doc_bodies[0].clone();
let doc = reader.load_document(&body)?;
let opts = RenderOptions::with_dpi(200.0);
reader.render_page_to_file(&doc, 0, &opts, "page0.png")?;
# Ok::<(), ofd_core::OfdError>(())
```

See the **[API reference](docs/REFERENCE.en.md)** for the full type and method listing.

## 📚 Documentation

| Document | Description |
| --- | --- |
| [Best Practices](docs/BEST_PRACTICES.en.md) | Guidance on performance, error handling, fonts, batch validation, etc. |
| [API Reference](docs/REFERENCE.en.md) | Quick reference for `ofd-core` modules, types and methods |
| [中文 README](README.md) | Chinese documentation |

## 🗂️ Project layout

```
ofd-tools/
├── ofd-core/            # core library
│   ├── src/
│   │   ├── package.rs   # ZIP container reader (spec ch. 6)
│   │   ├── model/       # data model (spec ch. 7)
│   │   ├── render.rs    # OFD → image rendering
│   │   ├── verify.rs    # conformance + signature integrity checking
│   │   ├── crypto.rs    # self-contained SM3 and Base64
│   │   ├── types.rs     # basic data types (7.3)
│   │   └── error.rs     # error types
│   └── tests/           # end-to-end parsing tests
├── ofd-cli/             # command-line tool (info / tree / cat / check / render)
└── misc/specification/  # GB/T 33190—2016 spec text
```

## 🛠️ Development

```bash
cargo build           # build
cargo test            # run tests
cargo clippy          # lint
cargo fmt             # format
```

Contributions welcome! Before opening a PR, please ensure `cargo test`, `cargo clippy` and `cargo fmt --check` all pass.

## ❤️ Sponsor / Donate

`ofd-tools` is free and open-source software. If it saved you time, consider buying the author a coffee — your support keeps the project maintained 🙏

<div align="center">

| WeChat | Alipay |
| :---: | :---: |
| <img src="assets/wechat.jpg" width="220" alt="WeChat reward QR"> | <img src="assets/alipay.jpg" width="220" alt="Alipay QR"> |

</div>

If you can't donate right now, **a Star ⭐ or sharing it with others** is equally appreciated!

## 📄 License

This project is licensed under the **[MIT License](LICENSE)** — free for commercial and non-commercial use.

The GB/T 33190—2016 specification text is copyrighted by the respective standards body and is included for development reference only.

## 🙏 Acknowledgements

- [tiny-skia](https://github.com/RazrFalcon/tiny-skia) — 2D rasterization
- [quick-xml](https://github.com/tafia/quick-xml) — XML parsing
- [image](https://github.com/image-rs/image) — image codecs
- [fontdb](https://github.com/RazrFalcon/fontdb) / [ttf-parser](https://github.com/RazrFalcon/ttf-parser) — font handling
