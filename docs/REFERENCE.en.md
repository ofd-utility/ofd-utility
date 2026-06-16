# API Reference

English · [中文](REFERENCE.md)

Quick reference for the `ofd-core` public API. Spec clauses refer to **GB/T 33190—2016**. Generate full docs with `cargo doc --open`.

---

## Module overview

| Module | Responsibility | Spec |
| --- | --- | --- |
| [`types`](#module-types) | Basic data types and path resolution | 7.3 |
| [`package`](#module-package) | ZIP container reader | ch. 6 |
| [`model`](#module-model) | XML data model | ch. 7 |
| [`render`](#module-render) | Render pages to images | 8.5 |
| [`verify`](#module-verify) | Conformance + signature-integrity checking | ch. 7, 18 |
| [`crypto`](#module-crypto) | SM3 hashing and Base64 | GB/T 32905—2016 |
| [`xmlfmt`](#module-xmlfmt) | XML pretty-printing | — |
| [`error`](#module-error) | Error types | — |

The top-level entry point is [`OfdReader`](#ofdreader), which wraps the container and model and exposes high-level loading, rendering and validation methods.

---

## OfdReader

The high-level reader for an OFD document. Parses the entry point on open, then loads document bodies on demand.

| Method | Signature | Description |
| --- | --- | --- |
| `open` | `open<P: AsRef<Path>>(path) -> Result<Self>` | Open from a filesystem path (`OfdReader<File>`) |
| `new` | `new(package: OfdPackage<R>) -> Result<Self>` | Create from an open package and parse `OFD.xml` |
| `ofd` | `ofd(&self) -> &Ofd` | The entry data model |
| `package_mut` | `package_mut(&mut self) -> &mut OfdPackage<R>` | Mutable access to the container (read unmodeled parts) |
| `load_document` | `load_document(&mut self, body: &DocBody) -> Result<LoadedDocument>` | Load the document root `Document.xml` |
| `load_page` | `load_page(&mut self, doc, page: &PageRef) -> Result<PageObject>` | Load a page object `Content.xml` |
| `load_template` | `load_template(&mut self, doc, template: &CtTemplatePage) -> Result<PageObject>` | Load a template page |
| `load_resource` | `load_resource(&mut self, doc, loc: &StLoc) -> Result<Res>` | Load a resource file |
| `load_signatures` | `load_signatures(&mut self) -> Result<Vec<LoadedSignature>>` | Load all signatures |
| `verify_signature` | `verify_signature(&mut self, &LoadedSignature) -> SignatureReport` | Verify one signature's integrity |
| `verify_signatures` | `verify_signatures(&mut self) -> Result<Vec<SignatureReport>>` | Verify all signatures |
| `render_page_to_file` | `render_page_to_file(&mut self, doc, page_index, &RenderOptions, path) -> Result<()>` | Render a page to an image file |

### LoadedDocument

A loaded layout document plus its directory `base` inside the package.

| Method | Returns | Description |
| --- | --- | --- |
| `pages()` | `&[PageRef]` | Pages in the page tree (in order) |
| `template_pages()` | `&[CtTemplatePage]` | Template pages |
| `public_res()` | `&[StLoc]` | Public resource paths |
| `document_res()` | `&[StLoc]` | Document resource paths |

Fields: `document: Document`, `base: String` (document directory, e.g. `Doc_0`).

---

## Module `types`

Basic data types (7.3) and path utilities.

| Type / function | Description |
| --- | --- |
| `StId` | Identifier (unsigned integer) |
| `StRefId` | Reference identifier |
| `StLoc` | In-package file path (relative or absolute) |
| `StBox` | Rectangle `x y w h` (mm) |
| `StPos` | Coordinate point |
| `StArray` | Numeric array |
| `resolve_path(base, loc) -> String` | Resolve an `StLoc` against a directory into an absolute in-package path |
| `parent_dir(path) -> &str` | Parent directory of a path |

---

## Module `package`

### OfdPackage&lt;R&gt;

Wraps a ZIP archive, generic over `R: Read + Seek` (commonly `File` or `Cursor`).

| Method | Description |
| --- | --- |
| `open(path)` | Open from a filesystem path |
| `new(reader)` | Create from any `Read + Seek` |
| `read(path) -> Result<Vec<u8>>` | Read an entry's raw bytes |
| `read_to_string(path) -> Result<String>` | Read as UTF-8 text |
| `parse<T: DeserializeOwned>(path) -> Result<T>` | Read and deserialize XML into a model |
| `contains(path) -> bool` | Whether an entry exists |
| `entries() -> Vec<String>` | All entry names |
| `len()` / `is_empty()` | Entry count / emptiness |

---

## Module `model`

The XML data model (ch. 7), split into submodules by part. The table lists the main exported types.

| Submodule | Main types | Part |
| --- | --- | --- |
| `ofd` | `Ofd`, `DocBody`, `CtDocInfo`, `Keywords`, `CustomData(s)` | `OFD.xml` (7.4) |
| `document` | `Document`, `CommonData`, `CtPageArea`, `CtPermission`, `Outlines`, `Bookmarks`, `ValidPeriod` | `Document.xml` (7.5/7.8) |
| `page` | `PageObject`, `PageRef`, `Pages`, `Content`, `CtLayer`, `CtTemplatePage`, `Template` | page tree/objects (7.6/7.7) |
| `resource` | `Res`, `CtFont`, `CtColorSpace`, `CtDrawParam`, `CtMultiMedia`, `Fonts`, `ColorSpaces` | resources (7.9) |
| `graphics` | `TextObject`, `PathObject`, `ImageObject`, `CompositeObject`, `CtColor`, `PageBlock`, `TextCode` | page graphic objects |
| `annotation` | `Annotations`, `Annot`, `PageAnnot`, `Appearance` | annotations |
| `signature` | `Signatures`, `Signature`, `SignedInfo`, `References`, `Reference`, `Seal`, `StampAnnot` | digital signatures (ch. 18) |
| `common` | `Version(s)`, `Actions`, `CtAction`, `CtDest` | common structures |

Common field examples:

- `Ofd { version: String, doc_type: String, doc_bodies: Vec<DocBody> }`, with method `is_archive()` (`DocType == "OFD-A"`).
- `DocBody { doc_info: CtDocInfo, doc_root: Option<StLoc>, signatures: Option<StLoc>, versions: Option<Versions> }`.
- `PageRef { id: StId, base_loc: StLoc }`.
- `PageObject` method `is_blank()` (no content layers).

---

## Module `render`

### RenderOptions

| Field / method | Description |
| --- | --- |
| `dpi: f64` | Output resolution (default 150) |
| `background: Option<[u8;4]>` | Background `[R,G,B,A]`; `None` is transparent (default opaque white) |
| `with_dpi(dpi) -> Self` | Construct with a given DPI, others default |
| `background(opt) -> Self` | Chainable background setter |

### Functions / re-exports

| Item | Description |
| --- | --- |
| `image_format_from_ext(ext) -> Option<ImageFormat>` | Infer image format from extension |
| `pub use image::ImageFormat` | Re-export, avoids depending on `image` directly |

Supported output formats: **PNG / JPEG / BMP / TIFF / GIF / WebP**. See `OfdReader::render_page_to_file` above.

---

## Module `verify`

| Type | Description |
| --- | --- |
| `CheckReport` | Per-file report: `problems: Vec<String>`, `signatures: Vec<SignatureReport>`; `conforms() -> bool` |
| `SignatureReport` | Per-signature report: `id`, `sig_type`, `method`, `references`; `verdict()`, `failures()` |
| `SigVerdict` | `Valid` / `Invalid` / `Unverified` |
| `RefCheck` | Per protected-file record: `file_ref`, `status` |
| `RefStatus` | `Ok` / `Mismatch{expected,actual}` / `Missing` / `Unsupported` |
| `CheckMethod` | `Sm3` / `Unsupported(String)`; `from_oid(opt)` |
| `LoadedSignature` | A loaded signature and its in-package path |

| Function | Description |
| --- | --- |
| `check_path<P: AsRef<Path>>(path) -> CheckReport` | Validate an OFD on the filesystem (never returns `Err`) |
| `check_reader<R>(&mut OfdReader<R>) -> CheckReport` | Validate an already-opened reader |

Constant: `OID_SM3 = "1.2.156.10197.1.401"`.

---

## Module `crypto`

Self-contained SM-series hashing and encoding, used by signature-integrity checking.

| Function | Description |
| --- | --- |
| `sm3(data: &[u8]) -> [u8; 32]` | SM3 hash (GB/T 32905—2016) |
| `base64_encode(data: &[u8]) -> String` | Standard Base64 encoding |

> SM2 signature verification is not implemented; digest computation only.

---

## Module `xmlfmt`

XML pretty-printing: reflow compact (unindented, possibly single-line) parts into a 2-space-indented multi-line form, preserving declarations, comments, CDATA and other nodes. `ofd-cli cat` uses this to print XML entries by default.

| Function | Description |
| --- | --- |
| `pretty_xml(input: &str) -> Result<String>` | Reflow XML text; returns `OfdError::Structure` on invalid input |

> Re-exported at the top level as `ofd_core::pretty_xml`.

---

## Module `error`

| Type | Description |
| --- | --- |
| `OfdError` | Error enum: `Io` / `Zip` / `Xml` / `BasicType` / `EntryNotFound` / `Structure` / `Image` / `Render` |
| `Result<T>` | Alias for `std::result::Result<T, OfdError>` |

Every variant has a human-readable `Display`, and `From` is implemented for `std::io::Error`, `zip::result::ZipError`, `quick_xml::DeError` and `image::ImageError`.

---

## Constants

| Constant | Value | Description |
| --- | --- | --- |
| `ENTRY_OFD` | `"OFD.xml"` | In-package entry filename (Table 1) |
| `verify::OID_SM3` | `"1.2.156.10197.1.401"` | SM3 algorithm OID |
