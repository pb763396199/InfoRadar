---
title: 新增 InfoRadar 信源
purpose: guide
status: active
date: 2026-06-22
language: zh-CN
audience: InfoRadar 维护者
origin: docs/plans/2026-06-20-001-feat-inforadar-rust-daily-workbench-plan.md
---

# 新增 InfoRadar 信源

信源只负责描述信息 origin、协议、限流、公开字段和风险等级。板块语义、分类和评分由 board 负责。

## 配置示例

```toml
[[sources]]
id = "example-news"
name = "Example News"
kind = "rss"
enabled = true
url = "https://example.com/feed.xml"
rate_limit_per_minute = 6
timeout_seconds = 20
max_items = 50
risk_level = "stable"
public_fields = ["title", "url", "description", "published_at"]
```

## 支持的 v1 类型

- `rss`
- `json_feed`
- `github_search`

`web_list`、登录态网页抓取和复杂 HTML 抽取不属于 v1 支持范围；需要先新增 Rust adapter 和测试，再暴露给配置。

## 验证

```powershell
cargo run -p inforadar-cli -- validate-config
cargo run -p inforadar-cli -- collect --board unreal --date 2026-06-22
```

如果单个信源失败，InfoRadar 应记录来源健康状态，而不是让整个板块日报静默失败。
