use anyhow::{Context, Result};
use inforadar_core::{sanitize_public_json, PublicIssue};
use std::{fs, path::Path};

pub fn build_site(issues: &[PublicIssue], out_dir: impl AsRef<Path>) -> Result<()> {
    let out_dir = out_dir.as_ref();
    if out_dir.exists() {
        let data_dir = out_dir.join("data");
        if data_dir.exists() {
            fs::remove_dir_all(&data_dir)
                .with_context(|| format!("clean {}", data_dir.display()))?;
        }
        let index_file = out_dir.join("index.html");
        if index_file.exists() {
            fs::remove_file(&index_file)
                .with_context(|| format!("clean {}", index_file.display()))?;
        }
    }
    let data_dir = out_dir.join("data");
    fs::create_dir_all(&data_dir)?;

    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "boards": boards_from_issues(issues),
        "issues": issues.iter().map(|issue| {
            serde_json::json!({
                "boardId": issue.board_id,
                "date": issue.issue_date,
                "path": format!("data/{}/{}.json", issue.board_id, issue.issue_date)
            })
        }).collect::<Vec<_>>()
    });
    sanitize_public_json(&manifest)?;
    fs::write(
        data_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    for issue in issues {
        let board_dir = data_dir.join(&issue.board_id);
        fs::create_dir_all(&board_dir)?;
        let value = serde_json::to_value(issue)?;
        sanitize_public_json(&value)?;
        fs::write(
            board_dir.join(format!("{}.json", issue.issue_date)),
            serde_json::to_string_pretty(issue)?,
        )?;
    }

    fs::write(out_dir.join("index.html"), dashboard_html()).context("write dashboard")?;
    Ok(())
}

fn boards_from_issues(issues: &[PublicIssue]) -> Vec<serde_json::Value> {
    let mut boards = Vec::<String>::new();
    for issue in issues {
        if !boards.contains(&issue.board_id) {
            boards.push(issue.board_id.clone());
        }
    }
    boards
        .into_iter()
        .map(|id| serde_json::json!({"id": id, "name": title_case(&id)}))
        .collect()
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn dashboard_html() -> &'static str {
    r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>InfoRadar</title>
  <style>
    :root { color-scheme: light; --bg:#f6f8fb; --panel:#fff; --line:#d8dee8; --text:#172033; --muted:#667085; --accent:#0f766e; --warn:#b45309; }
    * { box-sizing:border-box; }
    body { margin:0; font-family:Inter, Segoe UI, Arial, sans-serif; background:var(--bg); color:var(--text); }
    header { padding:18px 24px; border-bottom:1px solid var(--line); background:var(--panel); display:flex; justify-content:space-between; gap:16px; align-items:center; }
    h1 { margin:0; font-size:22px; letter-spacing:0; }
    main { padding:18px 24px; display:grid; gap:14px; }
    .tabs,.filters,.stats { display:flex; flex-wrap:wrap; gap:8px; align-items:center; }
    button, select, input { border:1px solid var(--line); background:#fff; color:var(--text); border-radius:6px; padding:8px 10px; font:inherit; }
    button.active { background:var(--accent); color:#fff; border-color:var(--accent); }
    .stat { background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:12px 14px; min-width:130px; }
    .stat b { display:block; font-size:22px; }
    .layout { display:grid; grid-template-columns:minmax(0,1fr) 420px; gap:14px; align-items:start; }
    .panel { background:var(--panel); border:1px solid var(--line); border-radius:8px; overflow:hidden; }
    table { width:100%; border-collapse:collapse; table-layout:fixed; }
    th,td { border-bottom:1px solid var(--line); padding:10px; text-align:left; vertical-align:top; font-size:13px; }
    th { background:#f9fafb; color:#475467; position:sticky; top:0; }
    tr { cursor:pointer; }
    tr:hover { background:#f5fbfa; }
    .title { font-weight:700; }
    .muted { color:var(--muted); }
    .pill { display:inline-flex; border:1px solid var(--line); border-radius:999px; padding:2px 7px; font-size:12px; margin-right:4px; }
    .score { font-weight:700; color:var(--accent); }
    aside { position:sticky; top:12px; }
    .detail { padding:16px; display:grid; gap:12px; }
    .detail h2 { margin:0; font-size:18px; }
    .stars button { border:none; background:transparent; font-size:22px; padding:2px; color:#d0a300; }
    .source-health { display:grid; gap:6px; padding:12px; }
    .hidden { display:none; }
    a { color:#0f5fbb; }
    @media (max-width: 980px) { .layout { grid-template-columns:1fr; } aside { position:static; } }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>InfoRadar 情报日报工作台</h1>
      <div class="muted">全量信息优先，泳道和 Top 榜只是辅助视图</div>
    </div>
    <div class="tabs" id="boardTabs"></div>
  </header>
  <main>
    <section class="filters">
      <select id="dateSelect"></select>
      <input id="searchInput" placeholder="搜索标题、摘要、来源">
      <select id="statusFilter">
        <option value="all">全部</option>
        <option value="new">新增</option>
        <option value="high">高价值</option>
        <option value="unread">未读</option>
        <option value="starred">已收藏</option>
      </select>
      <select id="sourceFilter"><option value="all">全部来源</option></select>
      <select id="categoryFilter"><option value="all">全部类别</option></select>
    </section>
    <section class="stats" id="stats"></section>
    <section class="layout">
      <div class="panel">
        <table>
          <thead><tr><th style="width:42%">信息</th><th>来源</th><th>类别</th><th>评分</th><th>状态</th></tr></thead>
          <tbody id="itemsBody"></tbody>
        </table>
      </div>
      <aside class="panel">
        <div class="detail" id="detail">
          <h2>选择一条信息</h2>
          <p class="muted">详情会显示摘要、证据、来源、历史和本地评分。</p>
        </div>
        <div class="source-health" id="sourceHealth"></div>
      </aside>
    </section>
  </main>
<script>
const state = { manifest:null, issue:null, board:null, date:null, ratings: JSON.parse(localStorage.getItem('inforadarRatings') || '{}') };
const $ = id => document.getElementById(id);

async function init(){
  state.manifest = await fetch('data/manifest.json').then(r=>r.json());
  state.board = state.manifest.boards[0]?.id;
  renderBoards();
  await loadLatest();
}

function renderBoards(){
  $('boardTabs').innerHTML = state.manifest.boards.map(b => `<button class="${b.id===state.board?'active':''}" onclick="selectBoard('${b.id}')">${b.name}</button>`).join('');
}
async function selectBoard(board){ state.board = board; renderBoards(); await loadLatest(); }
async function loadLatest(){
  const issues = state.manifest.issues.filter(i=>i.boardId===state.board);
  issues.sort((a,b)=>b.date.localeCompare(a.date));
  $('dateSelect').innerHTML = issues.map(i=>`<option value="${i.date}">${i.date}</option>`).join('');
  state.date = issues[0]?.date;
  $('dateSelect').value = state.date;
  await loadIssue();
}
async function loadIssue(){
  if(!state.board || !state.date) return;
  state.issue = await fetch(`data/${state.board}/${state.date}.json`).then(r=>r.json());
  populateFilters();
  render();
}
function populateFilters(){
  const sources = [...new Set(state.issue.items.map(i=>i.source))].sort();
  const categories = [...new Set(state.issue.items.map(i=>i.category))].sort();
  $('sourceFilter').innerHTML = '<option value="all">全部来源</option>' + sources.map(s=>`<option>${escapeHtml(s)}</option>`).join('');
  $('categoryFilter').innerHTML = '<option value="all">全部类别</option>' + categories.map(c=>`<option>${escapeHtml(c)}</option>`).join('');
}
function render(){
  renderStats();
  renderHealth();
  const q = $('searchInput').value.toLowerCase();
  const status = $('statusFilter').value;
  const source = $('sourceFilter').value;
  const category = $('categoryFilter').value;
  const items = state.issue.items.filter(item => {
    const rating = state.ratings[item.id] || {};
    const text = `${item.title} ${item.description} ${item.source}`.toLowerCase();
    if(q && !text.includes(q)) return false;
    if(source !== 'all' && item.source !== source) return false;
    if(category !== 'all' && item.category !== category) return false;
    if(status === 'new' && !item.first_seen_at.startsWith(state.issue.issue_date)) return false;
    if(status === 'high' && item.rank_score < 70) return false;
    if(status === 'unread' && rating.read) return false;
    if(status === 'starred' && !rating.starred) return false;
    return true;
  });
  $('itemsBody').innerHTML = items.map(item => {
    const rating = state.ratings[item.id] || {};
    return `<tr onclick="showDetail('${item.id}')">
      <td><div class="title">${escapeHtml(item.title)}</div><div class="muted">${escapeHtml(item.description || '').slice(0,160)}</div></td>
      <td>${escapeHtml(item.source)}</td>
      <td><span class="pill">${escapeHtml(item.category)}</span></td>
      <td><span class="score">${item.rank_score}</span><div class="muted">${item.relevance}/100</div></td>
      <td>${rating.read?'已读':'未读'} ${rating.starred?'★':''}</td>
    </tr>`;
  }).join('');
}
function renderStats(){
  const t = state.issue.totals;
  $('stats').innerHTML = [
    ['总量', t.total_items], ['新增', t.new_items], ['高价值', t.high_value_items], ['来源', t.sources], ['失败来源', t.failed_sources]
  ].map(([k,v])=>`<div class="stat"><span class="muted">${k}</span><b>${v}</b></div>`).join('');
}
function renderHealth(){
  $('sourceHealth').innerHTML = '<b>来源健康</b>' + state.issue.source_health.map(s=>`<div><span class="pill">${escapeHtml(s.status)}</span>${escapeHtml(s.source)} <span class="muted">${s.count}</span></div>`).join('');
}
function showDetail(id){
  const item = state.issue.items.find(i=>i.id===id);
  const rating = state.ratings[id] || {stars:0, read:false, starred:false, note:''};
  const sources = Array.isArray(item.sources) && item.sources.length ? item.sources : [item.source];
  $('detail').innerHTML = `<h2>${escapeHtml(item.title)}</h2>
    <div>${sources.map(s=>`<span class="pill">${escapeHtml(s)}</span>`).join('')}<span class="pill">${escapeHtml(item.category)}</span><span class="pill">重复 ${item.duplicate_count}</span></div>
    <p>${escapeHtml(item.description || '无摘要')}</p>
    <div><b>入选原因</b><br>${escapeHtml(item.score_reason)}</div>
    <div><b>证据</b><br>${(item.evidence||[]).map(e=>`<span class="pill">${escapeHtml(e)}</span>`).join('') || '<span class="muted">暂无</span>'}</div>
    <div class="stars">${[1,2,3,4,5].map(n=>`<button onclick="setStars('${id}',${n})">${n<=rating.stars?'★':'☆'}</button>`).join('')}</div>
    <div class="filters">
      <button onclick="toggleRead('${id}')">${rating.read?'标记未读':'标记已读'}</button>
      <button onclick="toggleStarred('${id}')">${rating.starred?'取消收藏':'收藏'}</button>
    </div>
    <textarea id="note" rows="4" style="width:100%" placeholder="本地备注">${escapeHtml(rating.note||'')}</textarea>
    <button onclick="saveNote('${id}')">保存备注</button>
    ${safeUrl(item.url) ? `<a href="${safeUrl(item.url)}" target="_blank" rel="noopener">打开原始链接</a>` : '<span class="muted">原始链接无效</span>'}
    <div class="muted">首次出现：${escapeHtml(item.first_seen_at)} · 最近出现：${escapeHtml(item.last_seen_at)} · 本地评分仅保存在当前浏览器。</div>`;
}
function saveRatings(){ localStorage.setItem('inforadarRatings', JSON.stringify(state.ratings)); render(); }
function setStars(id, stars){ state.ratings[id] = {...(state.ratings[id]||{}), stars}; saveRatings(); showDetail(id); }
function toggleRead(id){ const r=state.ratings[id]||{}; state.ratings[id]={...r, read:!r.read}; saveRatings(); showDetail(id); }
function toggleStarred(id){ const r=state.ratings[id]||{}; state.ratings[id]={...r, starred:!r.starred}; saveRatings(); showDetail(id); }
function saveNote(id){ const r=state.ratings[id]||{}; state.ratings[id]={...r, note:$('note').value}; saveRatings(); showDetail(id); }
function escapeHtml(v){ return String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function safeUrl(v){ try { const u = new URL(String(v)); return ['http:','https:'].includes(u.protocol) ? escapeHtml(u.href) : ''; } catch { return ''; } }
['dateSelect','searchInput','statusFilter','sourceFilter','categoryFilter'].forEach(id => $(id).addEventListener('input', async e => {
  if(id==='dateSelect'){ state.date = e.target.value; await loadIssue(); } else render();
}));
init().catch(err => { document.body.innerHTML = '<pre>'+escapeHtml(err.stack || err)+'</pre>'; });
</script>
</body>
</html>
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_site_cleans_only_managed_files() {
        let out = std::env::temp_dir().join(format!(
            "inforadar-site-{}",
            inforadar_core::stable_id(&["site", &inforadar_core::now_rfc3339()])
        ));
        std::fs::create_dir_all(out.join("data")).unwrap();
        std::fs::write(out.join("data").join("stale.json"), "{}").unwrap();
        std::fs::write(out.join("index.html"), "stale").unwrap();
        std::fs::write(out.join("keep.txt"), "user file").unwrap();
        build_site(&[], &out).unwrap();
        assert!(!out.join("data").join("stale.json").exists());
        assert!(out.join("index.html").exists());
        assert!(out.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(out);
    }
}
