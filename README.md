# Quartermaster CLI

Quartermaster is a Rust CLI that scans a repository, generates a local `./.quartermaster` workspace, and opens a browser dashboard for exploring code, docs, notes, and dependency relationships.

## What It Does

- Analyzes a local path or GitHub repository
- Detects languages, frameworks, and tooling
- Builds versioned developer docs in `./.quartermaster/versions/...`
- Preserves editable notes in `./.quartermaster/notes`
- Launches a local dashboard for browsing the generated workspace

## Installation

```bash
cargo install --path .
```

For local development:

```bash
cargo build --release
```

## Usage

Interactive flow:

```bash
quartermaster
```

Chart a repository directly:

```bash
quartermaster chart .
quartermaster chart github.com/rust-lang/rust
qm analyze .
```

Useful flags:

```bash
quartermaster chart . --no-open
quartermaster chart . --non-interactive
quartermaster chart . --include-root src,cli --port 4310
quartermaster chart . --track-workspace
```

## Standalone Packaging

This crate ships the dashboard runtime as embedded static assets compiled into the binary. The source tree for the dashboard can live elsewhere; the CLI only needs the built artifacts under `static/` at compile time.

Use the sync script after rebuilding dashboard assets in the parent workspace:

```bash
./scripts/sync-dashboard-artifacts.sh
```

## Repository Layout

```text
cli/
├── Cargo.toml
├── build.rs
├── src/
├── static/
│   ├── dashboard/index.html
│   ├── assets/...
│   └── glyph_logo.svg
├── scripts/
└── docs/
```

## Development

```bash
cargo fmt
cargo test
cargo package --list --allow-dirty
make docs
```

Architecture notes live in [docs/architecture.md](./docs/architecture.md).

## License

MIT
