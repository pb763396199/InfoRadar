---
title: 新增 InfoRadar 板块
purpose: guide
status: active
date: 2026-06-22
language: zh-CN
audience: InfoRadar 维护者
origin: docs/plans/2026-06-20-001-feat-inforadar-rust-daily-workbench-plan.md
---

# 新增 InfoRadar 板块

板块是 InfoRadar 的领域契约。它定义关注主题、默认信源、分类口径和评分关键词。

## 步骤

1. 在 `configs/boards/` 下新增 `<board-id>.toml`。
2. 设置 `id`、`name`、`description`、`keywords` 和 `categories`。
3. 选择已有 adapter 类型，例如 `rss` 或 `github_search`。
4. 运行：

```powershell
cargo run -p inforadar-cli -- validate-config
cargo run -p inforadar-cli -- collect --board <board-id> --date 2026-06-22
cargo run -p inforadar-cli -- build-site --all --out public
```

## 原则

- 普通板块应优先通过配置添加。
- 只有当板块需要新的认证、解析或评分逻辑时，才新增 Rust adapter。
- 公开字段必须遵守 `docs/DATA_POLICY.md`。
