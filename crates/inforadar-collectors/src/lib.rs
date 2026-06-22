use anyhow::{anyhow, Context, Result};
use feed_rs::parser;
use inforadar_core::{ObservationDraft, SourceConfig};
use reqwest::blocking::Client;
use serde_json::Value;
use std::{io::Cursor, time::Duration};

pub fn collect_source(source: &SourceConfig) -> Result<Vec<ObservationDraft>> {
    match source.kind.as_str() {
        "rss" | "json_feed" => collect_rss(source),
        "github_search" => collect_github_search(source),
        other => Err(anyhow!("unsupported source kind: {}", other)),
    }
}

fn http_client(source: &SourceConfig) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(source.timeout_seconds))
        .user_agent("InfoRadar/0.1 (+https://github.com)")
        .build()
        .context("build http client")
}

fn collect_rss(source: &SourceConfig) -> Result<Vec<ObservationDraft>> {
    let bytes = http_client(source)?
        .get(&source.url)
        .send()?
        .error_for_status()?
        .bytes()?;
    let feed = parser::parse(Cursor::new(bytes)).context("parse feed")?;
    let mut observations = Vec::new();
    for entry in feed.entries.into_iter().take(source.max_items) {
        let url = entry
            .links
            .first()
            .map(|link| link.href.clone())
            .unwrap_or_else(|| source.url.clone());
        observations.push(ObservationDraft {
            source_id: source.id.clone(),
            title: entry
                .title
                .map(|title| title.content)
                .unwrap_or_else(|| "Untitled".to_string()),
            url,
            description: entry.summary.map(|summary| summary.content),
            published_at: entry
                .published
                .or(entry.updated)
                .map(|date| date.to_rfc3339()),
            category: Some("News".to_string()),
            raw: serde_json::json!({"source_kind": source.kind}),
        });
    }
    Ok(observations)
}

fn collect_github_search(source: &SourceConfig) -> Result<Vec<ObservationDraft>> {
    let value: Value = http_client(source)?
        .get(&source.url)
        .send()?
        .error_for_status()?
        .json()?;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .take(source.max_items)
        .filter_map(|item| {
            let title = item
                .get("full_name")
                .or_else(|| item.get("name"))?
                .as_str()?
                .to_string();
            let url = item.get("html_url")?.as_str()?.to_string();
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            let updated = item
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(ObservationDraft {
                source_id: source.id.clone(),
                title,
                url,
                description,
                published_at: updated,
                category: Some("Open Source".to_string()),
                raw: serde_json::json!({
                    "stars": item.get("stargazers_count").cloned().unwrap_or(Value::Null),
                    "language": item.get("language").cloned().unwrap_or(Value::Null),
                    "source_kind": source.kind
                }),
            })
        })
        .collect())
}
