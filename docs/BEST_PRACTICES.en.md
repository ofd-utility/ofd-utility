# Best Practices

English · [中文](BEST_PRACTICES.md)

Practical guidance for using `ofd-core` / `ofd-cli` to write more robust and efficient code.

---

## 1. Reader lifecycle and mutability

`OfdReader` parses the entry `OFD.xml` on `open`, then lazily loads documents, pages, templates and resources. Because loading reads the underlying ZIP, **most loading methods take `&mut self`**.

```rust
let mut reader = OfdReader::open("sample.ofd")?;   // note: mut
let bodies = reader.ofd().doc_bodies.clone();       // clone first to avoid borrow conflicts
let doc = reader.load_document(&bodies[0])?;
```

**Rule of thumb**: `clone()` the metadata you need (e.g. `doc_bodies`) before calling methods that require `&mut reader`, to avoid an immutable/mutable borrow conflict. That's exactly why the source examples `clone()` first.

## 2. An OFD may contain multiple document bodies

`OFD.xml` can have multiple `DocBody` entries, and some may lack a `DocRoot` (carrying only version/signature info). Always skip those:

```rust
for body in &reader.ofd().doc_bodies.clone() {
    if body.doc_root.is_none() {
        continue; // no document root, skip
    }
    let doc = reader.load_document(body)?;
    // ...
}
```

## 3. Let the library resolve paths

An `StLoc` inside the package may be relative or absolute. **Don't concatenate paths yourself** — `load_page` / `load_template` / `load_resource` already resolve them against the document directory `LoadedDocument::base`. To read parts the library doesn't model, use `package_mut()` together with `resolve_path`.

## 4. Rendering: DPI and memory

Rendering converts the physical page size (mm) × DPI into a pixel canvas. **Memory and time grow quadratically with DPI**:

| Use case | Suggested DPI |
| --- | --- |
| Screen preview / thumbnails | 96 – 150 |
| Normal printing | 200 – 300 |
| High-fidelity archival | 300 – 600 |

The library enforces a `MAX_DIMENSION = 20000` pixel safety cap and refuses to render beyond it to avoid huge allocations. Watch peak process memory when batch-rendering at high DPI.

```rust
let opts = RenderOptions::with_dpi(300.0)
    .background(None); // transparent background (default is opaque white)
```

## 5. Fonts and CJK text

When rendering text, glyphs are sourced in this order: **embedded font → same-named system font → CJK fallback families → generic sans-serif**.

- If the OFD embeds no font and references only a Chinese name (e.g. "宋体"/SimSun), **a Chinese font must be installed on the rendering machine**, otherwise CJK text disappears due to missing glyphs.
- Install broad-coverage CJK fonts such as `Noto Sans CJK SC` / `Source Han Sans SC`.
- The system font database is process-wide and lazily loaded, so the **first render pays a one-time loading cost**; in a service, warm it up with one render at startup.

## 6. Validation: use `check_*`, not `Err`

The validation entry points `check_path` / `check_reader` do **not** return `Err` on a corrupt file or a failed signature; instead they collect everything into a `CheckReport`. This lets you validate a batch of files and list all non-conforming items at once:

```rust
use ofd_core::verify::{check_path, SigVerdict};

let report = check_path("sample.ofd");
if report.conforms() {
    println!("conforms");
}
for problem in &report.problems {
    eprintln!("structure problem: {problem}");
}
for sig in &report.signatures {
    match sig.verdict() {
        SigVerdict::Valid => println!("signature #{} intact", sig.id),
        SigVerdict::Invalid => println!("signature #{} tampered/missing", sig.id),
        SigVerdict::Unverified => println!("signature #{} not verifiable (unsupported alg)", sig.id),
    }
}
```

**Mind the three-state verdict**: `Unverified` (unsupported algorithm, cannot decide) is **not** the same as non-conforming — `conforms()` treats only `Invalid` as failure.

## 7. Limits of signature checking

- The library performs **digest-level integrity** checking only (SM3 recompute-and-compare), answering "have the signature-protected files been altered?".
- It does **not** verify the signature value itself (SM2 signature, certificate chain, signing time), so it **cannot** alone establish legal "signature validity". For full verification, combine it with a dedicated SM2/SM3 cryptography library.

## 8. In-memory data sources (not files)

`OfdReader` / `OfdPackage` are generic over any `Read + Seek` source. For network downloads or database BLOBs, wrap the bytes in a `Cursor` — no need to touch disk:

```rust
use std::io::Cursor;
use ofd_core::{OfdPackage, OfdReader};

let bytes: Vec<u8> = fetch_ofd_bytes();
let pkg = OfdPackage::new(Cursor::new(bytes))?;
let mut reader = OfdReader::new(pkg)?;
```

## 9. Error handling

Every fallible library API returns `ofd_core::Result<T>` (`OfdError`). `OfdError` is defined with `thiserror` and distinguishes IO, ZIP, XML, structure, render and image-codec categories, so you can branch by kind or convert into your own error type (it's `#[from]`-friendly).

## 10. Logging

`ofd-cli` uses `tracing`, defaulting to `INFO`, overridable via `RUST_LOG` (e.g. `RUST_LOG=debug`). When embedding the library, your application initializes `tracing-subscriber`; the library itself mandates no logging backend.

---

Next: read the **[API Reference](REFERENCE.en.md)** for the full type and method listing.
