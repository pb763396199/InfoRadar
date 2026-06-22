---
title: InfoRadar Rust 多板块情报日报工作台
purpose: plan
type: feat
status: active
date: 2026-06-20
language: zh-CN
origin: user-request-2026-06-20-inforadar-strict-review-plan
---

# InfoRadar Rust 多板块情报日报工作台

## Overview

InfoRadar 是从 `F:\AiProject\TechRadar` 的 UE 情报原型中抽象出来的新工程，目标不是复刻传统 Tech Radar 图，而是构建一个每天可用的情报日报工作台：把分散来源的信息采集、归一、去重、评分、编排、发布，并让用户能查看全量信息、快速过滤、高价值优先阅读、评分收藏、追溯原链接和复盘历史。

v1 采用 Rust CLI + SQLite + GitHub Actions + GitHub Pages。它是离线生成器和静态只读工作台，不是常驻 Web 服务。第一板块固定为 `unreal`，先证明一个真实板块能连续 7 天稳定产出高质量日报，再扩展到 AI、GameDev、Tools 等板块。

## Problem Statement / Motivation

旧 `TechRadar` 工程已经证明“UE 情报采集 + 静态大屏”有价值，但它仍是技能包式原型：

- 采集、归一、评分、去重、索引、详情侧车和 HTML 生成耦合在长脚本里。
- 信源扩展依赖硬编码 allowlist 和脚本约定。
- Dashboard 过度偏向泳道/Top 展示，用户不能以“全量信息处理流”为主。
- 去重、来源健康、公开字段白名单、评分持久化和发布边界没有形成工程契约。
- GitHub Pages 只能做静态发布，不能承载本地预览代理或在线写入能力。

第一性原理要求 InfoRadar 先服务用户每天的最小工作链条：

1. 从可信来源拿到候选信息。
2. 统一时间、来源、标题、链接、摘要、标签。
3. 合并重复事件。
4. 判断重要性、可信度、影响范围和时效性。
5. 按板块生成日报。
6. 发布可读、可追溯的静态页面。
7. 让用户能评分、收藏、已读、备注并复盘历史。

## Proposed Solution

新建 `F:\AiProject\InfoRadar` 为独立 Rust workspace，保留旧工程作为迁移来源和行为参考，不在旧工程中继续大规模重构。

v1 交付一个纵向闭环：

- Rust CLI 支持导入旧 TechRadar UE 历史数据。
- SQLite 保存 board、source、observation、item、score、daily issue、publish snapshot。
- 基础去重以 canonical URL 为第一优先级，标题归一相似匹配为补充。
- 构建静态 `public/`，GitHub Pages 只发布公开白名单字段。
- Dashboard 默认是日报工作台：顶部概览、板块切换、全量列表、快速筛选、详情抽屉、本地评分收藏。
- GitHub Actions 支持 daily schedule 和 workflow_dispatch 手动补采。

## User Story Map

| 日常活动主线 | 最小用户故事 | 页面/交互要求 | MVP 优先级 |
| --- | --- | --- | --- |
| 打开日报 | 用户进入今日默认板块，先知道今天收集了多少、哪些新增、哪些高价值 | 顶部显示日期、板块、采集总量、新增数、高价值数、未读数、来源覆盖、失败来源 | P0 |
| 切换板块 | 用户可在 Unreal、AI、GameDev、Tools 等板块间切换 | 板块是一等导航对象，不只是筛选标签 | P0 |
| 看全量信息 | 用户能看到本日全量收集条目 | 主视图是可排序、可过滤的信息流/表格 | P0 |
| 看新增/高价值 | 用户每天优先看新增和高价值内容 | 默认排序为高价值 + 新增 + 未读 | P0 |
| 展开详情 | 用户点开条目后看到摘要、证据、来源、历史、评分 | 使用详情抽屉，不强制跳页 | P0 |
| 评分/收藏 | 用户可评分、收藏、标记已读、备注 | v1 使用浏览器本地持久化，后续再做跨设备 | P0 |
| 对比历史 | 用户知道条目是首次出现、重复出现还是变化 | P1 增加历史变化标记和详情内历史链路 | P1 |
| 回到原链接 | 用户能回源验证上下文 | 原链接是详情主操作，显示来源可信度和采集时间 | P0 |

## Technical Approach

### Architecture

Rust workspace：

- `crates/inforadar-core`：实体模型、URL 归一、去重、评分、分类、公开导出契约。
- `crates/inforadar-store`：SQLite schema、迁移、查询、快照。
- `crates/inforadar-collectors`：内置采集 adapters。
- `crates/inforadar-site`：静态站点导出。
- `crates/inforadar-cli`：命令入口。
- `web/`：静态 dashboard 模板、CSS、JS。
- `configs/boards/`：板块配置，例如 `unreal.toml`。

核心命令：

```powershell
cargo run -p inforadar-cli -- import-techradar --from F:\AiProject\TechRadar
cargo run -p inforadar-cli -- collect --board unreal --date 2026-06-20
cargo run -p inforadar-cli -- build-issue --board unreal --date 2026-06-20
cargo run -p inforadar-cli -- build-site --all --out public
cargo run -p inforadar-cli -- validate-config
```

### Domain Model

- `board`：板块，是领域契约，定义来源集合、权重、分类口径、去重规则、输出模板。
- `source`：信源，只描述 origin、协议、限流、认证、可信度和公开字段白名单。
- `observation`：一次采集得到的原始观测，保留 provenance。
- `item`：归一化后的情报条目，可合并多个 observation。
- `score`：系统评分，版本化、可重放。
- `rating`：用户评分、收藏、已读、备注；v1 本地保存，后续可同步。
- `daily_issue`：某一天某板块的日报批次。
- `publish_snapshot`：一次静态发布记录。

### SQLite 初始表

```sql
CREATE TABLE boards (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  config_json TEXT NOT NULL
);

CREATE TABLE sources (
  id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  url TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  risk_level TEXT NOT NULL,
  public_fields_json TEXT NOT NULL
);

CREATE TABLE collection_runs (
  id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL,
  run_date TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  status TEXT NOT NULL,
  error TEXT
);

CREATE TABLE observations (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  canonical_url TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  published_at TEXT,
  collected_at TEXT NOT NULL,
  raw_json TEXT NOT NULL
);

CREATE TABLE items (
  id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL,
  canonical_url TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  category TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  duplicate_count INTEGER NOT NULL
);

CREATE TABLE item_sources (
  item_id TEXT NOT NULL,
  observation_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  PRIMARY KEY (item_id, observation_id)
);

CREATE TABLE scores (
  item_id TEXT NOT NULL,
  score_version TEXT NOT NULL,
  rank_score INTEGER NOT NULL,
  relevance INTEGER NOT NULL,
  reason TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  PRIMARY KEY (item_id, score_version)
);

CREATE TABLE daily_issues (
  id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL,
  issue_date TEXT NOT NULL,
  generated_at TEXT NOT NULL,
  summary_json TEXT NOT NULL
);

CREATE TABLE publish_snapshots (
  id TEXT PRIMARY KEY,
  issue_id TEXT NOT NULL,
  output_dir TEXT NOT NULL,
  generated_at TEXT NOT NULL,
  public_manifest_json TEXT NOT NULL
);
```

### Plugin Strategy

v1 使用配置化 + 第一方 Rust 静态注册：

- 新板块优先通过 `configs/boards/*.toml` 添加。
- 普通 RSS/API/GitHub 类信源通过配置添加。
- 内置 adapters：RSS/Atom、JSON Feed、GitHub Releases/Search、Google News RSS。通用网页列表、登录态网页抓取和复杂 HTML 抽取不属于 v1 支持范围，必须先新增 Rust adapter 与测试。

v1.5 稳定内部 Rust trait 边界：

```rust
pub trait SourceAdapter {
    fn kind(&self) -> &'static str;
    fn collect(&self, source: &SourceConfig, ctx: &CollectContext) -> anyhow::Result<Vec<ObservationDraft>>;
}

pub trait Parser {
    fn parse(&self, payload: &[u8], source: &SourceConfig) -> anyhow::Result<Vec<ObservationDraft>>;
}

pub trait Normalizer {
    fn normalize(&self, board: &BoardConfig, observation: &ObservationDraft) -> anyhow::Result<ItemDraft>;
}

pub trait Scorer {
    fn score(&self, board: &BoardConfig, item: &ItemDraft) -> anyhow::Result<ScoreDraft>;
}

pub trait Exporter {
    fn export(&self, snapshot: &PublishSnapshot, out_dir: &Path) -> anyhow::Result<()>;
}
```

v2 仅在真实第三方扩展需求出现后考虑 WASM 或外部进程协议。外部插件必须声明权限、schema、速率限制、公开字段白名单、数据政策。

## Implementation Phases

### Phase 1: 工程骨架与契约

- 建立 Rust workspace。
- 建立 `unreal.toml` 板块配置。
- 实现 core 模型、URL 归一、公开导出 DTO。
- 实现 SQLite schema 和 migration。
- 实现 `validate-config`。

成功标准：

- `cargo test --workspace` 通过。
- `validate-config` 能读取 `configs/boards/unreal.toml`。

### Phase 2: 旧数据导入与去重

- 实现 `import-techradar --from F:\AiProject\TechRadar`。
- 从旧 `reports/index.json` 导入历史 UE 条目。
- 以 canonical URL 合并重复项。
- 保留来源、日期、分类、分数、证据片段。

成功标准：

- 可导入 2026-04-09、2026-06-18、2026-06-19 历史样本。
- 同一 URL 不再重复成为多个 item。

### Phase 3: 日报构建与静态站导出

- 实现 `build-issue`。
- 实现 `build-site --all --out public`。
- Dashboard 包含顶部概览、板块切换、全量列表、筛选、详情抽屉、localStorage 评分/收藏/已读。
- 公开 JSON 做字段白名单过滤。

成功标准：

- `public/index.html` 可直接打开或通过静态 server 访问。
- 用户能进入全量列表，而不是只看 Top 榜或泳道图。

### Phase 4: Actions 与 Pages

- 新增 GitHub Actions workflow。
- 支持 schedule 和 workflow_dispatch。
- 恢复/上传 SQLite artifact。
- Pages 只发布 `public/`。
- 原始数据和 SQLite 不进入 Pages artifact。

成功标准：

- 手动触发能构建 Pages artifact。
- artifact 检查确认不包含 raw HTML、完整正文、日志、密钥。

### Phase 5: 首个真实采集闭环

- 实现 RSS/Atom、Google News RSS、GitHub adapter。
- `collect --board unreal --date <date>` 写入 observation。
- 单源失败不阻断整板块。
- 来源健康在日报中可见。

成功标准：

- 连续 7 天可自动生成 `unreal` 日报。
- 来源失败可诊断，不静默丢失。

## System-Wide Impact

### Interaction Graph

`inforadar collect` 读取 board config，调用 source adapter，写入 `collection_runs` 和 `observations`，随后 normalizer 将 observation 合并进 `items` 与 `item_sources`，scorer 写入 `scores`。`build-issue` 从 SQLite 读取 items/scores/source health，写入 `daily_issues`。`build-site` 读取 daily issues 和 items，生成 `public/` 静态站点和 JSON。

### Error & Failure Propagation

- 单个 source timeout、403、429、解析失败：记录 source health，不中断整板块。
- board config 无效：`validate-config` 和 collect 阶段直接失败。
- SQLite migration 失败：CLI 失败并输出明确错误。
- public export sanitizer 发现禁用字段：构建失败，禁止发布。

### State Lifecycle Risks

- 重复运行同一天可能重复写入 observation：通过 run id 和 canonical URL 合并 item，保持 daily issue 幂等。
- Actions artifact 恢复失败：允许从空库初始化，但必须在日志中标明历史缺失。
- localStorage 评分只在本机有效：UI 明确标注本地状态，避免误以为跨设备同步。

### API Surface Parity

所有入口必须共享同一套 core/store/site 模块：

- 本地 CLI。
- GitHub Actions。
- 后续 Web API。
- 后续外部插件 host。

不得让 dashboard、collector 或 importer 各自实现不同的去重、评分和公开字段白名单逻辑。

## Acceptance Criteria

### Functional Requirements

- [ ] `F:\AiProject\InfoRadar` 是独立 Rust workspace。
- [ ] `unreal` 是首个 board。
- [ ] `import-techradar` 能导入旧 UE 历史样本。
- [ ] 同一 canonical URL 只生成一个 item。
- [ ] `build-site --all --out public` 生成静态日报工作台。
- [ ] Dashboard 默认展示全量信息流/表格。
- [ ] 每条信息有标题、来源、原链接、发布时间或采集时间、摘要/片段、入选原因。
- [ ] 用户可在本地评分、收藏、标记已读。
- [ ] 来源失败在 UI 和 JSON 中可见。

### Non-Functional Requirements

- [ ] GitHub Pages 公开产物不包含 raw HTML、完整正文、密钥、内部日志。
- [ ] 新增普通 RSS/API 信源不需要改 Rust 代码。
- [ ] 新增普通板块只需要新增 board config 并选择已有 adapters。
- [ ] GitHub Actions 定时任务之外必须支持手动补采。
- [ ] Rust tests 覆盖 URL 归一、去重、评分、配置校验和公开导出 sanitizer。

### Quality Gates

- [ ] `cargo fmt --check`
- [ ] `cargo test --workspace`
- [ ] `cargo run -p inforadar-cli -- validate-config`
- [ ] `cargo run -p inforadar-cli -- import-techradar --from F:\AiProject\TechRadar`
- [ ] `cargo run -p inforadar-cli -- build-site --all --out public`
- [ ] 浏览器烟测：板块切换、全量列表、筛选、详情、评分、原链接。

## Success Metrics

- 连续 7 天自动生成 `unreal` 日报。
- 用户打开页面后 2 分钟内能完成今日新增和高价值信息浏览。
- 同 URL 重复条目数为 0。
- 单源失败不导致整板块失败。
- Pages artifact 中敏感/原始字段检查为 0 命中。

## Dependencies & Risks

- GitHub Actions schedule 是 best effort，必须依赖 `workflow_dispatch` 做补采。
- GitHub API 和 RSS 源可能限流，adapter 必须有 timeout、重试、限速和失败记录。
- SQLite artifact 是免费方案下的折中，长期历史和跨设备写入后续可能需要对象存储或云数据库。
- Rust 对复杂网页抓取/登录态采集不是 v1 重点，高风险抓取默认关闭。
- AI 摘要若未来接入 LLM，必须单独设计成本、正文抓取许可和公开字段边界。

## Documentation Plan

- [ ] 更新 `README.md`：定位、快速开始、命令。
- [ ] 新增 `docs/guides/adding-board/index.md`：如何新增板块。
- [ ] 新增 `docs/guides/adding-source/index.md`：如何新增信源。
- [ ] 新增 `docs/solutions/`：首轮实现完成后沉淀迁移与去重经验。
- [ ] 保持 `docs/DATA_POLICY.md` 与公开导出 sanitizer 一致。

## Sources & References

### Internal References

- `F:\AiProject\TechRadar\reports\index.json`：旧 UE 历史数据来源。
- `F:\AiProject\TechRadar\skills\tech-radar\scripts\daily-unreal.mjs`：旧日报生成行为参考。
- `F:\AiProject\TechRadar\skills\tech-radar\scripts\collectors\unreal-all.mjs`：旧 UE 信源参考。
- `F:\AiProject\InfoRadar\configs\boards\unreal.toml`：新 board 配置起点。

### External References

- Thoughtworks Build Your Own Radar: https://github.com/thoughtworks/build-your-own-radar
- Zalando Tech Radar: https://github.com/zalando/tech-radar
- QIWI Tech Radar: https://github.com/qiwi/tech-radar
- GitHub Pages custom workflows: https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages

## Ultra Goal Handoff

建议使用 `$ultragoal` 进行 durable execution。执行时以本计划文件为唯一主计划，不要回到旧 `TechRadar` 工程做大重构。

推荐命令：

```powershell
omx ultragoal create-goals --brief-file F:\AiProject\InfoRadar\docs\plans\2026-06-20-001-feat-inforadar-rust-daily-workbench-plan.md
omx ultragoal complete-goals
```
