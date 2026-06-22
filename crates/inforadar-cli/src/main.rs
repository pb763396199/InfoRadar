use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use inforadar_collectors::collect_source;
use inforadar_core::load_board_config;
use inforadar_site::build_site;
use inforadar_store::{SourceRunResultDraft, Store};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "inforadar")]
#[command(about = "Offline intelligence daily generator")]
struct Cli {
    #[arg(long, default_value = ".inforadar/inforadar.db")]
    db: PathBuf,
    #[arg(long, default_value = "configs/boards")]
    boards_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    ValidateConfig,
    ImportTechradar {
        #[arg(long)]
        from: PathBuf,
    },
    Collect {
        #[arg(long)]
        board: String,
        #[arg(long)]
        date: Option<String>,
    },
    BuildIssue {
        #[arg(long)]
        board: String,
        #[arg(long)]
        date: String,
    },
    BuildSite {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::ValidateConfig => {
            let boards = load_boards(&cli.boards_dir)?;
            println!("validated {} board(s)", boards.len());
        }
        Command::ImportTechradar { from } => {
            let board = load_board(&cli.boards_dir, "unreal")?;
            let store = Store::open(&cli.db)?;
            let count = store.import_techradar(from, &board)?;
            println!("imported {} observation(s)", count);
        }
        Command::Collect { board, date } => {
            let date = date.unwrap_or_else(|| Utc::now().date_naive().to_string());
            let config = load_board(&cli.boards_dir, &board)?;
            let store = Store::open(&cli.db)?;
            store.upsert_board(&config)?;
            let run_id = store.begin_run(&config.id, &date)?;
            let mut total = 0usize;
            let mut failures = Vec::new();
            for source in config.sources.iter().filter(|source| source.enabled) {
                match collect_source(source) {
                    Ok(observations) => {
                        let mut source_count = 0usize;
                        for observation in observations {
                            store.ingest_observation(&config, &run_id, &observation)?;
                            source_count += 1;
                            total += 1;
                        }
                        let status = if source_count == 0 {
                            "empty"
                        } else {
                            "success"
                        };
                        store.record_source_result(SourceRunResultDraft {
                            run_id: &run_id,
                            board_id: &config.id,
                            run_date: &date,
                            source_id: &source.id,
                            status,
                            count: source_count,
                            reason: "",
                        })?;
                    }
                    Err(err) => {
                        let reason = format!("{err:#}");
                        store.record_source_result(SourceRunResultDraft {
                            run_id: &run_id,
                            board_id: &config.id,
                            run_date: &date,
                            source_id: &source.id,
                            status: "failed",
                            count: 0,
                            reason: &reason,
                        })?;
                        failures.push(format!("{}: {reason}", source.id));
                    }
                }
            }
            let status = if failures.is_empty() {
                "success"
            } else {
                "partial"
            };
            let error = if failures.is_empty() {
                None
            } else {
                Some(failures.join("; "))
            };
            store.finish_run(&run_id, status, error.as_deref())?;
            store.build_issue(&config.id, &date)?;
            println!("collected {} observation(s), status {}", total, status);
            if let Some(error) = error {
                println!("failures: {}", error);
            }
        }
        Command::BuildIssue { board, date } => {
            let store = Store::open(&cli.db)?;
            let issue = store.build_issue(&board, &date)?;
            println!(
                "built issue {} {} with {} item(s)",
                issue.board_id,
                issue.issue_date,
                issue.items.len()
            );
        }
        Command::BuildSite { all, out } => {
            if !all {
                anyhow::bail!("v1 supports --all only");
            }
            let store = Store::open(&cli.db)?;
            let issues = store.issues()?;
            build_site(&issues, &out)?;
            store.record_publish_snapshot(&out.to_string_lossy())?;
            println!("built site with {} issue(s)", issues.len());
        }
    }
    Ok(())
}

fn load_board(boards_dir: &Path, id: &str) -> Result<inforadar_core::BoardConfig> {
    load_board_config(boards_dir.join(format!("{id}.toml")))
        .with_context(|| format!("load board {}", id))
}

fn load_boards(boards_dir: &Path) -> Result<Vec<inforadar_core::BoardConfig>> {
    let mut boards = Vec::new();
    for entry in
        std::fs::read_dir(boards_dir).with_context(|| format!("read {}", boards_dir.display()))?
    {
        let entry = entry?;
        if entry.path().extension().and_then(|value| value.to_str()) == Some("toml") {
            boards.push(load_board_config(entry.path())?);
        }
    }
    Ok(boards)
}
