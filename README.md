# Quartermaster CLI

Quartermaster is a Rust CLI for mapping a codebase into a local, browsable workspace.

Run it against a repository and it will:

- scan the project tree
- detect languages, frameworks, and tooling
- extract dependency relationships
- generate versioned developer docs in `./.quartermaster`
- open a local dashboard for exploring code, docs, notes, and graph data

## Quickstart

Build locally:

```bash
cargo build --release
```

Install from crates.io:

```bash
cargo install quartermaster-cli
```

Run the interactive flow in the current repository:

```bash
cargo run --bin quartermaster
```

Chart a repository directly:

```bash
quartermaster chart .
quartermaster chart github.com/rust-lang/rust
qm analyze .
```

## How It Works

Quartermaster writes a local workspace alongside the repository being analyzed:

```text
.quartermaster/
├── current.txt
├── notes/
└── versions/
    └── <version-id>/
        ├── manifest.json
        ├── README.md
        └── dev_docs/
```

The generated workspace separates durable notes from generated output:

- `notes/` is for human-authored notes that persist across runs
- `versions/<version-id>/` contains generated docs and manifests for a specific scan
- `current.txt` points the dashboard at the active generated version

## Commands

Start the interactive flow:

```bash
quartermaster
```

Analyze a local path or GitHub repository:

```bash
quartermaster chart .
quartermaster chart path/to/repo
quartermaster chart github.com/owner/repo
```

Common flags:

```bash
quartermaster chart . --no-open
quartermaster chart . --non-interactive
quartermaster chart . --include-root src,tests
quartermaster chart . --port 4310
quartermaster chart . --track-workspace
quartermaster chart . --no-gitignore
```

There is also a short alias binary:

```bash
qm analyze .
```

## Dashboard

After generation, Quartermaster can start a localhost server and open a browser dashboard.

The dashboard is designed around three content sources:

- embedded static UI assets bundled with the CLI
- generated workspace files from `./.quartermaster`
- repository files from the checked-out source tree

That makes the CLI self-contained at runtime while still letting the browser inspect the current repository and generated docs on one local origin.

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
├── docs/
└── scripts/
```

## Development

Useful local checks:

```bash
cargo fmt
cargo test
cargo package --list --allow-dirty
```

Further reading:

- Architecture: [docs/architecture.md](./docs/architecture.md)

## License

MIT
