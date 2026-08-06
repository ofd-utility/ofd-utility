#!/usr/bin/env bash
#
# 快速发布 ofd-core crate 到 crates.io。
#
# 用法:
#   scripts/publish-ofd-core.sh [选项]
#
# 选项:
#   --dry-run        只做打包校验（cargo publish --dry-run），不真正发布
#   --no-verify      跳过 fmt / clippy / test 等本地检查，同时透传 --no-verify
#                    给 cargo，跳过隔离目录中对全部依赖的验证构建（不推荐用于真发布）
#   --offline        不访问网络：透传 --offline 给 cargo，并跳过 crates.io 版本
#                    占用检查。只能与 --dry-run 同用
#   --no-tag         发布成功后不创建 git tag
#   --allow-dirty    允许工作区有未提交改动（透传给 cargo / 跳过 git 检查）
#   -y, --yes        跳过最终确认提示
#   -h, --help       显示帮助
#
# 快速打包校验（秒级，不联网、不重编译）:
#   scripts/publish-ofd-core.sh --dry-run --no-verify --offline
#
# 环境变量:
#   CARGO_REGISTRY_TOKEN   crates.io token（若未登录则需要）
#
set -euo pipefail

CRATE="ofd-core"

# 切到仓库根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ---- 颜色输出 ----
if [[ -t 1 ]]; then
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'
  C_BLUE=$'\033[34m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_RED=''; C_GREEN=''; C_YELLOW=''; C_BLUE=''; C_BOLD=''; C_RESET=''
fi
info()  { echo "${C_BLUE}==>${C_RESET} ${C_BOLD}$*${C_RESET}"; }
ok()    { echo "${C_GREEN}✓${C_RESET} $*"; }
warn()  { echo "${C_YELLOW}!${C_RESET} $*" >&2; }
die()   { echo "${C_RED}✗ $*${C_RESET}" >&2; exit 1; }

# ---- 解析参数 ----
DRY_RUN=0
NO_VERIFY=0
NO_TAG=0
ALLOW_DIRTY=0
ASSUME_YES=0
OFFLINE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)     DRY_RUN=1 ;;
    --no-verify)   NO_VERIFY=1 ;;
    --offline)     OFFLINE=1 ;;
    --no-tag)      NO_TAG=1 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    -y|--yes)      ASSUME_YES=1 ;;
    -h|--help)     awk 'NR>1 && /^#/ {sub(/^# ?/,""); print; next} NR>1 {exit}' "$0"; exit 0 ;;
    *)             die "未知选项: $1（使用 --help 查看用法）" ;;
  esac
  shift
done

# 真正发布必须联网上传，--offline 只对打包校验有意义。
if [[ $OFFLINE -eq 1 && $DRY_RUN -eq 0 ]]; then
  die "--offline 只能与 --dry-run 同用（真正发布需要访问 crates.io）"
fi

command -v cargo >/dev/null 2>&1 || die "未找到 cargo，请先安装 Rust 工具链"

# ---- 读取版本号 ----
VERSION="$(cargo metadata --no-deps --format-version 1 \
  | grep -o "\"name\":\"$CRATE\"[^}]*\"version\":\"[^\"]*\"" \
  | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4 || true)"
[[ -n "$VERSION" ]] || VERSION="$(grep -m1 '^version' "$CRATE/Cargo.toml" | cut -d'"' -f2)"
[[ -n "$VERSION" ]] || die "无法解析 $CRATE 的版本号"
info "准备发布 ${C_BOLD}$CRATE v$VERSION${C_RESET}"

# ---- git 工作区检查 ----
if [[ $ALLOW_DIRTY -eq 0 ]]; then
  if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
    die "工作区有未提交改动，请先提交或使用 --allow-dirty"
  fi
  ok "git 工作区干净"
fi

# ---- 检查该版本是否已发布 ----
if [[ $OFFLINE -eq 1 ]]; then
  warn "已跳过 crates.io 版本占用检查（--offline）"
elif curl -sf "https://crates.io/api/v1/crates/$CRATE/$VERSION" >/dev/null 2>&1; then
  die "$CRATE v$VERSION 已存在于 crates.io，请先在 $CRATE/Cargo.toml 中提升版本号"
fi

# ---- 离线开关（透传给所有 cargo 子命令）----
OFFLINE_ARGS=()
[[ $OFFLINE -eq 1 ]] && OFFLINE_ARGS+=(--offline)

# ---- 本地检查 ----
if [[ $NO_VERIFY -eq 0 ]]; then
  info "cargo fmt --check"
  cargo fmt -p "$CRATE" -- --check || die "格式检查未通过（cargo fmt）"
  ok "格式检查通过"

  info "cargo clippy"
  cargo clippy "${OFFLINE_ARGS[@]}" -p "$CRATE" --all-features -- -D warnings \
    || die "clippy 检查未通过"
  ok "clippy 通过"

  info "cargo test"
  cargo test "${OFFLINE_ARGS[@]}" -p "$CRATE" --all-features || die "测试未通过"
  ok "测试通过"
else
  warn "已跳过 fmt / clippy / test 检查，以及 cargo 的验证构建"
fi

# ---- 打包校验 ----
# 显式指定 --registry crates-io：默认即 crates.io，且可避免本地
# ~/.cargo/config.toml 中替换/重定义 crates-io 源时的发布报错。
#
# --locked：复用 workspace 的 Cargo.lock，避免 cargo 为解压出的独立包重新解析
#   依赖图（否则每次都会打印 "Locking N packages"）。
# --no-verify：跳过在 target/package/ 隔离目录中对全部依赖的从零编译，这是
#   打包校验的耗时大头；隔离目录不共享 workspace 的编译缓存。
PUBLISH_ARGS=(-p "$CRATE" --registry crates-io --locked "${OFFLINE_ARGS[@]}")
[[ $ALLOW_DIRTY -eq 1 ]] && PUBLISH_ARGS+=(--allow-dirty)
[[ $NO_VERIFY  -eq 1 ]] && PUBLISH_ARGS+=(--no-verify)

# `cargo publish --dry-run` 即使不上传也必定访问 registry，与 --offline 互斥；
# 而它的实际工作（打包 + 验证构建）等价于 `cargo package`，后者支持离线。
if [[ $OFFLINE -eq 1 ]]; then
  info "cargo package（离线，等价于 publish --dry-run 的打包校验）"
  cargo package "${PUBLISH_ARGS[@]}"
else
  info "cargo publish --dry-run"
  cargo publish "${PUBLISH_ARGS[@]}" --dry-run
fi
ok "打包校验通过"

if [[ $DRY_RUN -eq 1 ]]; then
  ok "dry-run 完成，未真正发布"
  exit 0
fi

# ---- 最终确认 ----
if [[ $ASSUME_YES -eq 0 ]]; then
  echo
  read -r -p "确认发布 ${C_BOLD}$CRATE v$VERSION${C_RESET} 到 crates.io? [y/N] " reply
  [[ "$reply" =~ ^[Yy]$ ]] || die "已取消"
fi

# ---- 发布 ----
info "cargo publish"
cargo publish "${PUBLISH_ARGS[@]}"
ok "已发布 $CRATE v$VERSION 到 crates.io"

# ---- 打 tag ----
if [[ $NO_TAG -eq 0 ]]; then
  TAG="$CRATE-v$VERSION"
  if git rev-parse "$TAG" >/dev/null 2>&1; then
    warn "git tag $TAG 已存在，跳过"
  else
    git tag -a "$TAG" -m "Release $CRATE v$VERSION"
    ok "已创建 git tag $TAG（执行 'git push origin $TAG' 推送）"
  fi
fi

ok "完成 🎉"
