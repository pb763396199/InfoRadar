# InfoRadar

InfoRadar is a Rust-based offline intelligence daily generator.

It turns scattered sources into board-based daily briefings that are readable,
filterable, scoreable, traceable, and publishable as a static GitHub Pages site.

## v1 Scope

- Rust CLI generator, not a long-running server.
- SQLite as the canonical local store.
- GitHub Actions builds the daily issue.
- GitHub Pages publishes only the static `public/` output.
- `unreal` is the first board.

## Quick Start

```powershell
cargo run -p inforadar-cli -- import-techradar --from F:\AiProject\TechRadar
cargo run -p inforadar-cli -- build-issue --board unreal --date 2026-06-19
cargo run -p inforadar-cli -- build-site --all --out public
```

Then open `public/index.html`.
