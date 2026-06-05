# 最佳实践

[English](BEST_PRACTICES.en.md) · 中文

本文汇总在使用 `ofd-core` / `ofd-cli` 时的实用建议，帮助你写出更健壮、更高效的代码。

---

## 1. 读取器的生命周期与可变性

`OfdReader` 在 `open` 时即解析主入口 `OFD.xml`，后续按需装载文档、页、模板与资源。由于装载会读取底层 ZIP，**绝大多数装载方法需要 `&mut self`**。

```rust
let mut reader = OfdReader::open("sample.ofd")?;   // 注意 mut
let bodies = reader.ofd().doc_bodies.clone();       // 先克隆，避免借用冲突
let doc = reader.load_document(&bodies[0])?;
```

**经验法则**：先把需要的元数据（如 `doc_bodies`）`clone()` 出来，再调用需要 `&mut reader` 的方法，避免不可变借用与可变借用冲突。这正是源码示例中先 `clone()` 的原因。

## 2. 一个 OFD 可能包含多个文档体

`OFD.xml` 的 `DocBody` 可以有多个，且某些 `DocBody` 可能缺少 `DocRoot`（仅承载版本/签名信息）。遍历时务必跳过：

```rust
for body in &reader.ofd().doc_bodies.clone() {
    if body.doc_root.is_none() {
        continue; // 没有文档根节点，跳过
    }
    let doc = reader.load_document(body)?;
    // ...
}
```

## 3. 路径解析交给库

包内 `StLoc` 既可能是相对路径也可能是绝对路径。**不要自己拼接路径**——`load_page` / `load_template` / `load_resource` 已经基于文档所在目录 `LoadedDocument::base` 自动完成解析。需要读取本库未建模的部件时，用 `package_mut()` 配合 `resolve_path`。

## 4. 渲染：DPI 与内存

渲染按页面物理尺寸（毫米）× DPI 换算为像素画布。**DPI 越高，内存与耗时呈平方增长**：

| 用途 | 建议 DPI |
| --- | --- |
| 屏幕预览 / 缩略图 | 96 – 150 |
| 普通打印 | 200 – 300 |
| 高精度归档 | 300 – 600 |

库内置 `MAX_DIMENSION = 20000` 像素的安全上限，超过会拒绝渲染以避免超大内存分配。批量渲染高 DPI 时注意进程内存峰值。

```rust
let opts = RenderOptions::with_dpi(300.0)
    .background(None); // 透明背景（默认白色不透明）
```

## 5. 字体与中文显示

渲染文字时按以下顺序取字型：**包内内嵌字型 → 系统同名字体 → CJK 兜底字体族 → 通用无衬线**。

- 若 OFD 未内嵌字型且只用中文名（如“宋体”）引用，**渲染机器上需安装中文字体**，否则中文会因缺字形而丢失。
- 推荐安装 `Noto Sans CJK SC` / `Source Han Sans SC` 等覆盖面广的中文字型。
- 系统字体库为进程级懒加载，**首次渲染会有一次性加载开销**；服务化场景可在启动时预热一次渲染。

## 6. 校验：用 `check_*` 而非 `Err`

校验入口 `check_path` / `check_reader` **不会**因为文件损坏或签名失败返回 `Err`，而是把问题汇总进 `CheckReport`。这让你能一次性校验一批文件并列出全部不合规项：

```rust
use ofd_core::verify::{check_path, SigVerdict};

let report = check_path("sample.ofd");
if report.conforms() {
    println!("符合规范");
}
for problem in &report.problems {
    eprintln!("结构问题: {problem}");
}
for sig in &report.signatures {
    match sig.verdict() {
        SigVerdict::Valid => println!("签名 #{} 完整", sig.id),
        SigVerdict::Invalid => println!("签名 #{} 被篡改/缺失", sig.id),
        SigVerdict::Unverified => println!("签名 #{} 无法校验（算法不支持）", sig.id),
    }
}
```

**注意区分三态**：`Unverified`（算法不支持，无法判定）**不**等于不合规——`conforms()` 只把 `Invalid` 视为失败。

## 7. 签名校验的边界

- 本库只做**摘要级完整性**校验（SM3 重算比对），用于回答“被签名保护的文件是否被改动过”。
- 它**不**验证签名值本身（SM2 签名、证书链、签发时间），因此**不能**单独用于法律意义上的“签名有效性”判定。需要完整验签时，请结合专门的国密密码库。

## 8. 内存数据源（非文件）

`OfdReader` / `OfdPackage` 是泛型的，任何 `Read + Seek` 都可作为数据源。处理网络下载或数据库 BLOB 时，用 `Cursor` 包裹字节即可，无需落盘：

```rust
use std::io::Cursor;
use ofd_core::{OfdPackage, OfdReader};

let bytes: Vec<u8> = fetch_ofd_bytes();
let pkg = OfdPackage::new(Cursor::new(bytes))?;
let mut reader = OfdReader::new(pkg)?;
```

## 9. 错误处理

所有可失败的库 API 返回 `ofd_core::Result<T>`（`OfdError`）。`OfdError` 用 `thiserror` 定义，区分了 IO、ZIP、XML、结构、渲染、图片编解码等类别，便于按类型分支处理或转换为你自己的错误类型（`#[from]` 友好）。

## 10. 日志

`ofd-cli` 使用 `tracing`，默认 `INFO` 级别，可用 `RUST_LOG` 覆盖（如 `RUST_LOG=debug`）。作为库嵌入时，由你的应用初始化 `tracing-subscriber`；库本身不强制任何日志后端。

---

下一步：阅读 **[API 参考](REFERENCE.md)** 了解完整的类型与方法。
