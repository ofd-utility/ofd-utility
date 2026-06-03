//! `ofd-core` 演示命令行：解析一个 OFD 文件并打印其基础结构信息。

use std::process::ExitCode;

use ofd_core::{OfdReader, Result};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    // 默认输出 INFO 级别，可通过 RUST_LOG 覆盖；报告内容偏正文，去掉时间戳与 target。
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .with_target(false)
        .init();

    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        error!("用法: ofd-core <文件.ofd>");
        return ExitCode::FAILURE;
    };

    match dump(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("解析失败: {e}");
            ExitCode::FAILURE
        }
    }
}

fn dump(path: &str) -> Result<()> {
    let mut reader = OfdReader::open(path)?;
    let ofd = reader.ofd();

    // —— OFD.xml 主入口全部内容 ——
    info!("OFD 主入口 ({path})");
    info!("  Version={}  DocType={}{}", ofd.version, ofd.doc_type,
        if ofd.is_archive() { " (存档规范)" } else { "" });
    info!("  DocBody 数: {}", ofd.doc_bodies.len());

    let bodies = ofd.doc_bodies.clone();
    for (i, body) in bodies.iter().enumerate() {
        info!("DocBody #{i}");
        info!("  DocRoot: {}", opt(body.doc_root.as_ref()));
        info!("  Signatures: {}", opt(body.signatures.as_ref()));
        if let Some(versions) = &body.versions {
            for v in &versions.versions {
                info!(
                    "  Version: ID={} Index={} Current={} BaseLoc={}",
                    opt(v.id.as_ref()), opt(v.index.as_ref()),
                    opt(v.current.as_ref()), opt(v.base_loc.as_ref()),
                );
            }
        }

        // —— DocInfo 文档元数据全部内容 ——
        dump_doc_info(&body.doc_info);

        let Some(root) = &body.doc_root else {
            info!("  (无 DocRoot，跳过文档体)");
            continue;
        };
        info!("  根节点: {root}");

        let doc = reader.load_document(body)?;
        let cd = &doc.document.common_data;
        info!("  MaxUnitID: {}", cd.max_unit_id);
        let pb = &cd.page_area.physical_box;
        info!("  默认页面物理区域: {} x {} (mm)", pb.width, pb.height);
        info!("  模板页数: {}", doc.template_pages().len());
        info!("  公共资源: {}", doc.public_res().len());
        info!("  文档资源: {}", doc.document_res().len());
        info!("  页数: {}", doc.pages().len());

        for page_ref in doc.pages() {
            let page = reader.load_page(&doc, page_ref)?;
            let layers = page
                .content
                .as_ref()
                .map(|c| c.layers.len())
                .unwrap_or(0);
            info!(
                "    页 {} -> {} (图层数: {}{})",
                page_ref.id,
                page_ref.base_loc,
                layers,
                if page.is_blank() { ", 空白页" } else { "" }
            );
        }
    }

    Ok(())
}

/// 打印 `CT_DocInfo` 的全部字段（缺省项显示 `-`），结构紧凑。
fn dump_doc_info(d: &ofd_core::CtDocInfo) {
    info!("  DocInfo:");
    info!("    DocID={}  DocUsage={}", opt(d.doc_id.as_ref()), opt(d.doc_usage.as_ref()));
    info!("    Title={}", opt(d.title.as_ref()));
    info!("    Author={}  Creator={} {}",
        opt(d.author.as_ref()), opt(d.creator.as_ref()), opt(d.creator_version.as_ref()));
    info!("    Subject={}", opt(d.subject.as_ref()));
    info!("    Abstract={}", opt(d.abstract_.as_ref()));
    info!("    CreationDate={}  ModDate={}",
        opt(d.creation_date.as_ref()), opt(d.mod_date.as_ref()));
    info!("    Cover={}", opt(d.cover.as_ref()));

    let keywords = d.keywords.as_ref().map(|k| k.keywords.join(", ")).unwrap_or_default();
    info!("    Keywords={}", if keywords.is_empty() { "-".into() } else { keywords });

    match &d.custom_datas {
        Some(c) if !c.custom_datas.is_empty() => {
            info!("    CustomDatas ({}):", c.custom_datas.len());
            for cd in &c.custom_datas {
                info!("      {} = {}", cd.name, cd.value);
            }
        }
        _ => info!("    CustomDatas=-"),
    }
}

/// 将可选值渲染为紧凑字符串，缺省显示 `-`。
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "-".into())
}
