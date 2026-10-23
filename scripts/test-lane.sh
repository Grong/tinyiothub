#!/usr/bin/env bash
# pregate 本地车道(pregate.json runner):Rust 全量 + web vitest。
# {files} 参数按本仓全量门禁惯例忽略——与 .githooks/pre-push.local 同覆盖面。
set -euo pipefail

cargo test --workspace --exclude zeroclaw --lib --bins --tests

# web 构建/测试需 Node 24(crypto.hash / ERR_REQUIRE_ESM;见 memory web-build-needs-node-24)
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npx vitest run
