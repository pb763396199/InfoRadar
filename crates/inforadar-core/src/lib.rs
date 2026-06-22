use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use url::Url;

pub const SCORE_VERSION: &str = "rules-v1";
pub const PUBLIC_TEXT_LIMIT: usize = 1200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub enabled: bool,
    pub url: String,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default = "default_risk_level")]
    pub risk_level: String,
    #[serde(default)]
    pub public_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationDraft {
    pub source_id: String,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
    pub published_at: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDraft {
    pub board_id: String,
    pub canonical_url: String,
    pub title: String,
    pub description: Option<String>,
    pub category: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreDraft {
    pub item_id: String,
    pub score_version: String,
    pub rank_score: i64,
    pub relevance: i64,
    pub reason: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicItem {
    pub id: String,
    pub board_id: String,
    pub title: String,
    pub url: String,
    pub source: String,
    pub sources: Vec<String>,
    pub category: String,
    pub description: String,
    pub published_at: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub rank_score: i64,
    pub relevance: i64,
    pub score_reason: String,
    pub evidence: Vec<String>,
    pub duplicate_count: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicIssue {
    pub schema_version: u32,
    pub generated_at: String,
    pub board_id: String,
    pub issue_date: String,
    pub totals: PublicTotals,
    pub source_health: Vec<PublicSourceHealth>,
    pub items: Vec<PublicItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicTotals {
    pub total_items: usize,
    pub high_value_items: usize,
    pub new_items: usize,
    pub sources: usize,
    pub failed_sources: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSourceHealth {
    pub source_id: String,
    pub source: String,
    pub status: String,
    pub count: usize,
    pub reason: String,
}

pub fn load_board_config(path: impl AsRef<Path>) -> Result<BoardConfig> {
    let path = path.as_ref();
    let body = fs::read_to_string(path)
        .with_context(|| format!("read board config {}", path.display()))?;
    let config: BoardConfig =
        toml::from_str(&body).with_context(|| format!("parse board config {}", path.display()))?;
    validate_board_config(&config)?;
    Ok(config)
}

pub fn validate_board_config(config: &BoardConfig) -> Result<()> {
    if config.id.trim().is_empty() {
        return Err(anyhow!("board id is required"));
    }
    if config.name.trim().is_empty() {
        return Err(anyhow!("board name is required"));
    }
    for source in &config.sources {
        if source.id.trim().is_empty() || source.name.trim().is_empty() {
            return Err(anyhow!("source id/name are required"));
        }
        if !matches!(source.kind.as_str(), "rss" | "github_search" | "json_feed") {
            return Err(anyhow!(
                "unsupported source kind '{}' for {}",
                source.kind,
                source.id
            ));
        }
        Url::parse(&source.url).with_context(|| format!("invalid source url for {}", source.id))?;
    }
    Ok(())
}

pub fn canonicalize_url(input: &str) -> String {
    match Url::parse(input.trim()) {
        Ok(mut url) => {
            url.set_fragment(None);
            let mut pairs: Vec<(String, String)> = url
                .query_pairs()
                .filter(|(k, _)| !is_tracking_param(k))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            pairs.sort();
            url.set_query(None);
            if url.path() != "/" {
                let trimmed = url.path().trim_end_matches('/').to_string();
                if !trimmed.is_empty() {
                    url.set_path(&trimmed);
                }
            }
            if !pairs.is_empty() {
                let query = pairs
                    .into_iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("&");
                url.set_query(Some(&query));
            }
            url.to_string()
        }
        Err(_) => input.trim().to_string(),
    }
}

pub fn is_public_http_url(input: &str) -> bool {
    Url::parse(input)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
}

pub fn public_text(input: &str) -> String {
    let text = strip_html(input)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&text, PUBLIC_TEXT_LIMIT)
}

pub fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            output.push_str("...");
            return output;
        }
        output.push(ch);
    }
    output
}

pub fn stable_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hex::encode(&hasher.finalize()[..16])
}

pub fn normalize_observation(
    board: &BoardConfig,
    observation: &ObservationDraft,
    now: &str,
) -> ItemDraft {
    let canonical_url = canonicalize_url(&observation.url);
    let description = observation.description.as_deref().map(public_text);
    let category = observation.category.clone().unwrap_or_else(|| {
        infer_category(
            board,
            &observation.title,
            observation.description.as_deref().unwrap_or(""),
        )
    });
    ItemDraft {
        board_id: board.id.clone(),
        canonical_url,
        title: observation.title.trim().to_string(),
        description,
        category,
        first_seen_at: observation
            .published_at
            .clone()
            .unwrap_or_else(|| now.to_string()),
        last_seen_at: now.to_string(),
    }
}

pub fn score_item(
    board: &BoardConfig,
    item_id: &str,
    item: &ItemDraft,
    source_name: &str,
) -> ScoreDraft {
    let haystack = format!(
        "{} {}",
        item.title.to_lowercase(),
        item.description.as_deref().unwrap_or("").to_lowercase()
    );
    let mut evidence = Vec::new();
    let mut relevance = 20;
    for keyword in &board.keywords {
        if haystack.contains(&keyword.to_lowercase()) {
            relevance += 12;
            evidence.push(format!("keyword: {}", keyword));
        }
    }
    relevance = relevance.min(100);
    let mut rank_score = relevance + category_weight(&item.category) + source_weight(source_name);
    if item.description.as_deref().unwrap_or("").len() > 80 {
        rank_score += 5;
    }
    ScoreDraft {
        item_id: item_id.to_string(),
        score_version: SCORE_VERSION.to_string(),
        rank_score: rank_score.min(100),
        relevance,
        reason: format!(
            "{} relevance, category {}, source {}",
            relevance, item.category, source_name
        ),
        evidence,
    }
}

pub fn require_public_http_url(input: &str) -> Result<String> {
    let canonical = canonicalize_url(input);
    if !is_public_http_url(&canonical) {
        return Err(anyhow!("public item URL must be http/https: {}", input));
    }
    Ok(canonical)
}

pub fn sanitize_public_json(value: &serde_json::Value) -> Result<()> {
    sanitize_value(value, "$")?;
    Ok(())
}

fn sanitize_value(value: &serde_json::Value, path: &str) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key_lower = key.to_ascii_lowercase();
                for forbidden in [
                    "rawfields",
                    "raw_html",
                    "raw html",
                    "full_body",
                    "body_html",
                    "api_key",
                    "secret",
                    "access_token",
                    "refresh_token",
                    "authorization",
                    "stacktrace",
                ] {
                    if key_lower.contains(forbidden) {
                        return Err(anyhow!(
                            "public export contains forbidden key: {path}.{key}"
                        ));
                    }
                }
                sanitize_value(value, &format!("{path}.{key}"))?;
            }
        }
        serde_json::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                sanitize_value(value, &format!("{path}[{index}]"))?;
            }
        }
        serde_json::Value::String(text) => {
            if text.chars().count() > PUBLIC_TEXT_LIMIT * 2 {
                return Err(anyhow!("public export string too long at {path}"));
            }
            let lower = text.to_ascii_lowercase();
            for forbidden in ["bearer ", "-----begin", "password="] {
                if lower.contains(forbidden) {
                    return Err(anyhow!(
                        "public export contains forbidden value marker at {path}"
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn infer_category(board: &BoardConfig, title: &str, description: &str) -> String {
    let text = format!("{} {}", title.to_lowercase(), description.to_lowercase());
    let category =
        if text.contains("github") || text.contains("plugin") || text.contains("open source") {
            "Open Source"
        } else if text.contains("marketplace") || text.contains("fab") || text.contains("asset") {
            "Marketplace"
        } else if text.contains("youtube") || text.contains("video") || text.contains("tutorial") {
            "Video"
        } else if text.contains("gdc") || text.contains("siggraph") || text.contains("talk") {
            "Talks"
        } else if text.contains("epic") || text.contains("release") {
            "Official"
        } else {
            board
                .categories
                .first()
                .map(String::as_str)
                .unwrap_or("News")
        };
    category.to_string()
}

fn category_weight(category: &str) -> i64 {
    match category {
        "Official" => 20,
        "Open Source" => 15,
        "News" => 12,
        "Talks" => 10,
        "Marketplace" => 8,
        _ => 5,
    }
}

fn source_weight(source: &str) -> i64 {
    match source {
        "GitHub" | "GitHub Unreal" => 10,
        "Google News Unreal" => 8,
        _ => 5,
    }
}

fn is_tracking_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "mc_cid" | "mc_eid" | "ref" | "snr"
        )
}

fn strip_html(input: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn default_rate_limit() -> u32 {
    6
}
fn default_timeout() -> u64 {
    20
}
fn default_max_items() -> usize {
    50
}
fn default_risk_level() -> String {
    "stable".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_url_removes_tracking_and_fragment() {
        let url = canonicalize_url("https://example.com/a/?utm_source=x&b=2&a=1#frag");
        assert_eq!(url, "https://example.com/a?a=1&b=2");
        let steam = canonicalize_url("https://store.steampowered.com/app/1/Game/?snr=abc");
        assert_eq!(steam, "https://store.steampowered.com/app/1/Game");
    }

    #[test]
    fn sanitizer_rejects_raw_fields() {
        let value = serde_json::json!({"rawFields": {"secret": "x"}});
        assert!(sanitize_public_json(&value).is_err());
    }

    #[test]
    fn public_text_truncates_long_content() {
        let text = public_text(&"a".repeat(PUBLIC_TEXT_LIMIT + 20));
        assert!(text.chars().count() <= PUBLIC_TEXT_LIMIT + 3);
        assert!(text.ends_with("..."));
    }

    #[test]
    fn public_url_allows_only_http() {
        assert!(is_public_http_url("https://example.com/a"));
        assert!(!is_public_http_url("javascript:alert(1)"));
    }
}
