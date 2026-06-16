#!/usr/bin/env bash
# 工作区覆盖率测量（cargo-llvm-cov）。
#
# 环境无 rustup，故显式指向系统 LLVM 工具链（与 rustc 内置 LLVM 版本兼容即可，
# llvm-profdata 需 >= 生成数据的版本）。stable rustc 不产出分支计数，
# 因此以 region 覆盖率作为分支覆盖的工程近似，门槛 90%。
#
# 用法：
#   scripts/coverage.sh            # 打印每文件汇总
#   scripts/coverage.sh --html     # 额外生成 HTML 报告到 target/llvm-cov/html
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
export LLVM_COV="${LLVM_COV:-/usr/lib/llvm/21/bin/llvm-cov}"
export LLVM_PROFDATA="${LLVM_PROFDATA:-/usr/lib/llvm/21/bin/llvm-profdata}"

cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--html" ]]; then
    cargo llvm-cov --workspace --html
    echo "HTML 报告：target/llvm-cov/html/index.html"
else
    cargo llvm-cov --workspace --summary-only
fi
