use anyhow::{Context, Result};
use inforadar_core::{
    normalize_observation, now_rfc3339, public_text, require_public_http_url, sanitize_public_json,
    score_item, stable_id, BoardConfig, ObservationDraft, PublicIssue, PublicItem,
    PublicSourceHealth, PublicTotals, SCORE_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::{fs, path::Path};

pub struct Store {
    conn: Connection,
}

pub struct SourceRunResultDraft<'a> {
    pub run_id: &'a str,
    pub board_id: &'a str,
    pub run_date: &'a str,
    pub source_id: &'a str,
    pub status: &'a str,
    pub count: usize,
    pub reason: &'a str,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("open sqlite {}", path.display()))?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS boards (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              description TEXT NOT NULL,
              config_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sources (
              id TEXT PRIMARY KEY,
              board_id TEXT NOT NULL,
              name TEXT NOT NULL,
              kind TEXT NOT NULL,
              url TEXT NOT NULL,
              enabled INTEGER NOT NULL,
              risk_level TEXT NOT NULL,
              public_fields_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS collection_runs (
              id TEXT PRIMARY KEY,
              board_id TEXT NOT NULL,
              run_date TEXT NOT NULL,
              started_at TEXT NOT NULL,
              finished_at TEXT,
              status TEXT NOT NULL,
              error TEXT
            );
            CREATE TABLE IF NOT EXISTS source_run_results (
              run_id TEXT NOT NULL,
              board_id TEXT NOT NULL,
              run_date TEXT NOT NULL,
              source_id TEXT NOT NULL,
              status TEXT NOT NULL,
              count INTEGER NOT NULL,
              reason TEXT NOT NULL,
              PRIMARY KEY (run_id, source_id)
            );
            CREATE TABLE IF NOT EXISTS observations (
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
            CREATE TABLE IF NOT EXISTS items (
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
            CREATE UNIQUE INDEX IF NOT EXISTS idx_items_board_url ON items(board_id, canonical_url);
            CREATE TABLE IF NOT EXISTS item_sources (
              item_id TEXT NOT NULL,
              observation_id TEXT NOT NULL,
              source_id TEXT NOT NULL,
              PRIMARY KEY (item_id, observation_id)
            );
            CREATE TABLE IF NOT EXISTS scores (
              item_id TEXT NOT NULL,
              score_version TEXT NOT NULL,
              rank_score INTEGER NOT NULL,
              relevance INTEGER NOT NULL,
              reason TEXT NOT NULL,
              evidence_json TEXT NOT NULL,
              PRIMARY KEY (item_id, score_version)
            );
            CREATE TABLE IF NOT EXISTS daily_issues (
              id TEXT PRIMARY KEY,
              board_id TEXT NOT NULL,
              issue_date TEXT NOT NULL,
              generated_at TEXT NOT NULL,
              summary_json TEXT NOT NULL,
              UNIQUE(board_id, issue_date)
            );
            CREATE TABLE IF NOT EXISTS publish_snapshots (
              id TEXT PRIMARY KEY,
              issue_id TEXT NOT NULL,
              output_dir TEXT NOT NULL,
              generated_at TEXT NOT NULL,
              public_manifest_json TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_board(&self, board: &BoardConfig) -> Result<()> {
        self.conn.execute(
            "INSERT INTO boards(id,name,description,config_json) VALUES(?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, config_json=excluded.config_json",
            params![board.id, board.name, board.description, serde_json::to_string(board)?],
        )?;
        for source in &board.sources {
            self.conn.execute(
                "INSERT INTO sources(id,board_id,name,kind,url,enabled,risk_level,public_fields_json) VALUES(?,?,?,?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET board_id=excluded.board_id,name=excluded.name,kind=excluded.kind,url=excluded.url,enabled=excluded.enabled,risk_level=excluded.risk_level,public_fields_json=excluded.public_fields_json",
                params![
                    source.id,
                    board.id,
                    source.name,
                    source.kind,
                    source.url,
                    if source.enabled { 1 } else { 0 },
                    source.risk_level,
                    serde_json::to_string(&source.public_fields)?
                ],
            )?;
        }
        Ok(())
    }

    pub fn begin_run(&self, board_id: &str, run_date: &str) -> Result<String> {
        let id = stable_id(&[board_id, run_date, &now_rfc3339()]);
        self.conn.execute(
            "INSERT INTO collection_runs(id,board_id,run_date,started_at,status) VALUES(?,?,?,?,?)",
            params![id, board_id, run_date, now_rfc3339(), "running"],
        )?;
        Ok(id)
    }

    pub fn finish_run(&self, run_id: &str, status: &str, error: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE collection_runs SET finished_at=?, status=?, error=? WHERE id=?",
            params![now_rfc3339(), status, error, run_id],
        )?;
        Ok(())
    }

    pub fn record_source_result(&self, result: SourceRunResultDraft<'_>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO source_run_results(run_id,board_id,run_date,source_id,status,count,reason)
             VALUES(?,?,?,?,?,?,?)
             ON CONFLICT(run_id,source_id) DO UPDATE SET status=excluded.status,count=excluded.count,reason=excluded.reason",
            params![
                result.run_id,
                result.board_id,
                result.run_date,
                result.source_id,
                result.status,
                result.count as i64,
                result.reason
            ],
        )?;
        Ok(())
    }

    pub fn ingest_observation(
        &self,
        board: &BoardConfig,
        run_id: &str,
        observation: &ObservationDraft,
    ) -> Result<String> {
        let now = now_rfc3339();
        let run_date: String = self.conn.query_row(
            "SELECT run_date FROM collection_runs WHERE id=?",
            params![run_id],
            |row| row.get(0),
        )?;
        let canonical_url = require_public_http_url(&observation.url)?;
        let observation_id = stable_id(&[
            &board.id,
            &run_date,
            &observation.source_id,
            &canonical_url,
            &observation.title,
        ]);
        let inserted_observations = self.conn.execute(
            "INSERT OR IGNORE INTO observations(id,run_id,source_id,canonical_url,title,description,published_at,collected_at,raw_json)
             VALUES(?,?,?,?,?,?,?,?,?)",
            params![
                observation_id,
                run_id,
                observation.source_id,
                canonical_url,
                observation.title,
                observation.description,
                observation.published_at,
                now,
                serde_json::to_string(&observation.raw)?
            ],
        )?;

        let item = normalize_observation(board, observation, &now);
        let item_id = stable_id(&[&item.board_id, &item.canonical_url]);
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM items WHERE board_id=? AND canonical_url=?",
                params![item.board_id, item.canonical_url],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some() && inserted_observations > 0 {
            self.conn.execute(
                "UPDATE items SET last_seen_at=?, duplicate_count=duplicate_count+1 WHERE id=?",
                params![item.last_seen_at, item_id],
            )?;
        } else if existing.is_none() {
            self.conn.execute(
                "INSERT INTO items(id,board_id,canonical_url,title,description,category,first_seen_at,last_seen_at,duplicate_count)
                 VALUES(?,?,?,?,?,?,?,?,?)",
                params![
                    item_id,
                    item.board_id,
                    item.canonical_url,
                    item.title,
                    item.description,
                    item.category,
                    item.first_seen_at,
                    item.last_seen_at,
                    1
                ],
            )?;
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO item_sources(item_id,observation_id,source_id) VALUES(?,?,?)",
            params![item_id, observation_id, observation.source_id],
        )?;
        let source_name = self
            .source_name(&observation.source_id)?
            .unwrap_or_else(|| observation.source_id.clone());
        let score = score_item(board, &item_id, &item, &source_name);
        self.conn.execute(
            "INSERT INTO scores(item_id,score_version,rank_score,relevance,reason,evidence_json) VALUES(?,?,?,?,?,?)
             ON CONFLICT(item_id,score_version) DO UPDATE SET rank_score=excluded.rank_score,relevance=excluded.relevance,reason=excluded.reason,evidence_json=excluded.evidence_json",
            params![score.item_id, score.score_version, score.rank_score, score.relevance, score.reason, serde_json::to_string(&score.evidence)?],
        )?;
        Ok(item_id)
    }

    pub fn build_issue(&self, board_id: &str, date: &str) -> Result<PublicIssue> {
        let items = self.public_items(board_id, date)?;
        let health = self.source_health(board_id, date)?;
        let totals = PublicTotals {
            total_items: items.len(),
            high_value_items: items.iter().filter(|item| item.rank_score >= 70).count(),
            new_items: items
                .iter()
                .filter(|item| item.first_seen_at.starts_with(date))
                .count(),
            sources: health.len(),
            failed_sources: health
                .iter()
                .filter(|source| source.status == "failed")
                .count(),
        };
        let issue = PublicIssue {
            schema_version: 1,
            generated_at: now_rfc3339(),
            board_id: board_id.to_string(),
            issue_date: date.to_string(),
            totals,
            source_health: health,
            items,
        };
        let value = serde_json::to_value(&issue)?;
        sanitize_public_json(&value)?;
        let issue_id = stable_id(&[board_id, date]);
        self.conn.execute(
            "INSERT INTO daily_issues(id,board_id,issue_date,generated_at,summary_json) VALUES(?,?,?,?,?)
             ON CONFLICT(board_id,issue_date) DO UPDATE SET generated_at=excluded.generated_at,summary_json=excluded.summary_json",
            params![issue_id, board_id, date, issue.generated_at, serde_json::to_string(&issue)?],
        )?;
        Ok(issue)
    }

    pub fn issues(&self) -> Result<Vec<PublicIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT summary_json FROM daily_issues ORDER BY issue_date DESC, board_id ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut issues = Vec::new();
        for row in rows {
            issues.push(serde_json::from_str(&row?)?);
        }
        Ok(issues)
    }

    pub fn record_publish_snapshot(&self, output_dir: &str) -> Result<()> {
        let generated_at = now_rfc3339();
        let id = stable_id(&["publish", output_dir, &generated_at]);
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "outputDir": output_dir,
            "generatedAt": generated_at,
            "publicOnly": true
        });
        sanitize_public_json(&manifest)?;
        self.conn.execute(
            "INSERT INTO publish_snapshots(id,issue_id,output_dir,generated_at,public_manifest_json) VALUES(?,?,?,?,?)",
            params![id, "all", output_dir, generated_at, serde_json::to_string(&manifest)?],
        )?;
        Ok(())
    }

    pub fn import_techradar(&self, root: impl AsRef<Path>, board: &BoardConfig) -> Result<usize> {
        self.upsert_board(board)?;
        let index_path = root.as_ref().join("reports").join("index.json");
        let value: Value = serde_json::from_str(
            &fs::read_to_string(&index_path)
                .with_context(|| format!("read {}", index_path.display()))?,
        )?;
        let days = value
            .get("days")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut count = 0;
        for day in days {
            let date = day.get("date").and_then(Value::as_str).unwrap_or("unknown");
            let run_id = self.begin_run(&board.id, date)?;
            let items = day
                .get("allItems")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for item in items {
                let url = item
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let title = item
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if url.is_empty() || title.is_empty() {
                    continue;
                }
                let source = item
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("Imported")
                    .to_string();
                let source_id = format!(
                    "imported-{}",
                    source
                        .to_lowercase()
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                );
                self.ensure_import_source(board, &source_id, &source)?;
                let observation = ObservationDraft {
                    source_id,
                    title,
                    url,
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .map(public_text),
                    published_at: item
                        .get("updatedAt")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .or_else(|| Some(format!("{}T00:00:00Z", date))),
                    category: item
                        .get("category")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    raw: public_raw_subset(&item),
                };
                self.ingest_observation(board, &run_id, &observation)?;
                count += 1;
            }
            self.finish_run(&run_id, "success", None)?;
            self.build_issue(&board.id, date)?;
        }
        Ok(count)
    }

    fn public_items(&self, board_id: &str, date: &str) -> Result<Vec<PublicItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT i.id, i.board_id, i.title, i.canonical_url,
                   COALESCE(group_concat(DISTINCT s.name), 'Unknown') AS source_names,
                   i.category, COALESCE(i.description, ''), i.first_seen_at, i.last_seen_at,
                   COALESCE(sc.rank_score, 0), COALESCE(sc.relevance, 0), COALESCE(sc.reason, ''),
                   COALESCE(sc.evidence_json, '[]'), i.duplicate_count
            FROM items i
            JOIN item_sources isrc ON isrc.item_id = i.id
            JOIN observations o ON o.id = isrc.observation_id
            JOIN collection_runs cr ON cr.id = o.run_id
            LEFT JOIN sources s ON s.id = isrc.source_id
            LEFT JOIN scores sc ON sc.item_id = i.id AND sc.score_version = ?
            WHERE i.board_id = ? AND cr.run_date = ?
            GROUP BY i.id
            ORDER BY COALESCE(sc.rank_score, 0) DESC, i.last_seen_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![SCORE_VERSION, board_id, date], |row| {
            let evidence_json: String = row.get(12)?;
            let source_names: String = row.get(4)?;
            let sources = source_names
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let raw_url: String = row.get(3)?;
            let url = require_public_http_url(&raw_url).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    err.into(),
                )
            })?;
            let description: String = row.get(6)?;
            Ok(PublicItem {
                id: row.get(0)?,
                board_id: row.get(1)?,
                title: row.get(2)?,
                url,
                source: sources
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string()),
                sources,
                category: row.get(5)?,
                description: public_text(&description),
                published_at: row.get(8)?,
                first_seen_at: row.get(7)?,
                last_seen_at: row.get(8)?,
                rank_score: row.get(9)?,
                relevance: row.get(10)?,
                score_reason: row.get(11)?,
                evidence: serde_json::from_str(&evidence_json).unwrap_or_default(),
                duplicate_count: row.get(13)?,
                status: "unread".to_string(),
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    fn source_health(&self, board_id: &str, date: &str) -> Result<Vec<PublicSourceHealth>> {
        let latest_run: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM collection_runs WHERE board_id=? AND run_date=? ORDER BY started_at DESC LIMIT 1",
                params![board_id, date],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(run_id) = latest_run {
            let result_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM source_run_results WHERE run_id=?",
                params![run_id],
                |row| row.get(0),
            )?;
            if result_count > 0 {
                let mut stmt = self.conn.prepare(
                    r#"
                    SELECT s.id, s.name, COALESCE(r.status, 'empty'), COALESCE(r.count, 0), COALESCE(r.reason, '')
                    FROM sources s
                    LEFT JOIN source_run_results r ON r.source_id = s.id AND r.run_id = ?
                    WHERE s.board_id = ?
                    ORDER BY s.name
                    "#,
                )?;
                let rows = stmt.query_map(params![run_id, board_id], |row| {
                    Ok(PublicSourceHealth {
                        source_id: row.get(0)?,
                        source: row.get(1)?,
                        status: row.get(2)?,
                        count: row.get::<_, i64>(3)? as usize,
                        reason: row.get(4)?,
                    })
                })?;
                let mut health = Vec::new();
                for row in rows {
                    health.push(row?);
                }
                return Ok(health);
            }
        }
        let mut stmt = self.conn.prepare(
            r#"
            SELECT s.id, s.name,
                   CASE WHEN COUNT(cr.id) > 0 THEN 'active' ELSE 'empty' END AS status,
                   COUNT(cr.id) AS count
            FROM sources s
            LEFT JOIN observations o ON o.source_id = s.id
            LEFT JOIN collection_runs cr ON cr.id = o.run_id AND cr.run_date = ?
            WHERE s.board_id = ?
            GROUP BY s.id, s.name
            ORDER BY s.name
            "#,
        )?;
        let rows = stmt.query_map(params![date, board_id], |row| {
            Ok(PublicSourceHealth {
                source_id: row.get(0)?,
                source: row.get(1)?,
                status: row.get(2)?,
                count: row.get::<_, i64>(3)? as usize,
                reason: String::new(),
            })
        })?;
        let mut health = Vec::new();
        for row in rows {
            health.push(row?);
        }
        Ok(health)
    }

    fn source_name(&self, source_id: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT name FROM sources WHERE id=?",
                params![source_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn ensure_import_source(
        &self,
        board: &BoardConfig,
        source_id: &str,
        source_name: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sources(id,board_id,name,kind,url,enabled,risk_level,public_fields_json) VALUES(?,?,?,?,?,?,?,?)",
            params![source_id, board.id, source_name, "imported", "", 1, "stable", "[]"],
        )?;
        Ok(())
    }
}

fn public_raw_subset(item: &Value) -> Value {
    serde_json::json!({
        "metric": item.get("metric").cloned().unwrap_or(Value::Null),
        "rankScore": item.get("rankScore").cloned().unwrap_or(Value::Null),
        "relevance": item.get("relevance").cloned().unwrap_or(Value::Null),
        "evidence": item.get("evidence").cloned().unwrap_or(Value::Null)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inforadar_core::SourceConfig;

    fn test_board() -> BoardConfig {
        BoardConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test board".to_string(),
            default: true,
            keywords: vec!["unreal".to_string()],
            categories: vec!["News".to_string()],
            sources: vec![SourceConfig {
                id: "src".to_string(),
                name: "Source".to_string(),
                kind: "rss".to_string(),
                enabled: true,
                url: "https://example.com/feed.xml".to_string(),
                rate_limit_per_minute: 1,
                timeout_seconds: 1,
                max_items: 10,
                risk_level: "stable".to_string(),
                public_fields: vec![],
            }],
        }
    }

    fn temp_db(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "inforadar-test-{}.db",
            inforadar_core::stable_id(&[name, &inforadar_core::now_rfc3339()])
        ))
    }

    #[test]
    fn source_failure_is_visible_in_issue() {
        let db = temp_db("source_failure");
        let store = Store::open(&db).unwrap();
        let board = BoardConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test board".to_string(),
            default: true,
            keywords: vec![],
            categories: vec!["News".to_string()],
            sources: vec![SourceConfig {
                id: "bad".to_string(),
                name: "Bad Source".to_string(),
                kind: "rss".to_string(),
                enabled: true,
                url: "https://example.invalid/feed.xml".to_string(),
                rate_limit_per_minute: 1,
                timeout_seconds: 1,
                max_items: 1,
                risk_level: "stable".to_string(),
                public_fields: vec![],
            }],
        };
        store.upsert_board(&board).unwrap();
        let run_id = store.begin_run("test", "2026-06-22").unwrap();
        store
            .record_source_result(SourceRunResultDraft {
                run_id: &run_id,
                board_id: "test",
                run_date: "2026-06-22",
                source_id: "bad",
                status: "failed",
                count: 0,
                reason: "timeout",
            })
            .unwrap();
        store
            .finish_run(&run_id, "partial", Some("bad failed"))
            .unwrap();
        let issue = store.build_issue("test", "2026-06-22").unwrap();
        assert_eq!(issue.totals.failed_sources, 1);
        assert_eq!(issue.source_health[0].status, "failed");
        let _ = std::fs::remove_file(db);
    }

    #[test]
    fn daily_issues_are_date_scoped_and_reimport_is_idempotent() {
        let db = temp_db("daily_idempotent");
        let store = Store::open(&db).unwrap();
        let board = test_board();
        store.upsert_board(&board).unwrap();
        let observation = ObservationDraft {
            source_id: "src".to_string(),
            title: "Unreal Engine update".to_string(),
            url: "https://example.com/news?utm_source=test".to_string(),
            description: Some("A short update about Unreal Engine.".to_string()),
            published_at: Some("2026-06-21T00:00:00Z".to_string()),
            category: Some("News".to_string()),
            raw: serde_json::json!({"metric":"fixture"}),
        };

        let run_21 = store.begin_run("test", "2026-06-21").unwrap();
        store
            .ingest_observation(&board, &run_21, &observation)
            .unwrap();
        store.finish_run(&run_21, "success", None).unwrap();
        let issue_21 = store.build_issue("test", "2026-06-21").unwrap();

        let run_22 = store.begin_run("test", "2026-06-22").unwrap();
        store
            .ingest_observation(&board, &run_22, &observation)
            .unwrap();
        store.finish_run(&run_22, "success", None).unwrap();
        let issue_22 = store.build_issue("test", "2026-06-22").unwrap();

        let repeat_22 = store.begin_run("test", "2026-06-22").unwrap();
        store
            .ingest_observation(&board, &repeat_22, &observation)
            .unwrap();
        store.finish_run(&repeat_22, "success", None).unwrap();
        let repeated_issue_22 = store.build_issue("test", "2026-06-22").unwrap();

        assert_eq!(issue_21.items.len(), 1);
        assert_eq!(issue_22.items.len(), 1);
        assert_eq!(repeated_issue_22.items.len(), 1);
        assert_eq!(repeated_issue_22.items[0].duplicate_count, 2);
        let _ = std::fs::remove_file(db);
    }
}
